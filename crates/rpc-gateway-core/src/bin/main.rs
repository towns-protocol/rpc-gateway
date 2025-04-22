use std::sync::Arc;

use clap::Parser;
use rpc_gateway_core::{cli::Cli, config::Config, gateway::Gateway, logging, server};
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

    gateway.run_upstream_health_checks().await;
    debug!("Ran upstream health checks");

    gateway.start_upstream_health_check_loops();

    // let tracker = TaskTracker::new();

    let config = Arc::new(config);

    let config_clone = config.clone();
    let start_metrics_server = async move {
        rpc_gateway_core::metrics::run(&config_clone.metrics).await;
    };

    let config_clone = config.clone();
    let start_server = async move {
        let gateway = gateway.clone();
        let config_clone = config_clone.clone();
        server::run(gateway, config_clone)
            .await
            .expect("Failed to run server");
    };

    tokio::join!(start_metrics_server, start_server);
}
