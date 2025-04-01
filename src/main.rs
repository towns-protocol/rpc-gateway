use actix_web::dev::{ServiceFactory, ServiceRequest, ServiceResponse};
use actix_web::{App, Error, HttpServer, Result, post, web};
use alloy_chains::Chain;
use alloy_json_rpc;
use rpc_gateway_config::Config;
use serde_json::Value;

// TODO: add better error handling.
#[post("/{chain_id}")]
async fn index(
    path: web::Path<u64>,
    request: web::Json<alloy_json_rpc::Request<Value>>,
    config: web::Data<Config>,
) -> Result<String> {
    let chain_id = path.into_inner();
    let chain = Chain::from(chain_id);
    // println!("Chain: {:?}", chain);
    // println!("Request: {:?}", request);
    // if request.meta.method.eq("eth_blockNumber") {
    //     print!("Block number request");
    // } else {
    //     print!("Other request");
    // }

    println!("Config: {:?}", config);

    Ok(format!("Chain ID: {}", chain_id))
}

// Create a function to configure and return the App
fn create_app(
    config: Config,
) -> App<
    impl ServiceFactory<
        ServiceRequest,
        Config = (),
        Response = ServiceResponse,
        Error = Error,
        InitError = (),
    >,
> {
    App::new().app_data(web::Data::new(config)).service(index)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Load configuration from file
    let config =
        Config::from_toml_file("example.config.toml").expect("Failed to load configuration");
    let server_config = config.clone();

    // Use the function in the HttpServer with the loaded config
    HttpServer::new(move || create_app(config.clone()))
        .bind((
            server_config.server.host.as_str(),
            server_config.server.port,
        ))?
        .run()
        .await
}
