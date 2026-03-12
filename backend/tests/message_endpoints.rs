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

async fn create_channel_context(app: &Router) -> (String, Uuid) {
    let token = register_and_login(app, "msg_owner").await;

    let (guild_status, guild_body) = send_authorized_json(
        app,
        "POST",
        "/api/guilds",
        &token,
        json!({ "name": "Message Guild" }),
    )
    .await;
    assert_eq!(guild_status, StatusCode::OK);

    let guild_id = Uuid::parse_str(
        guild_body["guild"]["id"]
            .as_str()
            .expect("guild id should exist"),
    )
    .expect("guild id should be uuid");

    let (channel_status, channel_body) = send_authorized_json(
        app,
        "POST",
        &format!("/api/guilds/{guild_id}/channels"),
        &token,
        json!({ "name": "general" }),
    )
    .await;
    assert_eq!(channel_status, StatusCode::OK);

    let channel_id = Uuid::parse_str(
        channel_body["channel"]["id"]
            .as_str()
            .expect("channel id should exist"),
    )
    .expect("channel id should be uuid");

    (token, channel_id)
}

async fn create_channel_context_with_guild(app: &Router) -> (String, Uuid, Uuid) {
    let token = register_and_login(app, "msg_owner_guild").await;

    let (guild_status, guild_body) = send_authorized_json(
        app,
        "POST",
        "/api/guilds",
        &token,
        json!({ "name": "Message Guild With Context" }),
    )
    .await;
    assert_eq!(guild_status, StatusCode::OK);

    let guild_id = Uuid::parse_str(
        guild_body["guild"]["id"]
            .as_str()
            .expect("guild id should exist"),
    )
    .expect("guild id should be uuid");

    let (channel_status, channel_body) = send_authorized_json(
        app,
        "POST",
        &format!("/api/guilds/{guild_id}/channels"),
        &token,
        json!({ "name": "general" }),
    )
    .await;
    assert_eq!(channel_status, StatusCode::OK);

    let channel_id = Uuid::parse_str(
        channel_body["channel"]["id"]
            .as_str()
            .expect("channel id should exist"),
    )
    .expect("channel id should be uuid");

    (token, guild_id, channel_id)
}

#[tokio::test]
#[serial_test::serial]
async fn create_and_list_messages_flow() {
    run_isolated_test(|app, _| async move {
        let (token, channel_id) = create_channel_context(&app).await;

        let (create_status, create_body) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/channels/{channel_id}/messages"),
            &token,
            json!({ "content": "hello world" }),
        )
        .await;
        assert_eq!(create_status, StatusCode::OK);
        assert_eq!(create_body["message"]["content"], "hello world");

        let (list_status, list_body) = send_authorized_get(
            &app,
            &format!("/api/channels/{channel_id}/messages?limit=20"),
            &token,
        )
        .await;

        assert_eq!(list_status, StatusCode::OK);
        assert_eq!(
            list_body["messages"]
                .as_array()
                .expect("messages should be array")
                .len(),
            1
        );
        assert!(
            list_body.get("next_cursor").is_some(),
            "list response must include next_cursor field"
        );
        assert!(
            list_body["next_cursor"].is_null(),
            "single item page should not expose a next cursor"
        );
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn update_message_endpoint_edits_author_message() {
    run_isolated_test(|app, _| async move {
        let (token, channel_id) = create_channel_context(&app).await;

        let (_, create_body) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/channels/{channel_id}/messages"),
            &token,
            json!({ "content": "before" }),
        )
        .await;

        let message_id = create_body["message"]["id"]
            .as_str()
            .expect("message id should exist");

        let (update_status, update_body) = send_authorized_json(
            &app,
            "PATCH",
            &format!("/api/messages/{message_id}"),
            &token,
            json!({ "content": "after" }),
        )
        .await;

        assert_eq!(update_status, StatusCode::OK);
        assert_eq!(update_body["message"]["content"], "after");
        assert!(!update_body["message"]["edited_at"].is_null());
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn delete_message_endpoint_soft_deletes_and_hides_message_from_list() {
    run_isolated_test(|app, _| async move {
        let (token, channel_id) = create_channel_context(&app).await;

        let (_, create_body) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/channels/{channel_id}/messages"),
            &token,
            json!({ "content": "to delete" }),
        )
        .await;

        let message_id = create_body["message"]["id"]
            .as_str()
            .expect("message id should exist");

        let (delete_status, _) = send_authorized_json(
            &app,
            "DELETE",
            &format!("/api/messages/{message_id}"),
            &token,
            json!({}),
        )
        .await;
        assert_eq!(delete_status, StatusCode::OK);

        let (_, list_body) = send_authorized_get(
            &app,
            &format!("/api/channels/{channel_id}/messages"),
            &token,
        )
        .await;

        assert_eq!(
            list_body["messages"]
                .as_array()
                .expect("messages should be array")
                .len(),
            0
        );
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn list_messages_supports_before_cursor() {
    run_isolated_test(|app, _| async move {
        let (token, channel_id) = create_channel_context(&app).await;

        let (_, first) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/channels/{channel_id}/messages"),
            &token,
            json!({ "content": "first" }),
        )
        .await;
        let first_id = first["message"]["id"]
            .as_str()
            .expect("first message id should exist")
            .to_string();

        let _ = send_authorized_json(
            &app,
            "POST",
            &format!("/api/channels/{channel_id}/messages"),
            &token,
            json!({ "content": "second" }),
        )
        .await;

        let (_status, page) = send_authorized_get(
            &app,
            &format!("/api/channels/{channel_id}/messages?before={first_id}&limit=50"),
            &token,
        )
        .await;

        assert_eq!(
            page["messages"]
                .as_array()
                .expect("messages should be array")
                .len(),
            0
        );
        assert!(
            page.get("next_cursor").is_some(),
            "list response must include next_cursor field"
        );
        assert!(
            page["next_cursor"].is_null(),
            "empty page should not expose a next cursor"
        );
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn non_author_cannot_edit_or_delete_message() {
    run_isolated_test(|app, _| async move {
        let (owner_token, channel_id) = create_channel_context(&app).await;
        let second_token = register_and_login(&app, "msg_non_author").await;

        let (_, created) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/channels/{channel_id}/messages"),
            &owner_token,
            json!({ "content": "owner message" }),
        )
        .await;

        let message_id = created["message"]["id"]
            .as_str()
            .expect("message id should exist");

        let (edit_status, _) = send_authorized_json(
            &app,
            "PATCH",
            &format!("/api/messages/{message_id}"),
            &second_token,
            json!({ "content": "hack" }),
        )
        .await;
        assert_eq!(edit_status, StatusCode::NOT_FOUND);

        let (delete_status, _) = send_authorized_json(
            &app,
            "DELETE",
            &format!("/api/messages/{message_id}"),
            &second_token,
            json!({}),
        )
        .await;
        assert_eq!(delete_status, StatusCode::NOT_FOUND);
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn mark_channel_read_endpoint_updates_has_unread_in_channel_list() {
    run_isolated_test(|app, _| async move {
        let (owner_token, guild_id, channel_id) = create_channel_context_with_guild(&app).await;
        let member_token = register_and_login(&app, "msg_channel_read_member").await;

        let (join_status, _) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/guilds/{guild_id}/join"),
            &member_token,
            json!({}),
        )
        .await;
        assert_eq!(join_status, StatusCode::OK);

        let (_, created_message) = send_authorized_json(
            &app,
            "POST",
            &format!("/api/channels/{channel_id}/messages"),
            &owner_token,
            json!({ "content": "unread message" }),
        )
        .await;
        let message_id = created_message["message"]["id"]
            .as_str()
            .expect("message id should exist");

        let (before_list_status, before_list_body) = send_authorized_get(
            &app,
            &format!("/api/guilds/{guild_id}/channels"),
            &member_token,
        )
        .await;
        assert_eq!(before_list_status, StatusCode::OK);
        assert_eq!(before_list_body["channels"][0]["has_unread"], true);

        let (read_status, read_body) = send_authorized_json(
            &app,
            "PUT",
            &format!("/api/channels/{channel_id}/read"),
            &member_token,
            json!({}),
        )
        .await;
        assert_eq!(read_status, StatusCode::OK);
        assert_eq!(read_body["read"]["channel_id"], channel_id.to_string());
        assert_eq!(read_body["read"]["last_read_message_id"], message_id);

        let (after_list_status, after_list_body) = send_authorized_get(
            &app,
            &format!("/api/guilds/{guild_id}/channels"),
            &member_token,
        )
        .await;
        assert_eq!(after_list_status, StatusCode::OK);
        assert_eq!(after_list_body["channels"][0]["has_unread"], false);
    })
    .await;
}
