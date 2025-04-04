use actix_web::body::BoxBody;
use actix_web::dev::{ServiceFactory, ServiceRequest, ServiceResponse};
use actix_web::{App, Error, HttpServer, Result, post, web};
use alloy_json_rpc;
use serde_json::Value;
use tracing::{debug, error, info};
use tracing_actix_web::{StreamSpan, TracingLogger};

use crate::config::Config;
use crate::gateway::Gateway;
use crate::logging;

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

pub async fn run(config: Config) -> std::io::Result<()> {
    logging::init_logging(&config);

    info!(
        host = %config.server.host,
        port = %config.server.port,
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
    .bind((config.server.host.as_str(), config.server.port))?
    .run()
    .await
}
