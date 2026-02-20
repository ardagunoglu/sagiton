use anyhow::Context;
use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::sync::OnceLock;
use std::time::Duration;

static METRICS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();
static METRICS_INIT_RESULT: OnceLock<Result<(), String>> = OnceLock::new();

/// Initializes Prometheus recorder if not initialized yet.
pub fn init_prometheus_recorder() -> anyhow::Result<()> {
    let result = METRICS_INIT_RESULT.get_or_init(|| {
        let handle = PrometheusBuilder::new()
            .install_recorder()
            .context("failed to install prometheus recorder")
            .map_err(|err| err.to_string())?;
        let _ = METRICS_HANDLE.set(handle);
        Ok(())
    });

    match result {
        Ok(()) => Ok(()),
        Err(err) => Err(anyhow::anyhow!(err.clone())),
    }
}

/// Returns rendered Prometheus exposition format.
pub fn render_prometheus() -> String {
    METRICS_HANDLE
        .get()
        .map(PrometheusHandle::render)
        .unwrap_or_default()
}

pub fn track_http_request(method: &str, route: &str, status: u16, elapsed: Duration) {
    let status = status.to_string();
    counter!(
        "http_requests_total",
        "method" => method.to_string(),
        "route" => route.to_string(),
        "status" => status.clone()
    )
    .increment(1);

    histogram!(
        "http_request_latency_seconds",
        "method" => method.to_string(),
        "route" => route.to_string(),
        "status" => status
    )
    .record(elapsed.as_secs_f64());
}

pub fn set_ws_connections_active(count: usize) {
    gauge!("ws_connections_active").set(count as f64);
}

pub fn increment_ws_events_sent(event_name: &str) {
    counter!(
        "ws_events_sent_total",
        "event" => event_name.to_string()
    )
    .increment(1);
}

pub fn increment_redis_reconnect() {
    counter!("redis_reconnect_total").increment(1);
}

pub fn increment_rate_limit_denied(scope: &str) {
    counter!(
        "rate_limit_denied_total",
        "scope" => scope.to_string()
    )
    .increment(1);
}
