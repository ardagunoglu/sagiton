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

/// Builds application router for integration tests.
///
/// # Parameters
/// - `database_url`: PostgreSQL connection string for the test environment.
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

/// Runs one test case in its own schema and drops it after completion.
///
/// # Parameters
/// - `test_fn`: Async test body that receives a fully initialized router.
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

/// Creates a dedicated schema for one integration test.
///
/// # Parameters
/// - `database_url`: Base connection string without test schema override.
/// - `schema`: Schema name to be created.
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

/// Drops test schema and all nested objects.
///
/// # Parameters
/// - `database_url`: Base connection string without test schema override.
/// - `schema`: Schema name to be dropped.
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

async fn register_and_login(app: &Router, prefix: &str) -> String {
    let username = unique_username(prefix);

    let (register_status, _) = send_json(
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

    login_body["access_token"]
        .as_str()
        .expect("access token should exist")
        .to_string()
}

async fn create_guild_with_owner(app: &Router, owner_token: &str, name: &str) -> Uuid {
    let (status, body) = send_authorized_json(
        app,
        "POST",
        "/api/guilds",
        owner_token,
        json!({ "name": name }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);

    let guild_id = body["guild"]["id"].as_str().expect("guild id should exist");

    Uuid::parse_str(guild_id).expect("guild id should be valid uuid")
}

async fn create_channel_in_guild(
    app: &Router,
    owner_token: &str,
    guild_id: Uuid,
    name: &str,
) -> Uuid {
    let (status, body) = send_authorized_json(
        app,
        "POST",
        &format!("/api/guilds/{guild_id}/channels"),
        owner_token,
        json!({ "name": name }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let channel_id = body["channel"]["id"]
        .as_str()
        .expect("channel id should exist");
    Uuid::parse_str(channel_id).expect("channel id should be valid uuid")
}

async fn expire_guild_invite(database_url: &str, token: &str) {
    let pool = PgPool::connect(database_url)
        .await
        .expect("must connect to expire invite");

    sqlx::query(
        r#"
        UPDATE guild_invites
        SET expires_at = NOW() - INTERVAL '1 second'
        WHERE token = $1
        "#,
    )
    .bind(token)
    .execute(&pool)
    .await
    .expect("must update guild invite expiration");
}

#[tokio::test]
#[serial_test::serial]
async fn create_guild_endpoint_creates_membership_for_owner() {
    run_isolated_test(|app, _| async move {
        let token = register_and_login(&app, "guild_create").await;
        let (status, body) = send_authorized_json(
            &app,
            "POST",
            "/api/guilds",
            &token,
            json!({ "name": "Sagiton Core" }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["guild"]["name"], "Sagiton Core");
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn list_guilds_endpoint_returns_member_guilds() {
    run_isolated_test(|app, _| async move {
        let token = register_and_login(&app, "guild_list").await;
        let _ = create_guild_with_owner(&app, &token, "List Guild").await;

        let req = Request::builder()
            .method("GET")
            .uri("/api/guilds")
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

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            json["guilds"]
                .as_array()
                .expect("guilds should be array")
                .len(),
            1
        );
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn get_guild_endpoint_returns_single_guild_detail() {
    run_isolated_test(|app, _| async move {
        let token = register_and_login(&app, "guild_get").await;
        let guild_id = create_guild_with_owner(&app, &token, "Detail Guild").await;

        let req = Request::builder()
            .method("GET")
            .uri(format!("/api/guilds/{guild_id}"))
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

        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["guild"]["id"], guild_id.to_string());
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn join_guild_endpoint_adds_second_user_to_guild() {
    run_isolated_test(|app, _| async move {
        let owner_token = register_and_login(&app, "guild_join_owner").await;
        let member_token = register_and_login(&app, "guild_join_member").await;
        let guild_id = create_guild_with_owner(&app, &owner_token, "Join Guild").await;

        let (join_status, _) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/guilds/{guild_id}/join"),
            &member_token,
            json!({}),
        )
        .await;
        assert_eq!(join_status, StatusCode::OK);

        let req = Request::builder()
            .method("GET")
            .uri("/api/guilds")
            .header(header::AUTHORIZATION, format!("Bearer {member_token}"))
            .body(Body::empty())
            .expect("request must build");

        let resp = app
            .clone()
            .oneshot(req)
            .await
            .expect("request should be processed");
        let body = to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        let json: Value = serde_json::from_slice(&body).expect("response should be valid json");
        assert_eq!(
            json["guilds"]
                .as_array()
                .expect("guilds should be array")
                .len(),
            1
        );
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn leave_guild_endpoint_removes_non_owner_membership() {
    run_isolated_test(|app, _| async move {
        let owner_token = register_and_login(&app, "guild_leave_owner").await;
        let member_token = register_and_login(&app, "guild_leave_member").await;
        let guild_id = create_guild_with_owner(&app, &owner_token, "Leave Guild").await;

        let (join_status, _) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/guilds/{guild_id}/join"),
            &member_token,
            json!({}),
        )
        .await;
        assert_eq!(join_status, StatusCode::OK);

        let (leave_status, _) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/guilds/{guild_id}/leave"),
            &member_token,
            json!({}),
        )
        .await;
        assert_eq!(leave_status, StatusCode::OK);

        let req = Request::builder()
            .method("GET")
            .uri("/api/guilds")
            .header(header::AUTHORIZATION, format!("Bearer {member_token}"))
            .body(Body::empty())
            .expect("request must build");

        let resp = app
            .clone()
            .oneshot(req)
            .await
            .expect("request should be processed");
        let body = to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        let json: Value = serde_json::from_slice(&body).expect("response should be valid json");

        assert_eq!(
            json["guilds"]
                .as_array()
                .expect("guilds should be array")
                .len(),
            0
        );
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn create_channel_endpoint_creates_text_channel() {
    run_isolated_test(|app, _| async move {
        let token = register_and_login(&app, "channel_create").await;
        let guild_id = create_guild_with_owner(&app, &token, "Channel Guild").await;

        let (status, body) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/guilds/{guild_id}/channels"),
            &token,
            json!({ "name": "general" }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["channel"]["name"], "general");
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn list_channels_endpoint_returns_guild_channels() {
    run_isolated_test(|app, _| async move {
        let token = register_and_login(&app, "channel_list").await;
        let guild_id = create_guild_with_owner(&app, &token, "Channel List Guild").await;

        let _ = send_authorized_json(
            &app,
            "POST",
            &format!("/api/guilds/{guild_id}/channels"),
            &token,
            json!({ "name": "general" }),
        )
        .await;

        let req = Request::builder()
            .method("GET")
            .uri(format!("/api/guilds/{guild_id}/channels"))
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

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            json["channels"]
                .as_array()
                .expect("channels should be array")
                .len(),
            1
        );
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn get_channel_endpoint_returns_channel_detail() {
    run_isolated_test(|app, _| async move {
        let token = register_and_login(&app, "channel_get").await;
        let guild_id = create_guild_with_owner(&app, &token, "Channel Detail Guild").await;

        let (_, create_body) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/guilds/{guild_id}/channels"),
            &token,
            json!({ "name": "general" }),
        )
        .await;

        let channel_id = create_body["channel"]["id"]
            .as_str()
            .expect("channel id should exist");

        let req = Request::builder()
            .method("GET")
            .uri(format!("/api/channels/{channel_id}"))
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

        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["channel"]["id"], channel_id);
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn create_guild_invite_endpoint_creates_token_for_member() {
    run_isolated_test(|app, _| async move {
        let owner_token = register_and_login(&app, "guild_invite_owner").await;
        let guild_id = create_guild_with_owner(&app, &owner_token, "Invite Guild").await;

        let (status, body) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/guilds/{guild_id}/invites"),
            &owner_token,
            json!({ "expires_in_seconds": 3600, "max_uses": 5 }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(
            !body["invite"]["token"]
                .as_str()
                .expect("token should exist")
                .is_empty()
        );
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn join_guild_invite_endpoint_is_idempotent_and_unlocks_channel_access() {
    run_isolated_test(|app, _| async move {
        let owner_token = register_and_login(&app, "guild_join_owner").await;
        let member_token = register_and_login(&app, "guild_join_member").await;

        let guild_id = create_guild_with_owner(&app, &owner_token, "Invite Join Guild").await;
        let channel_id = create_channel_in_guild(&app, &owner_token, guild_id, "general").await;

        let (list_before_status, _) = send_authorized_get(
            &app,
            &format!("/api/channels/{channel_id}/messages"),
            &member_token,
        )
        .await;
        assert_eq!(list_before_status, StatusCode::NOT_FOUND);

        let (create_before_status, _) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/channels/{channel_id}/messages"),
            &member_token,
            json!({ "content": "hello" }),
        )
        .await;
        assert_eq!(create_before_status, StatusCode::NOT_FOUND);

        let (_, invite_body) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/guilds/{guild_id}/invites"),
            &owner_token,
            json!({ "max_uses": 1 }),
        )
        .await;

        let token = invite_body["invite"]["token"]
            .as_str()
            .expect("invite token should exist");

        let (first_join_status, first_join_body) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/guild-invites/{token}/join"),
            &member_token,
            json!({}),
        )
        .await;
        assert_eq!(first_join_status, StatusCode::OK);
        assert_eq!(first_join_body["joined"], true);

        let (second_join_status, second_join_body) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/guild-invites/{token}/join"),
            &member_token,
            json!({}),
        )
        .await;
        assert_eq!(second_join_status, StatusCode::OK);
        assert_eq!(second_join_body["joined"], false);

        let (list_after_status, list_after_body) = send_authorized_get(
            &app,
            &format!("/api/channels/{channel_id}/messages"),
            &member_token,
        )
        .await;
        assert_eq!(list_after_status, StatusCode::OK);
        assert_eq!(
            list_after_body["messages"]
                .as_array()
                .expect("messages should be array")
                .len(),
            0
        );

        let (create_after_status, create_after_body) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/channels/{channel_id}/messages"),
            &member_token,
            json!({ "content": "after join" }),
        )
        .await;
        assert_eq!(create_after_status, StatusCode::OK);
        assert_eq!(create_after_body["message"]["content"], "after join");
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn join_guild_invite_endpoint_returns_not_found_for_invalid_token() {
    run_isolated_test(|app, _| async move {
        let member_token = register_and_login(&app, "guild_invite_invalid").await;

        let (status, _) = send_authorized_json(
            &app,
            "POST",
            "/api/guild-invites/not-a-real-token/join",
            &member_token,
            json!({}),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn join_guild_invite_endpoint_returns_validation_for_expired_token() {
    run_isolated_test(|app, database_url| async move {
        let owner_token = register_and_login(&app, "guild_expire_owner").await;
        let member_token = register_and_login(&app, "guild_expire_member").await;

        let guild_id = create_guild_with_owner(&app, &owner_token, "Expired Invite Guild").await;
        let (_, invite_body) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/guilds/{guild_id}/invites"),
            &owner_token,
            json!({}),
        )
        .await;

        let token = invite_body["invite"]["token"]
            .as_str()
            .expect("invite token should exist")
            .to_string();

        expire_guild_invite(&database_url, &token).await;

        let (status, _) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/guild-invites/{token}/join"),
            &member_token,
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn join_guild_invite_endpoint_returns_validation_when_max_uses_is_reached() {
    run_isolated_test(|app, _| async move {
        let owner_token = register_and_login(&app, "guild_max_owner").await;
        let first_member = register_and_login(&app, "guild_max_member_1").await;
        let second_member = register_and_login(&app, "guild_max_member_2").await;

        let guild_id = create_guild_with_owner(&app, &owner_token, "Max Uses Guild").await;
        let (_, invite_body) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/guilds/{guild_id}/invites"),
            &owner_token,
            json!({ "max_uses": 1 }),
        )
        .await;
        let token = invite_body["invite"]["token"]
            .as_str()
            .expect("invite token should exist");

        let (first_status, _) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/guild-invites/{token}/join"),
            &first_member,
            json!({}),
        )
        .await;
        assert_eq!(first_status, StatusCode::OK);

        let (second_status, _) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/guild-invites/{token}/join"),
            &second_member,
            json!({}),
        )
        .await;
        assert_eq!(second_status, StatusCode::BAD_REQUEST);
    })
    .await;
}
