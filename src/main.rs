use actix_web::{App, HttpServer, Responder, web};
use alloy_json_rpc::{Request, Response};
use rpc_gateway_config::Config;
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, error, info};
use tracing_actix_web::TracingLogger;

mod request_pool;
use request_pool::RequestPool;

async fn handle_json_rpc(
    request: web::Json<Request<Value>>,
    pool: web::Data<Arc<RequestPool>>,
) -> impl Responder {
    debug!("Received JSON-RPC request: {:?}", request);

    // Extract chain_id from request params or use default
    let chain_id = request
        .params
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_u64())
        .unwrap_or(1);

    // Clone the request ID before consuming the request
    let request_id = request.meta.id.clone();

    match pool.forward_request(chain_id, request.into_inner()).await {
        Ok(response) => {
            debug!(
                "Successfully forwarded request to chain {}: {:?}",
                chain_id, response
            );
            web::Json(response)
        }
        Err(e) => {
            error!("Error forwarding request to chain {}: {}", chain_id, e);
            // Create an error response
            let error_response = Response::internal_error(request_id);
            web::Json(error_response)
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize tracing subscriber
    tracing_subscriber::fmt::init();
    info!("Starting RPC Gateway...");

    // Load configuration
    let config = Config::from_toml_file("example.config.toml").expect("Failed to load config");
    info!("Loaded configuration: {:?}", config);

    // Create request pool
    let pool = Arc::new(RequestPool::new(config.clone()));
    info!("Created request pool");

    // Start HTTP server
    let server_config = &config.server;
    info!(
        "Starting server on {}:{}",
        server_config.host, server_config.port
    );

    HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .app_data(web::Data::new(pool.clone()))
            .route("/", web::post().to(handle_json_rpc))
    })
    .bind((server_config.host.clone(), server_config.port))?
    .run()
    .await
}
