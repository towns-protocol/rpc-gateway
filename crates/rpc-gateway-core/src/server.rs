use std::collections::HashMap;
use std::sync::Arc;

use crate::config::Config;
use crate::gateway::Gateway;
use actix_cors::Cors;
use actix_web::{App, HttpResponse, HttpServer, Result, web};
use anvil_rpc::{self, error::RpcError, request::Request, response::Response};
use tracing::{debug, info};
use tracing_actix_web::TracingLogger;

// TODO: add better error handling.
// TODO: this should instrument with debug level, not info.
#[tracing::instrument(skip(path, gateway))]
async fn handle_rpc_request_with_project(
    path: web::Path<(String, u64)>,
    body: String,
    gateway: web::Data<Arc<Gateway>>,
    query: web::Query<HashMap<String, String>>,
) -> Result<String> {
    let (project_name, chain_id) = path.into_inner();
    let project_key = query.get("key").cloned();

    let request: Request = serde_json::from_str(&body).map_err(|e| {
        debug!(error = %e, "Failed to parse request body");
        actix_web::error::ErrorBadRequest("Invalid JSON-RPC request")
    })?;

    let project_config = gateway.config.projects.get(&project_name);

    if project_config.is_none() {
        return Ok(serde_json::to_string(&Response::error(
            RpcError::internal_error_with("Project not found"),
        ))?);
    }

    let response = gateway
        .handle_request(Some(project_name), project_key, chain_id, request)
        .await;

    let response = response.unwrap_or(Response::error(RpcError::internal_error_with(
        "Internal server error",
    )));

    let response_string = serde_json::to_string(&response)?;

    Ok(response_string)
}

#[tracing::instrument(skip(path, gateway))]
async fn handle_rpc_request_without_project(
    path: web::Path<u64>,
    body: String,
    gateway: web::Data<Arc<Gateway>>,
    query: web::Query<HashMap<String, String>>,
) -> Result<String> {
    let chain_id = path.into_inner();

    let request: Request = serde_json::from_str(&body).map_err(|e| {
        debug!(error = %e, "Failed to parse request body");
        actix_web::error::ErrorBadRequest("Invalid JSON-RPC request")
    })?;

    let response = gateway.handle_request(None, None, chain_id, request).await;

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

    let host = config.server.host.clone();
    let port = config.server.port;
    HttpServer::new(move || {
        let cors_config = &config.cors;
        let mut cors = Cors::default();

        if cors_config.allow_any_origin {
            cors = cors.allow_any_origin();
        } else if !cors_config.allowed_origins.is_empty() {
            for origin in &cors_config.allowed_origins {
                cors = cors.allowed_origin(origin);
            }
        }

        // TODO: make these configurable.
        cors = cors
            .max_age(cors_config.max_age as usize)
            .allowed_methods(vec!["GET", "POST", "OPTIONS"])
            .allowed_headers(vec!["*"])
            .expose_headers(vec!["*"]);

        App::new()
            .wrap(TracingLogger::default())
            .app_data(web::Data::new(gateway.clone()))
            .route("/health", web::get().to(liveness_probe))
            .route("/health/liveness", web::get().to(liveness_probe))
            .route("/health/readiness", web::get().to(readiness_probe))
            .route(
                "/{chain_id}/{project_name}",
                web::post().to(handle_rpc_request_with_project),
            )
            .route(
                "/{chain_id}",
                web::post().to(handle_rpc_request_without_project),
            )
            .default_service(
                web::route().to(|| async { HttpResponse::NotFound().body("404 Not Found") }),
            )
            .wrap(cors)
    })
    .bind((host, port))?
    .run()
    .await
}
