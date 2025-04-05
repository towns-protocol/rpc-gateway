use clap::Parser;
use rpc_gateway_core::{cli::Cli, config::Config, server};

#[actix_web::main]
async fn main() {
    let cli = Cli::parse();

    // Load configuration from YAML file
    let config_error_message = format!("Failed to load configuration from {}", &cli.config);
    let config = Config::from_yaml_file(&cli.config).expect(&config_error_message);
    server::run(config).await.expect("Failed to run server");
}
