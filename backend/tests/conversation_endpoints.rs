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
    };

    let state = AppState::new(config)
        .await
        .expect("app state must initialize for tests");

    build_router(state)
}

async fn run_isolated_test<F, Fut>(test_fn: F)
where
    F: FnOnce(Router, String) -> Fut,
    Fut: Future<Output = ()>,
{
    let base_url = test_database_url();
    let schema = unique_schema_name();

    create_schema(&base_url, &schema).await;
    let isolated_url = add_search_path(&base_url, &schema);
    let app = build_test_app(isolated_url.clone()).await;

    let result = AssertUnwindSafe(test_fn(app, isolated_url))
        .catch_unwind()
        .await;

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

    let access_token = login_body["access_token"]
        .as_str()
        .expect("access token should exist")
        .to_string();

    (access_token, user_id)
}

async fn create_direct_thread_http(
    app: &Router,
    token: &str,
    peer_user_id: Uuid,
) -> (Uuid, String, bool) {
    let (status, body) = send_authorized_json(
        app,
        "POST",
        "/api/direct-threads",
        token,
        json!({ "peer_user_id": peer_user_id }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let thread_id = Uuid::parse_str(
        body["thread"]["id"]
            .as_str()
            .expect("thread id should exist"),
    )
    .expect("thread id should be valid uuid");
    let status = body["thread"]["status"]
        .as_str()
        .expect("thread status should exist")
        .to_string();
    let created = body["created"]
        .as_bool()
        .expect("created flag should exist");
    (thread_id, status, created)
}

async fn create_group_thread_http(app: &Router, token: &str, name: &str, members: &[Uuid]) -> Uuid {
    let (status, body) = send_authorized_json(
        app,
        "POST",
        "/api/group-threads",
        token,
        json!({ "name": name, "member_user_ids": members }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    Uuid::parse_str(
        body["thread"]["id"]
            .as_str()
            .expect("thread id should exist"),
    )
    .expect("thread id should be valid uuid")
}

async fn insert_friendship(database_url: &str, user_a: Uuid, user_b: Uuid) {
    let (low, high) = if user_a < user_b {
        (user_a, user_b)
    } else {
        (user_b, user_a)
    };

    let pool = PgPool::connect(database_url)
        .await
        .expect("must connect to create friendship");
    sqlx::query(
        r#"
        INSERT INTO friends (user_id_a, user_id_b)
        VALUES ($1, $2)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(low)
    .bind(high)
    .execute(&pool)
    .await
    .expect("must create friendship");
}

async fn expire_invite(database_url: &str, token: &str) {
    let pool = PgPool::connect(database_url)
        .await
        .expect("must connect to expire invite");
    sqlx::query(
        r#"
        UPDATE group_invites
        SET expires_at = NOW() - INTERVAL '1 second'
        WHERE token = $1
        "#,
    )
    .bind(token)
    .execute(&pool)
    .await
    .expect("must expire invite");
}

#[tokio::test]
#[serial_test::serial]
async fn direct_thread_pending_inbox_and_accept_flow() {
    run_isolated_test(|app, _| async move {
        let (requester_token, requester_id) = register_and_login(&app, "dm_req").await;
        let (receiver_token, receiver_id) = register_and_login(&app, "dm_recv").await;

        let (thread_id, status, created) =
            create_direct_thread_http(&app, &requester_token, receiver_id).await;
        assert!(created);
        assert_eq!(status, "PENDING");

        let (inbox_status, inbox_body) =
            send_authorized_get(&app, "/api/direct-threads?inbox=pending", &receiver_token).await;
        assert_eq!(inbox_status, StatusCode::OK);
        assert_eq!(
            inbox_body["threads"]
                .as_array()
                .expect("threads must be array")
                .len(),
            1
        );

        let (pending_create_status, _) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/threads/{thread_id}/messages"),
            &requester_token,
            json!({ "content": "pending hello" }),
        )
        .await;
        assert_eq!(pending_create_status, StatusCode::OK);

        let (receiver_create_status, _) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/threads/{thread_id}/messages"),
            &receiver_token,
            json!({ "content": "blocked before accept" }),
        )
        .await;
        assert_eq!(receiver_create_status, StatusCode::FORBIDDEN);

        let (accept_status, accept_body) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/direct-threads/{thread_id}/accept"),
            &receiver_token,
            json!({}),
        )
        .await;
        assert_eq!(accept_status, StatusCode::OK);
        assert_eq!(accept_body["thread"]["status"], "ACTIVE");

        let (receiver_send_status, _) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/threads/{thread_id}/messages"),
            &receiver_token,
            json!({ "content": "accepted reply" }),
        )
        .await;
        assert_eq!(receiver_send_status, StatusCode::OK);

        let (get_status, get_body) = send_authorized_get(
            &app,
            &format!("/api/direct-threads/{thread_id}"),
            &requester_token,
        )
        .await;
        assert_eq!(get_status, StatusCode::OK);
        assert_eq!(get_body["thread"]["status"], "ACTIVE");

        assert_ne!(requester_id, receiver_id);
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn direct_thread_reject_endpoint_removes_thread() {
    run_isolated_test(|app, _| async move {
        let (requester_token, _requester_id) = register_and_login(&app, "dm_reject_req").await;
        let (receiver_token, receiver_id) = register_and_login(&app, "dm_reject_recv").await;

        let (thread_id, status, _) =
            create_direct_thread_http(&app, &requester_token, receiver_id).await;
        assert_eq!(status, "PENDING");

        let (reject_status, _) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/direct-threads/{thread_id}/reject"),
            &receiver_token,
            json!({}),
        )
        .await;
        assert_eq!(reject_status, StatusCode::OK);

        let (get_status, _) = send_authorized_get(
            &app,
            &format!("/api/direct-threads/{thread_id}"),
            &requester_token,
        )
        .await;
        assert_eq!(get_status, StatusCode::NOT_FOUND);
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn friends_create_direct_thread_as_active() {
    run_isolated_test(|app, isolated_db_url| async move {
        let (owner_token, owner_id) = register_and_login(&app, "dm_friend_owner").await;
        let (peer_token, peer_id) = register_and_login(&app, "dm_friend_peer").await;
        insert_friendship(&isolated_db_url, owner_id, peer_id).await;

        let (thread_id, status, created) =
            create_direct_thread_http(&app, &owner_token, peer_id).await;
        assert!(created);
        assert_eq!(status, "ACTIVE");

        let (message_status, message_body) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/threads/{thread_id}/messages"),
            &peer_token,
            json!({ "content": "friend flow works" }),
        )
        .await;
        assert_eq!(message_status, StatusCode::OK);
        assert_eq!(message_body["message"]["content"], "friend flow works");
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn mark_thread_read_endpoint_updates_has_unread_in_direct_list() {
    run_isolated_test(|app, isolated_db_url| async move {
        let (owner_token, owner_id) = register_and_login(&app, "dm_read_owner").await;
        let (peer_token, peer_id) = register_and_login(&app, "dm_read_peer").await;
        insert_friendship(&isolated_db_url, owner_id, peer_id).await;

        let (thread_id, status, _) = create_direct_thread_http(&app, &owner_token, peer_id).await;
        assert_eq!(status, "ACTIVE");

        let (_, created_message) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/threads/{thread_id}/messages"),
            &owner_token,
            json!({ "content": "needs read marker" }),
        )
        .await;
        let message_id = created_message["message"]["id"]
            .as_str()
            .expect("message id should exist");

        let (before_list_status, before_list_body) =
            send_authorized_get(&app, "/api/direct-threads", &peer_token).await;
        assert_eq!(before_list_status, StatusCode::OK);
        assert_eq!(before_list_body["threads"][0]["has_unread"], true);

        let (read_status, read_body) = send_authorized_json(
            &app,
            "PUT",
            &format!("/api/threads/{thread_id}/read"),
            &peer_token,
            json!({}),
        )
        .await;
        assert_eq!(read_status, StatusCode::OK);
        assert_eq!(read_body["read"]["thread_id"], thread_id.to_string());
        assert_eq!(read_body["read"]["last_read_message_id"], message_id);

        let (after_list_status, after_list_body) =
            send_authorized_get(&app, "/api/direct-threads", &peer_token).await;
        assert_eq!(after_list_status, StatusCode::OK);
        assert_eq!(after_list_body["threads"][0]["has_unread"], false);
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn group_thread_endpoints_create_list_get() {
    run_isolated_test(|app, _| async move {
        let (owner_token, _owner_id) = register_and_login(&app, "grp_owner").await;
        let (_member_token, member_id) = register_and_login(&app, "grp_member").await;

        let thread_id =
            create_group_thread_http(&app, &owner_token, "Sagiton Squad", &[member_id]).await;

        let (list_status, list_body) =
            send_authorized_get(&app, "/api/group-threads", &owner_token).await;
        assert_eq!(list_status, StatusCode::OK);
        assert_eq!(
            list_body["threads"]
                .as_array()
                .expect("threads must be array")
                .len(),
            1
        );

        let (get_status, get_body) = send_authorized_get(
            &app,
            &format!("/api/group-threads/{thread_id}"),
            &owner_token,
        )
        .await;
        assert_eq!(get_status, StatusCode::OK);
        assert_eq!(get_body["thread"]["id"], thread_id.to_string());
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn add_group_member_endpoint_allows_new_member_to_access_group() {
    run_isolated_test(|app, _| async move {
        let (owner_token, _owner_id) = register_and_login(&app, "grp_add_owner").await;
        let (new_member_token, new_member_id) = register_and_login(&app, "grp_add_member").await;

        let thread_id = create_group_thread_http(&app, &owner_token, "Add Flow", &[]).await;

        let (add_status, _) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/group-threads/{thread_id}/members"),
            &owner_token,
            json!({ "user_id": new_member_id }),
        )
        .await;
        assert_eq!(add_status, StatusCode::OK);

        let (member_get_status, _) = send_authorized_get(
            &app,
            &format!("/api/group-threads/{thread_id}"),
            &new_member_token,
        )
        .await;
        assert_eq!(member_get_status, StatusCode::OK);
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn leave_group_endpoint_removes_member_access() {
    run_isolated_test(|app, _| async move {
        let (owner_token, _owner_id) = register_and_login(&app, "grp_leave_owner").await;
        let (member_token, member_id) = register_and_login(&app, "grp_leave_member").await;

        let thread_id =
            create_group_thread_http(&app, &owner_token, "Leave Flow", &[member_id]).await;

        let (leave_status, _) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/group-threads/{thread_id}/leave"),
            &member_token,
            json!({}),
        )
        .await;
        assert_eq!(leave_status, StatusCode::OK);

        let (get_status, _) = send_authorized_get(
            &app,
            &format!("/api/group-threads/{thread_id}"),
            &member_token,
        )
        .await;
        assert_eq!(get_status, StatusCode::NOT_FOUND);
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn group_invite_create_join_and_non_member_policy() {
    run_isolated_test(|app, _| async move {
        let (owner_token, _owner_id) = register_and_login(&app, "grp_inv_owner").await;
        let (outsider_token, _outsider_id) = register_and_login(&app, "grp_inv_out").await;

        let thread_id = create_group_thread_http(&app, &owner_token, "Invite Group", &[]).await;

        let (pre_list_status, _) = send_authorized_get(
            &app,
            &format!("/api/threads/{thread_id}/messages"),
            &outsider_token,
        )
        .await;
        assert_eq!(pre_list_status, StatusCode::NOT_FOUND);

        let (pre_send_status, _) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/threads/{thread_id}/messages"),
            &outsider_token,
            json!({ "content": "no access" }),
        )
        .await;
        assert_eq!(pre_send_status, StatusCode::NOT_FOUND);

        let (invite_status, invite_body) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/group-threads/{thread_id}/invites"),
            &owner_token,
            json!({ "expires_in_seconds": 3600, "max_uses": 3 }),
        )
        .await;
        assert_eq!(invite_status, StatusCode::OK);

        let token = invite_body["invite"]["token"]
            .as_str()
            .expect("invite token should exist")
            .to_string();

        let (join_status, join_body) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/group-invites/{token}/join"),
            &outsider_token,
            json!({}),
        )
        .await;
        assert_eq!(join_status, StatusCode::OK);
        assert_eq!(join_body["thread"]["id"], thread_id.to_string());

        let (post_send_status, _) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/threads/{thread_id}/messages"),
            &outsider_token,
            json!({ "content": "joined via invite" }),
        )
        .await;
        assert_eq!(post_send_status, StatusCode::OK);
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn group_invite_invalid_or_expired_returns_expected_error() {
    run_isolated_test(|app, isolated_db_url| async move {
        let (owner_token, _owner_id) = register_and_login(&app, "grp_inv_owner2").await;
        let (joiner_token, _joiner_id) = register_and_login(&app, "grp_inv_joiner2").await;
        let thread_id = create_group_thread_http(&app, &owner_token, "Invite Group 2", &[]).await;

        let (invalid_status, _) = send_authorized_json(
            &app,
            "POST",
            "/api/group-invites/not-a-valid-token/join",
            &joiner_token,
            json!({}),
        )
        .await;
        assert_eq!(invalid_status, StatusCode::NOT_FOUND);

        let (invite_status, invite_body) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/group-threads/{thread_id}/invites"),
            &owner_token,
            json!({ "expires_in_seconds": 3600 }),
        )
        .await;
        assert_eq!(invite_status, StatusCode::OK);
        let token = invite_body["invite"]["token"]
            .as_str()
            .expect("invite token should exist")
            .to_string();
        expire_invite(&isolated_db_url, &token).await;

        let (expired_status, _) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/group-invites/{token}/join"),
            &joiner_token,
            json!({}),
        )
        .await;
        assert_eq!(expired_status, StatusCode::BAD_REQUEST);
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn thread_message_update_delete_and_non_member_privacy_policy() {
    run_isolated_test(|app, _| async move {
        let (owner_token, _owner_id) = register_and_login(&app, "thread_msg_owner").await;
        let (_peer_token, peer_id) = register_and_login(&app, "thread_msg_peer").await;
        let (outsider_token, _outsider_id) = register_and_login(&app, "thread_msg_out").await;

        let (thread_id, _, _) = create_direct_thread_http(&app, &owner_token, peer_id).await;

        let (create_status, create_body) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/threads/{thread_id}/messages"),
            &owner_token,
            json!({ "content": "hello thread" }),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK);

        let message_id = create_body["message"]["id"]
            .as_str()
            .expect("message id should exist")
            .to_string();

        let (update_status, _) = send_authorized_json(
            &app,
            "PATCH",
            &format!("/api/thread-messages/{message_id}"),
            &owner_token,
            json!({ "content": "updated thread text" }),
        )
        .await;
        assert_eq!(update_status, StatusCode::OK);

        let (delete_status, _) = send_authorized_json(
            &app,
            "DELETE",
            &format!("/api/thread-messages/{message_id}"),
            &owner_token,
            json!({}),
        )
        .await;
        assert_eq!(delete_status, StatusCode::OK);

        let (get_thread_status, _) = send_authorized_get(
            &app,
            &format!("/api/direct-threads/{thread_id}"),
            &outsider_token,
        )
        .await;
        assert_eq!(get_thread_status, StatusCode::NOT_FOUND);

        let (list_status, _) = send_authorized_get(
            &app,
            &format!("/api/threads/{thread_id}/messages"),
            &outsider_token,
        )
        .await;
        assert_eq!(list_status, StatusCode::NOT_FOUND);
    })
    .await;
}
