use clap::Parser;
use metrics::counter;
use rpc_gateway_config::Config;
use rpc_gateway_core::{cli::Cli, config_watcher::ConfigWatcher, gateway::Gateway, logging, server};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::{debug, error, info};

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[actix_web::main]
async fn main() {
    let cli = Cli::parse();

    // Load configuration from YAML file
    let config_error_message = format!("Failed to load configuration from {}", &cli.config);
    let config = Config::from_yaml_file(&cli.config).expect(&config_error_message);

    logging::init_logging(&config);

    // Create gateway with config path for hot-reloading
    let config_path: PathBuf = cli.config.clone().into();
    let gateway = Gateway::new(config.clone(), Some(config_path.clone())).await;
    let gateway = Arc::new(gateway);

    gateway.run_upstream_health_checks_once().await;
    debug!("Ran upstream health checks");

    let task_tracker = TaskTracker::new();

    let token = CancellationToken::new();

    // Spawn health check loops
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

    // Spawn config watcher for hot-reloading
    let (reload_tx, mut reload_rx) = mpsc::channel::<()>(1);

    let token_clone = token.clone();
    task_tracker.spawn(async move {
        let watcher = ConfigWatcher::new(config_path);
        tokio::select! {
            _ = token_clone.cancelled() => {
                debug!("Stopping config watcher");
            }
            _ = watcher.watch(reload_tx) => {
                debug!("Config watcher stopped");
            }
        }
    });

    // Spawn reload handler
    let gateway_clone = gateway.clone();
    let token_clone = token.clone();
    task_tracker.spawn(async move {
        loop {
            tokio::select! {
                _ = token_clone.cancelled() => {
                    debug!("Stopping reload handler");
                    break;
                }
                result = reload_rx.recv() => {
                    match result {
                        Some(()) => {
                            info!("Config change detected, reloading...");
                            match gateway_clone.reload_config().await {
                                Ok(()) => {
                                    info!("Configuration reloaded successfully");
                                }
                                Err(e) => {
                                    error!(error = %e, "Failed to reload configuration");
                                    counter!("config_reload_total", "status" => "error").increment(1);
                                }
                            }
                        }
                        None => {
                            debug!("Reload channel closed, stopping reload handler");
                            break;
                        }
                    }
                }
            }
        }
    });

    task_tracker.close();

    // Use the gateway's config for metrics and server
    let config = gateway.config();

    rpc_gateway_core::metrics::run(&config.metrics);

    let gateway_clone = gateway.clone();

    let server = server::GatewayServer::new(gateway_clone, config);
    server.start().await.expect("Failed to run server");
    info!("Gateway server shut down. Waiting for remaining tasks to complete...");

    token.cancel();
    task_tracker.wait().await;

    info!("All tasks completed. Goodbye!");
}
