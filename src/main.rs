use actix_web::body::BoxBody;
use actix_web::dev::{ServiceFactory, ServiceRequest, ServiceResponse};
use actix_web::{App, Error, HttpServer, Result, post, web};
use alloy_json_rpc;
use gateway::Gateway;
use rpc_gateway_config::Config;
use serde_json::Value;
use tracing::{debug, error, info};
use tracing_actix_web::{StreamSpan, TracingLogger};
use tracing_subscriber::{EnvFilter, fmt};

mod gateway;
mod request_pool;

// TODO: add better error handling.
#[post("/{chain_id}")]
async fn index(
    path: web::Path<u64>,
    request: web::Json<alloy_json_rpc::Request<Value>>,
    gateway: web::Data<Gateway>,
) -> Result<String> {
    let chain_id = path.into_inner();
    debug!(
        "Received JSON-RPC request for chain {}: {:?}",
        chain_id, request
    );

    let response = gateway
        .forward_request(chain_id, request.into_inner())
        .await
        .map_err(|e| {
            error!("Error forwarding request to chain {}: {}", chain_id, e);
            actix_web::error::ErrorInternalServerError(e)
        })?;

    debug!(
        "Successfully forwarded request for chain {}: {:?}",
        chain_id, response
    );
    Ok(serde_json::to_string(&response)?)
}

// Create a function to configure and return the App
fn create_app(
    config: Config,
) -> App<
    impl ServiceFactory<
        ServiceRequest,
        Config = (),
        Response = ServiceResponse<StreamSpan<BoxBody>>,
        Error = Error,
        InitError = (),
    >,
> {
    let gateway = Gateway::new(config.clone());
    App::new()
        .wrap(TracingLogger::default())
        .app_data(web::Data::new(gateway))
        .service(index)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize tracing subscriber with more detailed settings
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("rpc_gateway=debug,actix_web=debug,reqwest=debug"));

    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    info!("Starting RPC Gateway...");

    // Load configuration from file
    let config =
        Config::from_toml_file("example.config.toml").expect("Failed to load configuration");
    info!("Loaded configuration: {:?}", config);

    let server_config = config.clone();

    // Use the function in the HttpServer with the loaded config
    info!(
        "Starting server on {}:{}",
        server_config.server.host, server_config.server.port
    );

    HttpServer::new(move || create_app(config.clone()))
        .bind((
            server_config.server.host.as_str(),
            server_config.server.port,
        ))?
        .run()
        .await
}
