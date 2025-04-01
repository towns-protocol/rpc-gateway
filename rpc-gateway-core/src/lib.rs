// use alloy::{
//     rpc::client::{ClientBuilder, ReqwestClient},
//     transports::http::reqwest::Url,
// };

// pub async fn x() {
//     // Instantiate a new client over a transport.
//     let url: Url = "http://localhost:8545".to_string();
//     let client: ReqwestClient = ClientBuilder::default().http(url);

//     // // Prepare a batch request to the server.
//     // let batch = client.new_batch();

//     // // Batches serialize params immediately. So we need to handle the result when
//     // // adding calls.
//     // let block_number_fut = batch.add_call("eth_blockNumber", ()).unwrap();
//     // let balance_fut = batch.add_call("eth_getBalance", address).unwrap();

//     // // Make sure to send the batch!
//     // batch.send().await.unwrap();

//     // // After the batch is complete, we can get the results.
//     // // Note that requests may error separately!
//     // let block_number = block_number_fut.await.unwrap();
//     // let balance = balance_fut.await.unwrap();
// }
