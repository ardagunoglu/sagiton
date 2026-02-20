use axum::Router;
use backend::bootstrap::{app_state::AppState, config::AppConfig};
use backend::interfaces::http::router::build_router;
use backend::interfaces::ws::protocol::{PresenceStatus, ServerEvent};
use futures_util::{FutureExt, SinkExt, StreamExt};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{future::Future, panic::AssertUnwindSafe, time::Duration};
use tokio::net::TcpListener;
use tokio::time::Instant;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use uuid::Uuid;

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn build_test_app(database_url: String) -> Router {
    let config = AppConfig {
        port: 0,
        app_env: "test".to_string(),
        log_level: "info".to_string(),
        db_max_connections: 5,
        http_body_limit_bytes: 1024 * 1024,
        ws_max_message_bytes: 16_384,
        database_url,
        redis_url: "redis://localhost:6379".to_string(),
        jwt_secret: "test_secret_for_integration_tests_only".to_string(),
        refresh_token_pepper: "test_refresh_token_pepper".to_string(),
    };

    let state = AppState::new(config)
        .await
        .expect("app state must initialize for tests");

    build_router(state)
}

async fn run_isolated_ws_test<F, Fut>(test_fn: F)
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = ()>,
{
    let base_url = test_database_url();
    let schema = unique_schema_name();

    create_schema(&base_url, &schema).await;
    let isolated_db_url = add_search_path(&base_url, &schema);

    let app = build_test_app(isolated_db_url).await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("must bind tcp listener");
    let addr = listener.local_addr().expect("must get local addr");

    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let http_base = format!("http://{}", addr);
    let result = AssertUnwindSafe(test_fn(http_base)).catch_unwind().await;

    server.abort();
    let _ = server.await;

    drop_schema(&base_url, &schema).await;

    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

async fn create_schema(database_url: &str, schema: &str) {
    let pool = PgPool::connect(database_url)
        .await
        .expect("must connect to create schema");

    let sql = format!("CREATE SCHEMA IF NOT EXISTS \"{schema}\"");
    sqlx::query(&sql)
        .execute(&pool)
        .await
        .expect("must create test schema");
}

async fn drop_schema(database_url: &str, schema: &str) {
    let pool = PgPool::connect(database_url)
        .await
        .expect("must connect to drop schema");

    let sql = format!("DROP SCHEMA IF EXISTS \"{schema}\" CASCADE");
    sqlx::query(&sql)
        .execute(&pool)
        .await
        .expect("must drop test schema");
}

fn test_database_url() -> String {
    dotenvy::dotenv().ok();

    std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("set TEST_DATABASE_URL or DATABASE_URL before running tests")
}

fn unique_schema_name() -> String {
    format!("it_{}", Uuid::new_v4().simple())
}

fn add_search_path(base_url: &str, schema: &str) -> String {
    let encoded = format!("-csearch_path%3D{schema}");
    if base_url.contains('?') {
        format!("{base_url}&options={encoded}")
    } else {
        format!("{base_url}?options={encoded}")
    }
}

fn unique_username(prefix: &str) -> String {
    let suffix = Uuid::new_v4().simple().to_string();
    let short_suffix = &suffix[..8];
    let max_prefix_len = 23;
    let trimmed_prefix = if prefix.len() > max_prefix_len {
        &prefix[..max_prefix_len]
    } else {
        prefix
    };
    format!("{trimmed_prefix}_{short_suffix}")
}

async fn register_and_login_http(
    client: &reqwest::Client,
    base: &str,
    prefix: &str,
) -> (String, Uuid) {
    let username = unique_username(prefix);

    let register_resp = client
        .post(format!("{base}/api/auth/register"))
        .json(&json!({
            "username": username,
            "email": format!("{username}@example.com"),
            "password": "strong_password_123"
        }))
        .send()
        .await
        .expect("register request failed");
    assert_eq!(register_resp.status(), reqwest::StatusCode::OK);
    let register_json: Value = register_resp
        .json()
        .await
        .expect("register json should parse");
    let user_id = Uuid::parse_str(
        register_json["user"]["id"]
            .as_str()
            .expect("user id should exist"),
    )
    .expect("user id should be uuid");

    let login_resp = client
        .post(format!("{base}/api/auth/login"))
        .json(&json!({
            "username_or_email": username,
            "password": "strong_password_123"
        }))
        .send()
        .await
        .expect("login request failed");
    assert_eq!(login_resp.status(), reqwest::StatusCode::OK);

    let login_json: Value = login_resp.json().await.expect("login json should parse");
    (
        login_json["access_token"]
            .as_str()
            .expect("access token should exist")
            .to_string(),
        user_id,
    )
}

async fn create_channel_http(client: &reqwest::Client, base: &str, token: &str) -> Uuid {
    let guild_resp = client
        .post(format!("{base}/api/guilds"))
        .bearer_auth(token)
        .json(&json!({"name":"Realtime Guild"}))
        .send()
        .await
        .expect("create guild failed");
    assert_eq!(guild_resp.status(), reqwest::StatusCode::OK);

    let guild_json: Value = guild_resp.json().await.expect("guild json should parse");
    let guild_id = guild_json["guild"]["id"]
        .as_str()
        .expect("guild id should exist")
        .to_string();

    let channel_resp = client
        .post(format!("{base}/api/guilds/{guild_id}/channels"))
        .bearer_auth(token)
        .json(&json!({"name":"general"}))
        .send()
        .await
        .expect("create channel failed");
    assert_eq!(channel_resp.status(), reqwest::StatusCode::OK);

    let channel_json: Value = channel_resp
        .json()
        .await
        .expect("channel json should parse");
    Uuid::parse_str(
        channel_json["channel"]["id"]
            .as_str()
            .expect("channel id should exist"),
    )
    .expect("channel id should be uuid")
}

async fn create_direct_thread_http(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    peer_user_id: Uuid,
) -> Uuid {
    let resp = client
        .post(format!("{base}/api/direct-threads"))
        .bearer_auth(token)
        .json(&json!({ "peer_user_id": peer_user_id }))
        .send()
        .await
        .expect("create direct thread failed");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    let body: Value = resp.json().await.expect("direct thread json should parse");
    Uuid::parse_str(
        body["thread"]["id"]
            .as_str()
            .expect("thread id should exist"),
    )
    .expect("thread id should be uuid")
}

async fn force_disconnect_pubsub_clients() {
    let client = redis::Client::open("redis://localhost:6379").expect("redis client should open");
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .expect("redis connection should open");

    let _killed: i64 = redis::cmd("CLIENT")
        .arg("KILL")
        .arg("TYPE")
        .arg("PUBSUB")
        .query_async(&mut conn)
        .await
        .expect("redis CLIENT KILL should work");
}

async fn fetch_metrics_payload(client: &reqwest::Client, base: &str) -> String {
    let response = client
        .get(format!("{base}/metrics"))
        .send()
        .await
        .expect("metrics request should succeed");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    response.text().await.expect("metrics payload should read")
}

fn metric_value(payload: &str, metric_name: &str) -> Option<f64> {
    for line in payload.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let mut parts = trimmed.split_whitespace();
        let name = parts.next()?;
        if name != metric_name {
            continue;
        }
        let value = parts.next()?;
        return value.parse::<f64>().ok();
    }

    None
}

async fn local_redis_connection() -> redis::aio::MultiplexedConnection {
    let client = redis::Client::open("redis://localhost:6379").expect("redis client should open");
    client
        .get_multiplexed_async_connection()
        .await
        .expect("redis connection should open")
}

async fn presence_ttl_seconds(user_id: Uuid) -> Option<i64> {
    let key = format!("presence:user:{user_id}");
    let mut conn = local_redis_connection().await;
    let ttl: i64 = redis::cmd("TTL")
        .arg(key)
        .query_async(&mut conn)
        .await
        .expect("TTL command should work");

    if ttl < 0 { None } else { Some(ttl) }
}

async fn presence_key_exists(user_id: Uuid) -> bool {
    let key = format!("presence:user:{user_id}");
    let mut conn = local_redis_connection().await;
    let exists: i64 = redis::cmd("EXISTS")
        .arg(key)
        .query_async(&mut conn)
        .await
        .expect("EXISTS command should work");
    exists > 0
}

async fn recv_event_within(ws: &mut WsStream, timeout: Duration) -> Option<ServerEvent> {
    let frame = tokio::time::timeout(timeout, ws.next()).await.ok()??.ok()?;

    match frame {
        Message::Text(txt) => serde_json::from_str::<ServerEvent>(&txt).ok(),
        _ => None,
    }
}

async fn wait_for_event<F>(ws: &mut WsStream, timeout: Duration, matcher: F) -> Option<ServerEvent>
where
    F: Fn(&ServerEvent) -> bool,
{
    let deadline = Instant::now() + timeout;

    loop {
        let now = Instant::now();
        if now >= deadline {
            return None;
        }

        let remaining = deadline - now;
        let step = remaining.min(Duration::from_millis(300));
        let Some(event) = recv_event_within(ws, step).await else {
            continue;
        };

        if matcher(&event) {
            return Some(event);
        }
    }
}

async fn no_matching_event<F>(ws: &mut WsStream, duration: Duration, matcher: F) -> bool
where
    F: Fn(&ServerEvent) -> bool,
{
    wait_for_event(ws, duration, matcher).await.is_none()
}

async fn expect_ready(ws: &mut WsStream) {
    let ready = wait_for_event(ws, Duration::from_secs(5), |event| {
        matches!(event, ServerEvent::Ready { .. })
    })
    .await;
    assert!(ready.is_some(), "expected READY event");
}

async fn subscribe_channel_and_wait_ack(ws: &mut WsStream, channel_id: Uuid) {
    ws.send(Message::Text(
        json!({
            "op": "SUBSCRIBE_CHANNEL",
            "d": { "channel_id": channel_id }
        })
        .to_string()
        .into(),
    ))
    .await
    .expect("subscribe frame should send");

    let ack = wait_for_event(ws, Duration::from_secs(5), |event| {
        matches!(
            event,
            ServerEvent::ChannelSubscribeAck { channel_id: cid } if *cid == channel_id
        )
    })
    .await;
    assert!(ack.is_some(), "expected CHANNEL_SUBSCRIBE_ACK event");
}

async fn subscribe_thread_and_wait_ack(ws: &mut WsStream, thread_id: Uuid) {
    ws.send(Message::Text(
        json!({
            "op": "SUBSCRIBE_THREAD",
            "d": { "thread_id": thread_id }
        })
        .to_string()
        .into(),
    ))
    .await
    .expect("subscribe thread frame should send");

    let ack = wait_for_event(ws, Duration::from_secs(5), |event| {
        matches!(
            event,
            ServerEvent::ThreadSubscribeAck { thread_id: tid } if *tid == thread_id
        )
    })
    .await;
    assert!(ack.is_some(), "expected THREAD_SUBSCRIBE_ACK event");
}

async fn wait_until_true<F, Fut>(timeout: Duration, poll_every: Duration, mut check: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let deadline = Instant::now() + timeout;
    loop {
        if check().await {
            return true;
        }

        let now = Instant::now();
        if now >= deadline {
            return false;
        }

        let remaining = deadline - now;
        tokio::time::sleep(remaining.min(poll_every)).await;
    }
}

#[tokio::test]
#[serial_test::serial]
async fn message_create_http_emits_ws_message_create_event_once() {
    run_isolated_ws_test(|base| async move {
        let client = reqwest::Client::new();
        let (token, _) = register_and_login_http(&client, &base, "ws_msg_owner").await;
        let channel_id = create_channel_http(&client, &base, &token).await;

        let ws_url = format!("{}/ws?token={}", base.replace("http", "ws"), token);
        let (mut ws, _) = connect_async(ws_url).await.expect("ws connect should work");
        expect_ready(&mut ws).await;
        subscribe_channel_and_wait_ack(&mut ws, channel_id).await;

        let msg_resp = client
            .post(format!("{base}/api/channels/{channel_id}/messages"))
            .bearer_auth(&token)
            .json(&json!({"content":"realtime hello"}))
            .send()
            .await
            .expect("create message request failed");
        assert_eq!(msg_resp.status(), reqwest::StatusCode::OK);
        let msg_body: Value = msg_resp
            .json()
            .await
            .expect("message create json should parse");
        let message_id = msg_body["message"]["id"]
            .as_str()
            .expect("message id should exist")
            .to_string();

        let received = wait_for_event(&mut ws, Duration::from_secs(5), |event| {
            matches!(
                event,
                ServerEvent::MessageCreate { channel_id: cid, message }
                if *cid == channel_id && message.content == "realtime hello"
            )
        })
        .await;
        assert!(received.is_some(), "expected MESSAGE_CREATE event");

        let no_duplicate = no_matching_event(&mut ws, Duration::from_secs(1), |event| {
            matches!(
                event,
                ServerEvent::MessageCreate { channel_id: cid, message }
                if *cid == channel_id && message.content == "realtime hello"
            )
        })
        .await;
        assert!(no_duplicate, "duplicate MESSAGE_CREATE event detected");

        let update_resp = client
            .patch(format!("{base}/api/messages/{message_id}"))
            .bearer_auth(&token)
            .json(&json!({"content":"realtime edited"}))
            .send()
            .await
            .expect("update message request failed");
        assert_eq!(update_resp.status(), reqwest::StatusCode::OK);

        let update_event = wait_for_event(&mut ws, Duration::from_secs(5), |event| {
            matches!(
                event,
                ServerEvent::MessageUpdate { channel_id: cid, message }
                if *cid == channel_id
                    && message.id.to_string() == message_id
                    && message.content == "realtime edited"
            )
        })
        .await;
        assert!(update_event.is_some(), "expected MESSAGE_UPDATE event");

        let no_update_duplicate = no_matching_event(&mut ws, Duration::from_secs(1), |event| {
            matches!(
                event,
                ServerEvent::MessageUpdate { channel_id: cid, message }
                if *cid == channel_id
                    && message.id.to_string() == message_id
                    && message.content == "realtime edited"
            )
        })
        .await;
        assert!(
            no_update_duplicate,
            "duplicate MESSAGE_UPDATE event detected"
        );

        let delete_resp = client
            .delete(format!("{base}/api/messages/{message_id}"))
            .bearer_auth(&token)
            .json(&json!({}))
            .send()
            .await
            .expect("delete message request failed");
        assert_eq!(delete_resp.status(), reqwest::StatusCode::OK);

        let delete_event = wait_for_event(&mut ws, Duration::from_secs(5), |event| {
            matches!(
                event,
                ServerEvent::MessageDelete { channel_id: cid, message_id: mid, .. }
                if *cid == channel_id && mid.to_string() == message_id
            )
        })
        .await;
        assert!(delete_event.is_some(), "expected MESSAGE_DELETE event");

        let no_delete_duplicate = no_matching_event(&mut ws, Duration::from_secs(1), |event| {
            matches!(
                event,
                ServerEvent::MessageDelete { channel_id: cid, message_id: mid, .. }
                if *cid == channel_id && mid.to_string() == message_id
            )
        })
        .await;
        assert!(
            no_delete_duplicate,
            "duplicate MESSAGE_DELETE event detected"
        );
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn redis_pubsub_listener_reconnects_after_forced_disconnect() {
    run_isolated_ws_test(|base| async move {
        let client = reqwest::Client::new();
        let (token, _) = register_and_login_http(&client, &base, "ws_reconnect_owner").await;
        let channel_id = create_channel_http(&client, &base, &token).await;

        let ws_url = format!("{}/ws?token={}", base.replace("http", "ws"), token);
        let (mut ws, _) = connect_async(ws_url).await.expect("ws connect should work");
        expect_ready(&mut ws).await;
        subscribe_channel_and_wait_ack(&mut ws, channel_id).await;

        let first_resp = client
            .post(format!("{base}/api/channels/{channel_id}/messages"))
            .bearer_auth(&token)
            .json(&json!({"content":"before reconnect"}))
            .send()
            .await
            .expect("first message request failed");
        assert_eq!(first_resp.status(), reqwest::StatusCode::OK);

        let first_event = wait_for_event(&mut ws, Duration::from_secs(5), |event| {
            matches!(
                event,
                ServerEvent::MessageCreate { channel_id: cid, message }
                if *cid == channel_id && message.content == "before reconnect"
            )
        })
        .await;
        assert!(first_event.is_some(), "first event should arrive");

        let reconnects_before = metric_value(
            &fetch_metrics_payload(&client, &base).await,
            "redis_reconnect_total",
        )
        .unwrap_or(0.0);
        force_disconnect_pubsub_clients().await;
        let reconnected =
            wait_until_true(Duration::from_secs(8), Duration::from_millis(150), || {
                let client = client.clone();
                let base = base.clone();
                async move {
                    let payload = fetch_metrics_payload(&client, &base).await;
                    metric_value(&payload, "redis_reconnect_total").unwrap_or(0.0)
                        > reconnects_before
                }
            })
            .await;
        assert!(
            reconnected,
            "redis reconnect metric should increase after forced disconnect"
        );

        let second_resp = client
            .post(format!("{base}/api/channels/{channel_id}/messages"))
            .bearer_auth(&token)
            .json(&json!({"content":"after reconnect"}))
            .send()
            .await
            .expect("second message request failed");
        assert_eq!(second_resp.status(), reqwest::StatusCode::OK);

        let second_event = wait_for_event(&mut ws, Duration::from_secs(8), |event| {
            matches!(
                event,
                ServerEvent::MessageCreate { channel_id: cid, message }
                if *cid == channel_id && message.content == "after reconnect"
            )
        })
        .await;
        assert!(
            second_event.is_some(),
            "event should continue after redis pubsub reconnect"
        );

        let metrics_payload = fetch_metrics_payload(&client, &base).await;
        let reconnects = metric_value(&metrics_payload, "redis_reconnect_total")
            .expect("redis_reconnect_total metric should exist");
        assert!(
            reconnects >= 1.0,
            "redis_reconnect_total should increase after forced disconnect"
        );
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn unauthorized_ws_subscription_receives_no_channel_message_event() {
    run_isolated_ws_test(|base| async move {
        let client = reqwest::Client::new();

        let (owner_token, _) = register_and_login_http(&client, &base, "ws_owner").await;
        let (outsider_token, _) = register_and_login_http(&client, &base, "ws_outsider").await;
        let channel_id = create_channel_http(&client, &base, &owner_token).await;

        let ws_url = format!("{}/ws?token={}", base.replace("http", "ws"), outsider_token);
        let (mut ws, _) = connect_async(ws_url).await.expect("ws connect should work");
        expect_ready(&mut ws).await;

        ws.send(Message::Text(
            json!({
                "op": "SUBSCRIBE_CHANNEL",
                "d": { "channel_id": channel_id }
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("subscribe frame should send");
        let subscribe_error = wait_for_event(&mut ws, Duration::from_secs(5), |event| {
            matches!(
                event,
                ServerEvent::Error { code, .. } if code == "FORBIDDEN" || code == "SUBSCRIBE_FAILED"
            )
        })
        .await;
        assert!(
            subscribe_error.is_some(),
            "outsider subscribe should return an error event"
        );

        let _ = client
            .post(format!("{base}/api/channels/{channel_id}/messages"))
            .bearer_auth(&owner_token)
            .json(&json!({"content":"owner message"}))
            .send()
            .await
            .expect("owner message request failed");

        let got_event = wait_for_event(&mut ws, Duration::from_secs(2), |event| {
            matches!(
                event,
                ServerEvent::MessageCreate { channel_id: cid, .. } if *cid == channel_id
            )
        })
        .await;
        assert!(
            got_event.is_none(),
            "outsider websocket must not receive channel message events"
        );
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn thread_message_create_http_emits_ws_thread_message_create_event_once() {
    run_isolated_ws_test(|base| async move {
        let client = reqwest::Client::new();
        let (owner_token, _) = register_and_login_http(&client, &base, "ws_thread_owner").await;
        let (_peer_token, peer_id) =
            register_and_login_http(&client, &base, "ws_thread_peer").await;
        let thread_id = create_direct_thread_http(&client, &base, &owner_token, peer_id).await;

        let ws_url = format!("{}/ws?token={}", base.replace("http", "ws"), owner_token);
        let (mut ws, _) = connect_async(ws_url).await.expect("ws connect should work");
        expect_ready(&mut ws).await;
        subscribe_thread_and_wait_ack(&mut ws, thread_id).await;

        let resp = client
            .post(format!("{base}/api/threads/{thread_id}/messages"))
            .bearer_auth(&owner_token)
            .json(&json!({"content":"thread realtime hello"}))
            .send()
            .await
            .expect("create thread message request failed");
        assert_eq!(resp.status(), reqwest::StatusCode::OK);

        let received = wait_for_event(&mut ws, Duration::from_secs(5), |event| {
            matches!(
                event,
                ServerEvent::ThreadMessageCreate { thread_id: tid, message }
                if *tid == thread_id && message.content == "thread realtime hello"
            )
        })
        .await;
        assert!(received.is_some(), "expected THREAD_MESSAGE_CREATE event");

        let no_duplicate = no_matching_event(&mut ws, Duration::from_secs(1), |event| {
            matches!(
                event,
                ServerEvent::ThreadMessageCreate { thread_id: tid, message }
                if *tid == thread_id && message.content == "thread realtime hello"
            )
        })
        .await;
        assert!(
            no_duplicate,
            "duplicate THREAD_MESSAGE_CREATE event detected"
        );
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn unauthorized_ws_thread_subscription_receives_no_thread_message_event() {
    run_isolated_ws_test(|base| async move {
        let client = reqwest::Client::new();

        let (owner_token, _) = register_and_login_http(&client, &base, "ws_thread_owner2").await;
        let (_peer_token, peer_id) =
            register_and_login_http(&client, &base, "ws_thread_peer2").await;
        let (outsider_token, _) =
            register_and_login_http(&client, &base, "ws_thread_outsider").await;

        let thread_id = create_direct_thread_http(&client, &base, &owner_token, peer_id).await;

        let ws_url = format!("{}/ws?token={}", base.replace("http", "ws"), outsider_token);
        let (mut ws, _) = connect_async(ws_url).await.expect("ws connect should work");
        expect_ready(&mut ws).await;

        ws.send(Message::Text(
            json!({
                "op": "SUBSCRIBE_THREAD",
                "d": { "thread_id": thread_id }
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("subscribe thread frame should send");
        let subscribe_error = wait_for_event(&mut ws, Duration::from_secs(5), |event| {
            matches!(
                event,
                ServerEvent::Error { code, .. } if code == "FORBIDDEN"
            )
        })
        .await;
        assert!(
            subscribe_error.is_some(),
            "outsider thread subscribe should return FORBIDDEN error event"
        );

        let _ = client
            .post(format!("{base}/api/threads/{thread_id}/messages"))
            .bearer_auth(&owner_token)
            .json(&json!({"content":"owner thread message"}))
            .send()
            .await
            .expect("owner thread message request failed");

        let got_event = wait_for_event(&mut ws, Duration::from_secs(2), |event| {
            matches!(
                event,
                ServerEvent::ThreadMessageCreate { thread_id: tid, .. } if *tid == thread_id
            )
        })
        .await;
        assert!(
            got_event.is_none(),
            "outsider websocket must not receive thread message events"
        );
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn presence_offline_published_only_after_last_connection_closes() {
    run_isolated_ws_test(|base| async move {
        let client = reqwest::Client::new();
        let (monitor_token, _monitor_user) =
            register_and_login_http(&client, &base, "ws_presence_monitor").await;
        let (target_token, target_user_id) =
            register_and_login_http(&client, &base, "ws_presence_target").await;

        let monitor_url = format!("{}/ws?token={}", base.replace("http", "ws"), monitor_token);
        let (mut monitor_ws, _) = connect_async(monitor_url)
            .await
            .expect("monitor ws connect should work");
        expect_ready(&mut monitor_ws).await;

        let target_url = format!("{}/ws?token={}", base.replace("http", "ws"), target_token);
        let (mut target_ws1, _) = connect_async(target_url.clone())
            .await
            .expect("target ws1 connect should work");
        expect_ready(&mut target_ws1).await;

        let online_event = wait_for_event(&mut monitor_ws, Duration::from_secs(5), |event| {
            matches!(
                event,
                ServerEvent::PresenceUpdate { user_id, status }
                if *user_id == target_user_id && matches!(status, PresenceStatus::Online)
            )
        })
        .await;
        assert!(online_event.is_some(), "expected online presence update");

        let (mut target_ws2, _) = connect_async(target_url)
            .await
            .expect("target ws2 connect should work");
        expect_ready(&mut target_ws2).await;

        let second_online = wait_for_event(&mut monitor_ws, Duration::from_millis(900), |event| {
            matches!(
                event,
                ServerEvent::PresenceUpdate { user_id, status }
                if *user_id == target_user_id && matches!(status, PresenceStatus::Online)
            )
        })
        .await;
        assert!(
            second_online.is_none(),
            "second connection must not trigger extra online event"
        );

        target_ws1
            .close(None)
            .await
            .expect("closing first socket should succeed");
        let key_still_exists = wait_until_true(
            Duration::from_secs(2),
            Duration::from_millis(100),
            || async { presence_key_exists(target_user_id).await },
        )
        .await;
        assert!(
            key_still_exists,
            "presence key must remain while one device is still connected"
        );
        let first_close_offline =
            wait_for_event(&mut monitor_ws, Duration::from_millis(1200), |event| {
                matches!(
                    event,
                    ServerEvent::PresenceUpdate { user_id, status }
                    if *user_id == target_user_id && matches!(status, PresenceStatus::Offline)
                )
            })
            .await;
        assert!(
            first_close_offline.is_none(),
            "offline event must not publish while another connection is still alive"
        );

        target_ws2
            .close(None)
            .await
            .expect("closing second socket should succeed");
        let second_close_offline =
            wait_for_event(&mut monitor_ws, Duration::from_secs(5), |event| {
                matches!(
                    event,
                    ServerEvent::PresenceUpdate { user_id, status }
                    if *user_id == target_user_id && matches!(status, PresenceStatus::Offline)
                )
            })
            .await;
        assert!(
            second_close_offline.is_some(),
            "offline event must publish after last connection closes"
        );

        let cleared = wait_until_true(
            Duration::from_secs(2),
            Duration::from_millis(100),
            || async { !presence_key_exists(target_user_id).await },
        )
        .await;
        assert!(
            cleared,
            "presence key must be deleted after final disconnect"
        );
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn heartbeat_refreshes_presence_ttl() {
    run_isolated_ws_test(|base| async move {
        let client = reqwest::Client::new();
        let (token, user_id) = register_and_login_http(&client, &base, "ws_ttl_user").await;

        let ws_url = format!("{}/ws?token={}", base.replace("http", "ws"), token);
        let (mut ws, _) = connect_async(ws_url).await.expect("ws connect should work");
        expect_ready(&mut ws).await;

        let initial_ttl = presence_ttl_seconds(user_id)
            .await
            .expect("presence ttl should exist after connect");
        let ttl_decreased = wait_until_true(
            Duration::from_secs(3),
            Duration::from_millis(100),
            || async {
                let Some(current_ttl) = presence_ttl_seconds(user_id).await else {
                    return false;
                };
                current_ttl < initial_ttl
            },
        )
        .await;
        assert!(
            ttl_decreased,
            "ttl should decrease before heartbeat refresh"
        );
        let before_heartbeat_ttl = presence_ttl_seconds(user_id)
            .await
            .expect("presence ttl should still exist");
        assert!(
            before_heartbeat_ttl < initial_ttl,
            "ttl should naturally decrease before heartbeat refresh"
        );

        ws.send(Message::Text(
            json!({
                "op": "HEARTBEAT",
                "d": { "ts": 12345 }
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("heartbeat frame should send");

        let ack = wait_for_event(
            &mut ws,
            Duration::from_secs(5),
            |event| matches!(event, ServerEvent::HeartbeatAck { ts } if *ts == 12345),
        )
        .await;
        assert!(ack.is_some(), "heartbeat ack should arrive");

        let refreshed_ttl = presence_ttl_seconds(user_id)
            .await
            .expect("presence ttl should exist after heartbeat");
        assert!(
            refreshed_ttl > before_heartbeat_ttl,
            "heartbeat must refresh redis presence ttl"
        );

        ws.close(None)
            .await
            .expect("closing websocket should succeed");

        let cleared = wait_until_true(
            Duration::from_secs(2),
            Duration::from_millis(100),
            || async { !presence_key_exists(user_id).await },
        )
        .await;
        assert!(cleared, "presence key should be deleted on disconnect");
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn dm_request_and_accept_events_are_delivered_over_ws() {
    run_isolated_ws_test(|base| async move {
        let client = reqwest::Client::new();
        let (requester_token, requester_user_id) =
            register_and_login_http(&client, &base, "ws_dm_requester").await;
        let (receiver_token, receiver_user_id) =
            register_and_login_http(&client, &base, "ws_dm_receiver").await;

        let receiver_ws_url = format!("{}/ws?token={}", base.replace("http", "ws"), receiver_token);
        let (mut receiver_ws, _) = connect_async(receiver_ws_url)
            .await
            .expect("receiver ws connect should work");
        expect_ready(&mut receiver_ws).await;

        let thread_id =
            create_direct_thread_http(&client, &base, &requester_token, receiver_user_id).await;

        let request_event = wait_for_event(&mut receiver_ws, Duration::from_secs(5), |event| {
            matches!(
                event,
                ServerEvent::ThreadRequestReceived {
                    thread_id: tid,
                    requester_user_id: req,
                    receiver_user_id: recv
                } if *tid == thread_id && *req == requester_user_id && *recv == receiver_user_id
            )
        })
        .await;
        assert!(
            request_event.is_some(),
            "receiver must get THREAD_REQUEST_RECEIVED event"
        );

        let accept_resp = client
            .post(format!("{base}/api/direct-threads/{thread_id}/accept"))
            .bearer_auth(&receiver_token)
            .json(&json!({}))
            .send()
            .await
            .expect("accept request failed");
        assert_eq!(accept_resp.status(), reqwest::StatusCode::OK);

        let accepted_event = wait_for_event(&mut receiver_ws, Duration::from_secs(5), |event| {
            matches!(
                event,
                ServerEvent::ThreadAccepted {
                    thread_id: tid,
                    accepted_by_user_id,
                    requester_user_id: req,
                    receiver_user_id: recv
                } if *tid == thread_id
                    && *accepted_by_user_id == receiver_user_id
                    && *req == requester_user_id
                    && *recv == receiver_user_id
            )
        })
        .await;
        assert!(
            accepted_event.is_some(),
            "receiver must get THREAD_ACCEPTED"
        );

        subscribe_thread_and_wait_ack(&mut receiver_ws, thread_id).await;

        let msg_resp = client
            .post(format!("{base}/api/threads/{thread_id}/messages"))
            .bearer_auth(&requester_token)
            .json(&json!({"content":"dm after accept"}))
            .send()
            .await
            .expect("thread message create request failed");
        assert_eq!(msg_resp.status(), reqwest::StatusCode::OK);

        let create_event = wait_for_event(&mut receiver_ws, Duration::from_secs(5), |event| {
            matches!(
                event,
                ServerEvent::ThreadMessageCreate { thread_id: tid, message }
                if *tid == thread_id && message.content == "dm after accept"
            )
        })
        .await;
        assert!(
            create_event.is_some(),
            "receiver should receive thread message event after accept"
        );
    })
    .await;
}
