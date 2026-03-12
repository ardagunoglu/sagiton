use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use backend::bootstrap::{app_state::AppState, config::AppConfig};
use backend::interfaces::http::router::build_router;
use futures_util::FutureExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{future::Future, panic::AssertUnwindSafe};
use tower::ServiceExt;
use uuid::Uuid;

async fn build_test_app(database_url: String) -> Router {
    let config = AppConfig {
        port: 18080,
        app_env: "test".to_string(),
        log_level: "info".to_string(),
        db_max_connections: 5,
        http_body_limit_bytes: 1024 * 1024,
        ws_max_message_bytes: 16_384,
        database_url,
        redis_url: "redis://localhost:6379".to_string(),
        jwt_secret: "test_secret_for_integration_tests_only".to_string(),
        refresh_token_pepper: "test_refresh_token_pepper".to_string(),
        docs_enabled: false,
    };

    let state = AppState::new(config)
        .await
        .expect("app state must initialize for tests");

    build_router(state)
}

async fn run_isolated_test<F, Fut>(test_fn: F)
where
    F: FnOnce(Router) -> Fut,
    Fut: Future<Output = ()>,
{
    let base_url = test_database_url();
    let schema = unique_schema_name();

    create_schema(&base_url, &schema).await;
    let isolated_url = add_search_path(&base_url, &schema);
    let app = build_test_app(isolated_url).await;

    let result = AssertUnwindSafe(test_fn(app)).catch_unwind().await;

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

async fn send_json(app: &Router, method: &str, uri: &str, payload: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .expect("request must build");

    let resp = app
        .clone()
        .oneshot(req)
        .await
        .expect("request should be processed");
    let status = resp.status();
    let body = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");

    let json: Value = serde_json::from_slice(&body).expect("response should be valid json");
    (status, json)
}

async fn send_authorized_json(
    app: &Router,
    method: &str,
    uri: &str,
    token: &str,
    payload: Value,
) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .expect("request must build");

    let resp = app
        .clone()
        .oneshot(req)
        .await
        .expect("request should be processed");
    let status = resp.status();
    let body = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");

    let json: Value = serde_json::from_slice(&body).expect("response should be valid json");
    (status, json)
}

async fn send_authorized_get(app: &Router, uri: &str, token: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .expect("request must build");

    let resp = app
        .clone()
        .oneshot(req)
        .await
        .expect("request should be processed");
    let status = resp.status();
    let body = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");

    let json: Value = serde_json::from_slice(&body).expect("response should be valid json");
    (status, json)
}

async fn register_and_login(app: &Router, prefix: &str) -> (String, Uuid) {
    let username = unique_username(prefix);

    let (register_status, register_body) = send_json(
        app,
        "POST",
        "/api/auth/register",
        json!({
            "username": username,
            "email": format!("{username}@example.com"),
            "password": "strong_password_123",
        }),
    )
    .await;
    assert_eq!(register_status, StatusCode::OK);

    let user_id = Uuid::parse_str(
        register_body["user"]["id"]
            .as_str()
            .expect("user id should exist"),
    )
    .expect("user id should be valid uuid");

    let (login_status, login_body) = send_json(
        app,
        "POST",
        "/api/auth/login",
        json!({
            "username_or_email": username,
            "password": "strong_password_123",
        }),
    )
    .await;
    assert_eq!(login_status, StatusCode::OK);

    let token = login_body["access_token"]
        .as_str()
        .expect("access token should exist")
        .to_string();

    (token, user_id)
}

#[tokio::test]
#[serial_test::serial]
async fn create_friend_request_endpoint_creates_pending_request() {
    run_isolated_test(|app| async move {
        let (alice_token, _alice_id) = register_and_login(&app, "friend_req_alice").await;
        let (_bob_token, bob_id) = register_and_login(&app, "friend_req_bob").await;

        let (status, body) = send_authorized_json(
            &app,
            "POST",
            "/api/friends/requests",
            &alice_token,
            json!({ "to_user_id": bob_id }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["request"]["status"], "PENDING");
        assert_eq!(body["request"]["to_user_id"], bob_id.to_string());
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn accept_friend_request_endpoint_creates_friendship_and_direct_thread_active() {
    run_isolated_test(|app| async move {
        let (alice_token, alice_id) = register_and_login(&app, "friend_acc_alice").await;
        let (bob_token, bob_id) = register_and_login(&app, "friend_acc_bob").await;

        let (request_status, _) = send_authorized_json(
            &app,
            "POST",
            "/api/friends/requests",
            &alice_token,
            json!({ "to_user_id": bob_id }),
        )
        .await;
        assert_eq!(request_status, StatusCode::OK);

        let (accept_status, accept_body) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/friends/requests/{alice_id}/accept"),
            &bob_token,
            json!({}),
        )
        .await;
        assert_eq!(accept_status, StatusCode::OK);
        assert_eq!(accept_body["friend"]["user_id"], alice_id.to_string());

        let (friends_status, friends_body) =
            send_authorized_get(&app, "/api/friends", &bob_token).await;
        assert_eq!(friends_status, StatusCode::OK);
        assert_eq!(
            friends_body["friends"]
                .as_array()
                .expect("friends should be array")
                .len(),
            1
        );

        let (direct_status, direct_body) = send_authorized_json(
            &app,
            "POST",
            "/api/direct-threads",
            &alice_token,
            json!({ "peer_user_id": bob_id }),
        )
        .await;
        assert_eq!(direct_status, StatusCode::OK);
        assert_eq!(direct_body["thread"]["status"], "ACTIVE");
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn reject_friend_request_endpoint_rejects_pending_request() {
    run_isolated_test(|app| async move {
        let (alice_token, alice_id) = register_and_login(&app, "friend_rej_alice").await;
        let (bob_token, bob_id) = register_and_login(&app, "friend_rej_bob").await;

        let (request_status, _) = send_authorized_json(
            &app,
            "POST",
            "/api/friends/requests",
            &alice_token,
            json!({ "to_user_id": bob_id }),
        )
        .await;
        assert_eq!(request_status, StatusCode::OK);

        let (reject_status, reject_body) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/friends/requests/{alice_id}/reject"),
            &bob_token,
            json!({}),
        )
        .await;
        assert_eq!(reject_status, StatusCode::OK);
        assert_eq!(reject_body["message"], "friend request rejected");

        let (friends_status, friends_body) =
            send_authorized_get(&app, "/api/friends", &bob_token).await;
        assert_eq!(friends_status, StatusCode::OK);
        assert_eq!(
            friends_body["friends"]
                .as_array()
                .expect("friends should be array")
                .len(),
            0
        );
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn list_friend_requests_endpoint_returns_pending_inbox_and_outbox() {
    run_isolated_test(|app| async move {
        let (alice_token, alice_id) = register_and_login(&app, "friend_req_list_alice").await;
        let (bob_token, bob_id) = register_and_login(&app, "friend_req_list_bob").await;

        let (request_status, _) = send_authorized_json(
            &app,
            "POST",
            "/api/friends/requests",
            &alice_token,
            json!({ "to_user_id": bob_id }),
        )
        .await;
        assert_eq!(request_status, StatusCode::OK);

        let (inbox_status, inbox_body) =
            send_authorized_get(&app, "/api/friends/requests?inbox=pending", &bob_token).await;
        assert_eq!(inbox_status, StatusCode::OK);
        assert_eq!(
            inbox_body["requests"]
                .as_array()
                .expect("requests should be array")
                .len(),
            1
        );
        assert_eq!(
            inbox_body["requests"][0]["from_user_id"],
            alice_id.to_string()
        );
        assert_eq!(inbox_body["requests"][0]["to_user_id"], bob_id.to_string());

        let (outbox_status, outbox_body) =
            send_authorized_get(&app, "/api/friends/requests?inbox=outbox", &alice_token).await;
        assert_eq!(outbox_status, StatusCode::OK);
        assert_eq!(
            outbox_body["requests"]
                .as_array()
                .expect("requests should be array")
                .len(),
            1
        );
        assert_eq!(
            outbox_body["requests"][0]["from_user_id"],
            alice_id.to_string()
        );
        assert_eq!(outbox_body["requests"][0]["to_user_id"], bob_id.to_string());
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn accept_friend_request_auto_activates_existing_pending_direct_thread() {
    run_isolated_test(|app| async move {
        let (alice_token, alice_id) = register_and_login(&app, "friend_dm_activate_alice").await;
        let (bob_token, bob_id) = register_and_login(&app, "friend_dm_activate_bob").await;

        let (create_dm_status, create_dm_body) = send_authorized_json(
            &app,
            "POST",
            "/api/direct-threads",
            &alice_token,
            json!({ "peer_user_id": bob_id }),
        )
        .await;
        assert_eq!(create_dm_status, StatusCode::OK);
        assert_eq!(create_dm_body["thread"]["status"], "PENDING");
        let thread_id = create_dm_body["thread"]["id"]
            .as_str()
            .expect("thread id should exist")
            .to_string();

        let (request_status, _) = send_authorized_json(
            &app,
            "POST",
            "/api/friends/requests",
            &alice_token,
            json!({ "to_user_id": bob_id }),
        )
        .await;
        assert_eq!(request_status, StatusCode::OK);

        let (accept_status, _) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/friends/requests/{alice_id}/accept"),
            &bob_token,
            json!({}),
        )
        .await;
        assert_eq!(accept_status, StatusCode::OK);

        let (get_dm_status, get_dm_body) = send_authorized_get(
            &app,
            &format!("/api/direct-threads/{thread_id}"),
            &bob_token,
        )
        .await;
        assert_eq!(get_dm_status, StatusCode::OK);
        assert_eq!(get_dm_body["thread"]["status"], "ACTIVE");

        let (send_status, _) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/threads/{thread_id}/messages"),
            &bob_token,
            json!({ "content": "active after friend accept" }),
        )
        .await;
        assert_eq!(send_status, StatusCode::OK);
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn list_friends_endpoint_returns_bidirectional_friendship() {
    run_isolated_test(|app| async move {
        let (alice_token, alice_id) = register_and_login(&app, "friend_list_alice").await;
        let (bob_token, bob_id) = register_and_login(&app, "friend_list_bob").await;

        let _ = send_authorized_json(
            &app,
            "POST",
            "/api/friends/requests",
            &alice_token,
            json!({ "to_user_id": bob_id }),
        )
        .await;

        let _ = send_authorized_json(
            &app,
            "POST",
            &format!("/api/friends/requests/{alice_id}/accept"),
            &bob_token,
            json!({}),
        )
        .await;

        let (alice_list_status, alice_list_body) =
            send_authorized_get(&app, "/api/friends", &alice_token).await;
        assert_eq!(alice_list_status, StatusCode::OK);
        assert_eq!(alice_list_body["friends"][0]["user_id"], bob_id.to_string());

        let (bob_list_status, bob_list_body) =
            send_authorized_get(&app, "/api/friends", &bob_token).await;
        assert_eq!(bob_list_status, StatusCode::OK);
        assert_eq!(bob_list_body["friends"][0]["user_id"], alice_id.to_string());
    })
    .await;
}
