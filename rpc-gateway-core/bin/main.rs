use actix_web::body::BoxBody;
use actix_web::dev::{ServiceFactory, ServiceRequest, ServiceResponse};
use actix_web::{App, Error, HttpServer, Result, post, web};
use alloy_json_rpc;
use rpc_gateway_core::config::Config;
use rpc_gateway_core::gateway::Gateway;
use rpc_gateway_core::logging;
use serde_json::Value;
use tracing::{debug, error, info};
use tracing_actix_web::{StreamSpan, TracingLogger};

// TODO: add better error handling.
#[post("/{chain_id}")]
async fn index(
    path: web::Path<u64>,
    request: web::Json<alloy_json_rpc::Request<Value>>,
    gateway: web::Data<Gateway>,
) -> Result<String> {
    let chain_id = path.into_inner();
    debug!(
        chain_id = %chain_id,
        request = ?request,
        "Received JSON-RPC request"
    );

    let response = gateway
        .forward_request(chain_id, request.into_inner())
        .await
        .map_err(|e| {
            error!(
                chain_id = %chain_id,
                error = %e,
                "Error forwarding request"
            );
            actix_web::error::ErrorInternalServerError(e)
        })?;

    debug!(
        chain_id = %chain_id,
        response = ?response,
        "Successfully forwarded request"
    );
    Ok(serde_json::to_string(&response)?)
}

// Create a function to configure and return the App
async fn create_app(
    gateway: Gateway,
) -> App<
    impl ServiceFactory<
        ServiceRequest,
        Config = (),
        Response = ServiceResponse<StreamSpan<BoxBody>>,
        Error = Error,
        InitError = (),
    >,
> {
    App::new()
        .wrap(TracingLogger::default())
        .app_data(web::Data::new(gateway))
        .service(index)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Load configuration from file
    let config =
        Config::from_toml_file("example.config.toml").expect("Failed to load configuration");
    info!(config = ?config, "Loaded configuration");

    // Initialize logging with the configuration
    logging::init_logging(&config);

    let server_config = config.clone();

    // Use the function in the HttpServer with the loaded config
    info!(
        host = %server_config.server.host,
        port = %server_config.server.port,
        "Starting server"
    );

    let gateway = Gateway::new(config.clone());
    gateway.readiness_probe().await;

    HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .app_data(web::Data::new(gateway.clone()))
            .service(index)
    })
    .bind((
        server_config.server.host.as_str(),
        server_config.server.port,
    ))?
    .run()
    .await
}
