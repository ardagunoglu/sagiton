use crate::bootstrap::metrics::render_prometheus;

/// Returns Prometheus exposition payload.
pub async fn metrics() -> String {
    render_prometheus()
}
