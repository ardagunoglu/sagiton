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

/// Builds application router for integration tests.
///
/// # Parameters
/// - `database_url`: PostgreSQL connection string for the test environment.
async fn build_test_app(database_url: String) -> (Router, PgPool) {
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
    let pool = state.db_pool().clone();

    (build_router(state), pool)
}

/// Runs one test case in its own schema and drops it after completion.
///
/// # Parameters
/// - `test_fn`: Async test body that receives a fully initialized router.
async fn run_isolated_test<F, Fut>(test_fn: F)
where
    F: FnOnce(Router) -> Fut,
    Fut: Future<Output = ()>,
{
    let base_url = test_database_url();
    let schema = unique_schema_name();

    create_schema(&base_url, &schema).await;
    let isolated_url = add_search_path(&base_url, &schema);
    let (app, _pool) = build_test_app(isolated_url).await;

    let result = AssertUnwindSafe(test_fn(app)).catch_unwind().await;

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

fn unique_username(prefix: &str) -> String {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let short_suffix = &suffix[..8];
    let max_prefix_len = 23;
    let trimmed_prefix = if prefix.len() > max_prefix_len {
        &prefix[..max_prefix_len]
    } else {
        prefix
    };
    format!("{trimmed_prefix}_{short_suffix}")
}

fn unique_schema_name() -> String {
    format!("it_{}", uuid::Uuid::new_v4().simple())
}

fn add_search_path(base_url: &str, schema: &str) -> String {
    let encoded = format!("-csearch_path%3D{schema}");
    if base_url.contains('?') {
        format!("{base_url}&options={encoded}")
    } else {
        format!("{base_url}?options={encoded}")
    }
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

async fn register_user(app: &Router, username: &str, password: &str) {
    let (status, _) = send_json(
        app,
        "POST",
        "/api/auth/register",
        json!({
            "username": username,
            "email": format!("{username}@example.com"),
            "password": password,
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
#[serial_test::serial]
async fn health_endpoint_returns_ok() {
    run_isolated_test(|app| async move {
        let req = Request::builder()
            .method("GET")
            .uri("/health")
            .body(Body::empty())
            .expect("request must build");

        let resp = app
            .clone()
            .oneshot(req)
            .await
            .expect("request should be processed");
        let status = resp.status();

        assert_eq!(status, StatusCode::OK);
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn register_endpoint_creates_user() {
    run_isolated_test(|app| async move {
        let username = unique_username("register_case_user");
        let (status, body) = send_json(
            &app,
            "POST",
            "/api/auth/register",
            json!({
                "username": username,
                "email": format!("{username}@example.com"),
                "password": "strong_password_123",
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["user"]["username"], username);
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn login_endpoint_returns_tokens() {
    run_isolated_test(|app| async move {
        let username = unique_username("login_case_user");
        register_user(&app, &username, "strong_password_123").await;

        let (status, body) = send_json(
            &app,
            "POST",
            "/api/auth/login",
            json!({
                "username_or_email": username,
                "password": "strong_password_123",
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(body["access_token"].as_str().is_some());
        assert!(body["refresh_token"].as_str().is_some());
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn refresh_endpoint_rotates_tokens() {
    run_isolated_test(|app| async move {
        let username = unique_username("refresh_case_user");
        register_user(&app, &username, "strong_password_123").await;

        let (_, login_body) = send_json(
            &app,
            "POST",
            "/api/auth/login",
            json!({
                "username_or_email": username,
                "password": "strong_password_123",
            }),
        )
        .await;

        let old_refresh = login_body["refresh_token"]
            .as_str()
            .expect("refresh token must be present")
            .to_string();

        let (status, refresh_body) = send_json(
            &app,
            "POST",
            "/api/auth/refresh",
            json!({
                "refresh_token": old_refresh,
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_ne!(
            login_body["refresh_token"].as_str(),
            refresh_body["refresh_token"].as_str()
        );
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn logout_endpoint_revokes_refresh_token() {
    run_isolated_test(|app| async move {
        let username = unique_username("logout_case_user");
        register_user(&app, &username, "strong_password_123").await;

        let (_, login_body) = send_json(
            &app,
            "POST",
            "/api/auth/login",
            json!({
                "username_or_email": username,
                "password": "strong_password_123",
            }),
        )
        .await;

        let refresh_token = login_body["refresh_token"]
            .as_str()
            .expect("refresh token must be present")
            .to_string();

        let (logout_status, _) = send_json(
            &app,
            "POST",
            "/api/auth/logout",
            json!({
                "refresh_token": refresh_token.clone(),
            }),
        )
        .await;

        assert_eq!(logout_status, StatusCode::OK);

        let (refresh_status, refresh_body) = send_json(
            &app,
            "POST",
            "/api/auth/refresh",
            json!({
                "refresh_token": refresh_token,
            }),
        )
        .await;

        assert_eq!(refresh_status, StatusCode::UNAUTHORIZED);
        assert_eq!(refresh_body["error"]["code"], "UNAUTHORIZED");
    })
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn me_endpoint_returns_current_user() {
    run_isolated_test(|app| async move {
        let username = unique_username("me_case_user");
        register_user(&app, &username, "strong_password_123").await;

        let (_, login_body) = send_json(
            &app,
            "POST",
            "/api/auth/login",
            json!({
                "username_or_email": username,
                "password": "strong_password_123",
            }),
        )
        .await;

        let access_token = login_body["access_token"]
            .as_str()
            .expect("access token must be present")
            .to_string();

        let req = Request::builder()
            .method("GET")
            .uri("/api/auth/me")
            .header(header::AUTHORIZATION, format!("Bearer {access_token}"))
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
        assert_eq!(json["user"]["username"], username);
    })
    .await;
}
