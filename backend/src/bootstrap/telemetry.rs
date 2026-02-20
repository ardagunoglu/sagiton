use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// Initializes tracing subscriber.
///
/// # Parameters
/// - `app_env`: Environment mode (`production` enables JSON output).
/// - `default_log_level`: Fallback log filter when `RUST_LOG` is not set.
pub fn init(app_env: &str, default_log_level: &str) {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_log_level));

    if app_env.eq_ignore_ascii_case("production") {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().json())
            .init();
        return;
    }

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().pretty())
        .init();
}
