use clap::Parser;
use rpc_gateway_core::{cli::Cli, config::Config, gateway::Gateway, logging, server};
use std::sync::Arc;
use tracing::debug;

#[actix_web::main]
async fn main() {
    let cli = Cli::parse();

    // Load configuration from YAML file
    let config_error_message = format!("Failed to load configuration from {}", &cli.config);
    let config = Config::from_yaml_file(&cli.config).expect(&config_error_message);

    logging::init_logging(&config);

    let gateway = Arc::new(Gateway::new(config.clone()));
    debug!(gateway = ?gateway, "Created gateway");

    gateway.run_upstream_health_checks_once().await;
    debug!("Ran upstream health checks");

    let gateway_clone = gateway.clone();
    let start_upstream_health_check_loops = async move {
        gateway_clone.start_upstream_health_check_loops().await;
    };

    // let tracker = TaskTracker::new();

    let config = Arc::new(config);

    let config_clone = config.clone();
    let start_metrics_server = async move {
        rpc_gateway_core::metrics::run(&config_clone.metrics).await;
    };

    let gateway_clone = gateway.clone();
    let config_clone = config.clone();
    let start_server = async move {
        let config_clone = config_clone.clone();
        let server = server::GatewayServer::new(gateway_clone, config_clone);
        server.start().await.expect("Failed to run server");
    };

    tokio::join!(
        start_metrics_server,
        start_server,
        start_upstream_health_check_loops
    );
}
