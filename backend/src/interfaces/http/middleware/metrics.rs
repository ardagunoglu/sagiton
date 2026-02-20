use crate::bootstrap::metrics::track_http_request;
use axum::{
    extract::{MatchedPath, Request},
    middleware::Next,
    response::Response,
};
use std::time::Instant;

/// Records HTTP request count and latency histogram labels.
pub async fn track_metrics(req: Request, next: Next) -> Response {
    let method = req.method().to_string();
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(MatchedPath::as_str)
        .unwrap_or("unmatched")
        .to_string();
    let start = Instant::now();

    let response = next.run(req).await;
    let elapsed = start.elapsed();
    let status = response.status().as_u16();
    track_http_request(&method, &route, status, elapsed);

    response
}
