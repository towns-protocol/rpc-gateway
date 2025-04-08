use crate::config::Config;
use crate::gateway::Gateway;
use crate::logging;

use actix_web::{App, HttpServer, Result, web};
use alloy_json_rpc;
use serde_json::Value;
use tracing::{debug, error, info};
use tracing_actix_web::TracingLogger;

// TODO: add better error handling.
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

async fn liveness_probe(gateway: web::Data<Gateway>) -> Result<String> {
    if gateway.liveness_probe() {
        Ok("OK".to_string())
    } else {
        Err(actix_web::error::ErrorInternalServerError(
            "Gateway is not healthy",
        ))
    }
}

async fn readiness_probe(gateway: web::Data<Gateway>) -> Result<String> {
    if gateway.readiness_probe() {
        Ok("OK".to_string())
    } else {
        Err(actix_web::error::ErrorInternalServerError(
            "Gateway is not ready",
        ))
    }
}

pub async fn run(config: Config) -> std::io::Result<()> {
    logging::init_logging(&config);

    info!(
        host = %config.server.host,
        port = %config.server.port,
        "Starting server"
    );

    let gateway = Gateway::new(config.clone());
    debug!(gateway = ?gateway, "Created gateway");

    gateway.start_health_check_loop();

    HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .app_data(web::Data::new(gateway.clone()))
            .route("/{chain_id}", web::post().to(index))
            .route("/health", web::get().to(liveness_probe))
            .route("/health/liveness", web::get().to(liveness_probe))
            .route("/health/readiness", web::get().to(readiness_probe))
    })
    .bind((config.server.host.as_str(), config.server.port))?
    .run()
    .await
}
