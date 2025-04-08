use clap::Parser;
use tracing::debug;

use crate::{cli::Cli, config::Config, gateway::Gateway, logging, server};

pub async fn run() {
    let cli = Cli::parse();

    // Load configuration from YAML file
    let config_error_message = format!("Failed to load configuration from {}", &cli.config);
    let config = Config::from_yaml_file(&cli.config).expect(&config_error_message);

    logging::init_logging(&config);

    let gateway = Gateway::new(config.clone());
    debug!(gateway = ?gateway, "Created gateway");

    gateway.start_health_check_loops();

    server::run(gateway, config)
        .await
        .expect("Failed to run server");
}
