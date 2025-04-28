use rpc_gateway_config::Config;
use std::sync::Arc;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{
    EnvFilter, Layer,
    fmt::{self},
    prelude::*,
    util::SubscriberInitExt,
};

pub fn init_logging(config: &Config) {
    let mut layers = Vec::new();
    let mut guards = Vec::new();

    // Configure console logging if enabled
    if config.logging.console.enabled {
        let console_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(&config.logging.console.rust_log));

        let console_layer = fmt::Layer::new()
            .with_target(config.logging.console.include_target)
            .with_thread_ids(config.logging.console.include_thread_ids)
            .with_thread_names(config.logging.console.include_thread_names)
            .with_file(config.logging.console.include_file)
            .with_line_number(config.logging.console.include_line_number);

        if config.logging.console.format == "json" {
            layers.push(console_layer.json().with_filter(console_filter).boxed());
        } else {
            layers.push(console_layer.with_filter(console_filter).boxed());
        }
    }

    // Configure file logging if enabled
    if config.logging.file.enabled {
        let file_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(&config.logging.file.rust_log));

        // Create the log directory if it doesn't exist
        if let Some(parent) = std::path::Path::new(&config.logging.file.path).parent() {
            std::fs::create_dir_all(parent).expect("Failed to create log directory");
        }

        let rotation = match config.logging.file.rotation.as_str() {
            "daily" => Rotation::DAILY,
            "hourly" => Rotation::HOURLY,
            _ => Rotation::NEVER,
        };

        let file_appender = RollingFileAppender::builder()
            .rotation(rotation)
            .filename_prefix("rpc-gateway")
            .filename_suffix("log")
            .build(&config.logging.file.path)
            .expect("Failed to create file appender");

        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        guards.push(Arc::new(guard)); // Store the guard to keep it alive

        let file_layer = fmt::Layer::new()
            .with_writer(non_blocking)
            .with_target(config.logging.file.include_target)
            .with_thread_ids(config.logging.file.include_thread_ids)
            .with_thread_names(config.logging.file.include_thread_names)
            .with_file(config.logging.file.include_file)
            .with_line_number(config.logging.file.include_line_number)
            .with_ansi(false);

        if config.logging.file.format == "json" {
            layers.push(file_layer.json().with_filter(file_filter).boxed());
        } else {
            layers.push(file_layer.with_filter(file_filter).boxed());
        }
    }

    // Initialize the subscriber with all layers
    tracing_subscriber::registry().with(layers).init();

    // Keep the guards alive by storing them in a static variable
    std::mem::forget(guards);
}
