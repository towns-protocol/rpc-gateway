use rpc_gateway_core::app;

#[actix_web::main]
async fn main() {
    app::run().await;
}
