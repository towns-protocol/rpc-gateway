use clap::Parser;
use rpc_gateway_core::{cli::Cli, config::Config, server};

#[actix_web::main]
async fn main() {
    let cli = Cli::parse();

    // Load configuration from file
    let config = Config::from_path_buf(&cli.config).expect("Failed to load configuration");
    server::run(config).await.expect("Failed to run server");
}
