use std::collections::HashMap;
use std::sync::Arc;

use crate::config::Config;
use crate::gateway::Gateway;
use actix_cors::Cors;
use actix_web::{App, HttpResponse, HttpServer, Result, web};
use anvil_rpc::{self, error::RpcError, request::Request, response::Response};
use tokio_util::task::TaskTracker;
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

    let project_config = match gateway.config.projects.get(&project_name) {
        Some(project_config) => project_config,
        None => {
            return Ok(serde_json::to_string(&Response::error(
                RpcError::internal_error_with("Project not found"),
            ))?);
        }
    };

    let response = gateway
        .handle_request(project_config, project_key, chain_id, request)
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
    let project_key = query.get("key").cloned();

    let request: Request = serde_json::from_str(&body).map_err(|e| {
        debug!(error = %e, "Failed to parse request body");
        actix_web::error::ErrorBadRequest("Invalid JSON-RPC request")
    })?;

    let project_config = gateway.config.projects.get("default").unwrap(); // TODO: make this a function on a ProjectsConfig struct.

    let response = gateway
        .handle_request(project_config, project_key, chain_id, request)
        .await;

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

pub struct GatewayServer {
    gateway: Arc<Gateway>,
    config: Arc<Config>,
}

impl GatewayServer {
    pub fn new(gateway: Arc<Gateway>, config: Arc<Config>) -> Self {
        Self { gateway, config }
    }

    pub async fn start(self) -> std::io::Result<()> {
        info!(
            host = %self.config.server.host,
            port = %self.config.server.port,
            "Starting server"
        );

        let host = self.config.server.host.clone();
        let port = self.config.server.port;
        HttpServer::new(move || {
            let cors_config = &self.config.cors;
            let mut cors = Cors::default();

            // TODO: make these configurable.
            if cors_config.allow_any_origin {
                cors = cors.allow_any_origin();
                cors = cors
                    .max_age(cors_config.max_age as usize)
                    .allow_any_origin()
                    .allow_any_header()
                    .allow_any_method()
                    .expose_any_header()
            }

            let gateway = self.gateway.clone();

            App::new()
                .wrap(TracingLogger::default())
                .app_data(web::Data::new(gateway.clone()))
                .route("/health", web::get().to(liveness_probe))
                .route("/health/liveness", web::get().to(liveness_probe))
                .route("/health/readiness", web::get().to(readiness_probe))
                .route(
                    "/{project_name}/{chain_id}",
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
}
