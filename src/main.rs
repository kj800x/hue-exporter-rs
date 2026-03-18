mod hue;
mod metrics;

use axum::{Router, routing::get};
use hue::{BridgeState, HueClient};
use std::sync::{Arc, RwLock};
use tracing::{error, info};

type SharedState = Arc<RwLock<Option<BridgeState>>>;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".parse().unwrap()),
        )
        .init();

    let bridge_ip =
        std::env::var("HUE_BRIDGE_IP").expect("HUE_BRIDGE_IP environment variable is required");
    let api_key =
        std::env::var("HUE_API_KEY").expect("HUE_API_KEY environment variable is required");
    let poll_interval: u64 = std::env::var("HUE_POLL_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(15);
    let port: u16 = std::env::var("HUE_EXPORTER_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(9366);

    let client = HueClient::new(&bridge_ip, &api_key);
    let state: SharedState = Arc::new(RwLock::new(None));

    // Spawn the polling loop.
    let poll_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(poll_interval));
        loop {
            interval.tick().await;
            match client.poll().await {
                Ok(bridge_state) => {
                    let n = bridge_state.lights.len();
                    *poll_state.write().unwrap() = Some(bridge_state);
                    info!(lights = n, "poll successful");
                }
                Err(e) => {
                    error!(error = %e, "poll failed — clearing metrics (loss of signal)");
                    *poll_state.write().unwrap() = None;
                }
            }
        }
    });

    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .route("/healthz", get(|| async { "ok" }))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .expect("failed to bind");
    info!(port, "hue-exporter listening");
    axum::serve(listener, app).await.expect("server error");
}

async fn metrics_handler(
    axum::extract::State(state): axum::extract::State<SharedState>,
) -> (
    axum::http::StatusCode,
    [(axum::http::header::HeaderName, &'static str); 1],
    String,
) {
    let snapshot = state.read().unwrap().clone();
    let body = metrics::render(&snapshot);
    (
        axum::http::StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
}
