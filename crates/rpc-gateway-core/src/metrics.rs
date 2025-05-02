use metrics_exporter_prometheus::{Matcher, PrometheusBuilder};
use rpc_gateway_config::MetricsConfig;
use tracing::info;

pub fn run(config: &MetricsConfig) {
    // Build + register the global recorder and start the HTTP server.
    let host_bytes = config
        .host_bytes()
        .expect("Invalid metrics host configuration");

    PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full("method_call_response_latency_seconds".to_owned()),
            &[
                0.01, // 10ms
                0.02, // 20ms
                0.05, // 50ms
                0.1,  // 100ms
                0.2,  // 200ms
                0.5,  // 500ms
                1.0,  // 1s
                2.0,  // 2s
            ],
        )
        .expect("failed to set buckets for method_call_response_latency_seconds")
        .set_buckets_for_metric(
            Matcher::Full("http_response_latency_seconds".to_owned()),
            &[
                0.01, // 10ms
                0.02, // 20ms
                0.05, // 50ms
                0.1,  // 100ms
                0.2,  // 200ms
                0.5,  // 500ms
                1.0,  // 1s
                2.0,  // 2s
            ],
        )
        .expect("failed to set buckets for http_response_latency_seconds")
        .with_http_listener((host_bytes, config.port)) // listen on configured host:port
        .install() // returns Result
        .expect("failed to install Prometheus recorder");

    info!(host = ?config.host, port = ?config.port, "Metrics server started");
}
