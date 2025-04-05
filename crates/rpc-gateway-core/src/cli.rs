use clap::Parser;
use std::path::PathBuf;

/// RPC Gateway - A high-performance RPC gateway for Ethereum networks
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Path to the configuration file
    #[arg(short = 'c', long = "config", value_name = "FILE")]
    pub config: String,
}
