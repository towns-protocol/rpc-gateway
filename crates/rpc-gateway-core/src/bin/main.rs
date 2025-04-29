use clap::Parser;
use rpc_gateway_config::Config;
use rpc_gateway_core::{cli::Cli, gateway::Gateway, logging, server};
use std::sync::Arc;
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::{debug, info};

#[actix_web::main]
async fn main() {
    let cli = Cli::parse();

    // Load configuration from YAML file
    let config_error_message = format!("Failed to load configuration from {}", &cli.config);
    let config = Config::from_yaml_file(&cli.config).expect(&config_error_message);

    logging::init_logging(&config);

    let gateway = Arc::new(Gateway::new(config.clone()));

    gateway.run_upstream_health_checks_once().await;
    debug!("Ran upstream health checks");

    let task_tracker = TaskTracker::new();

    let token = CancellationToken::new();

    let gateway_clone = gateway.clone();
    let token_clone = token.clone();

    task_tracker.spawn(async move {
        tokio::select! {
            _ = token_clone.cancelled() => {
                debug!("Stopping all health check loops");
            }
            _ = gateway_clone.start_upstream_health_check_loops() => {}
        }
        debug!("All health check loops stopped");
    });

    task_tracker.close();

    let config = Arc::new(config);
    let config_clone = config.clone();

    rpc_gateway_core::metrics::run(&config_clone.metrics);

    let gateway_clone = gateway.clone();

    let server = server::GatewayServer::new(gateway_clone, config_clone);
    server.start().await.expect("Failed to run server");
    info!("Gateway server shut down. Waiting for remaining tasks to complete...");

    token.cancel();
    task_tracker.wait().await;

    info!("All tasks completed. Goodbye!");
}
