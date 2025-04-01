use std::sync::Mutex;

use actix_web::dev::{ServiceFactory, ServiceRequest, ServiceResponse};
use actix_web::{App, Error, HttpResponse, HttpServer, Responder, Result, get, post, web};
use alloy_chains::{Chain, ChainKind, NamedChain};
use alloy_json_rpc;
use serde::Deserialize;
use serde_json::Value;

struct AppState {
    app_name: String,
}

struct AppStateWithCounter {
    counter: Mutex<i32>,
}

#[get("/")]
async fn hello(data: web::Data<AppStateWithCounter>) -> impl Responder {
    let mut counter = data.counter.lock().unwrap();
    *counter += 1;

    HttpResponse::Ok().body(format!("Hello {}!", counter))
}

#[post("/echo")]
async fn echo(req_body: web::Bytes) -> impl Responder {
    HttpResponse::Ok().body(req_body)
}

async fn manual_hello() -> impl Responder {
    HttpResponse::Ok().body("Hey there!")
}

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
    let counter = web::Data::new(AppStateWithCounter {
        counter: Mutex::new(0),
    });
    App::new()
        .app_data(counter)
        .service(index)
        .route("/hey", web::get().to(manual_hello))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Use the function in the HttpServer
    HttpServer::new(move || create_app())
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use actix_web::test;
//     use actix_web::web::Bytes;

//     #[actix_web::test]
//     async fn test_hello() {
//         let app = test::init_service(App::new().service(hello)).await;
//         let req = test::TestRequest::get().uri("/").to_request();
//         let resp: String = test::call_and_read_body(&app, req).await;
//         assert_eq!(resp, "Hello 1!");
//     }
// }
