use actix_web::dev::{ServiceFactory, ServiceRequest, ServiceResponse};
use actix_web::{App, Error, HttpServer, Result, post, web};
use alloy_chains::Chain;
use alloy_json_rpc;
use serde_json::Value;

// TODO: add better error handling.
#[post("/{chain_id}")]
async fn index(
    path: web::Path<u64>,
    request: web::Json<alloy_json_rpc::Request<Value>>,
) -> Result<String> {
    let chain_id = path.into_inner();
    let chain = Chain::from(chain_id);
    println!("Chain: {:?}", chain);
    println!("Request: {:?}", request);
    if request.meta.method.eq("eth_blockNumber") {
        print!("Block number request");
    } else {
        print!("Other request");
    }

    Ok(format!("Chain ID: {}", chain_id))
}

// Create a function to configure and return the App
fn create_app() -> App<
    impl ServiceFactory<
        ServiceRequest,
        Config = (),
        Response = ServiceResponse,
        Error = Error,
        InitError = (),
    >,
> {
    App::new().service(index)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Use the function in the HttpServer
    HttpServer::new(move || create_app())
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
