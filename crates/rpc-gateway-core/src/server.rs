use std::sync::Arc;

use crate::config::Config;
use crate::gateway::Gateway;
use actix_web::{App, HttpResponse, HttpServer, Result, web};
use anvil_rpc::{self, error::RpcError, request::Request, response::Response};
use tracing::{debug, info};
use tracing_actix_web::TracingLogger;

// TODO: add better error handling.
// TODO: this should instrument with debug level, not info.
#[tracing::instrument(skip(path, gateway))]
async fn handle_rpc_request(
    path: web::Path<u64>,
    body: String,
    gateway: web::Data<Arc<Gateway>>,
) -> Result<String> {
    let chain_id = path.into_inner();

    let request: Request = serde_json::from_str(&body).map_err(|e| {
        debug!(error = %e, "Failed to parse request body");
        actix_web::error::ErrorBadRequest("Invalid JSON-RPC request")
    })?;

    let response = gateway.handle_request(chain_id, request).await;

    let response = response.unwrap_or(Response::error(RpcError::internal_error_with(
        "Internal server error",
    )));

    let response_string = serde_json::to_string(&response)?;

    Ok(response_string)
}

async fn liveness_probe() -> Result<String> {
    // TODO: implement real liveness probes.
    Ok("OK".to_string())
}

async fn readiness_probe() -> Result<String> {
    // TODO: implement readiness probes.
    Ok("OK".to_string())
}

pub async fn run(gateway: Arc<Gateway>, config: Arc<Config>) -> std::io::Result<()> {
    info!(
        host = %config.server.host,
        port = %config.server.port,
        "Starting server"
    );

    HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .app_data(web::Data::new(gateway.clone()))
            .route("/health", web::get().to(liveness_probe))
            .route("/health/liveness", web::get().to(liveness_probe))
            .route("/health/readiness", web::get().to(readiness_probe))
            .route("/{chain_id}", web::post().to(handle_rpc_request))
            .default_service(
                web::route().to(|| async { HttpResponse::NotFound().body("404 Not Found") }),
            )
    })
    .bind((config.server.host.as_str(), config.server.port))?
    .run()
    .await
}
