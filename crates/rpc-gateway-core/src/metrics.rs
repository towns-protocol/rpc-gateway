use metrics_exporter_prometheus::PrometheusBuilder;
use tracing::info;

use crate::config::MetricsConfig;

pub struct MetricsServer;

impl MetricsServer {
    pub fn new() -> Self {
        Self
    }

    pub async fn start(&self, config: &MetricsConfig) {
        // Build + register the global recorder and start the HTTP server.
        let host_bytes = config
            .host_bytes()
            .expect("Invalid metrics host configuration");

        PrometheusBuilder::new()
            .with_http_listener((host_bytes, config.port)) // listen on configured host:port
            .install() // returns Result
            .expect("failed to install Prometheus recorder");

        info!(host = ?config.host, port = ?config.port, "Metrics server started");
    }

    pub async fn shutdown(&self) {
        info!("Metrics server stopped");
    }
}
