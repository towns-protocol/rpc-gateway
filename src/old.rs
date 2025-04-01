// //! Example of creating an HTTP provider using the `on_http` method on the `ProviderBuilder`.

// use std::{result, str::FromStr};

// use alloy::{
//     eips::BlockNumberOrTag,
//     primitives::Address,
//     providers::{Provider, ProviderBuilder},
//     rpc::json_rpc::{Id, Request},
//     transports::http::reqwest::Url,
// };
// use alloy_chains::{Chain, ChainKind, NamedChain};
// use eyre::{Result, eyre};

// /// Parses any input string into an alloy-rs Request object.
// ///
// /// This function is flexible and will attempt to parse the input in multiple ways:
// ///
// /// 1. First tries to parse as a complete JSON-RPC request object
// /// 2. If that fails, tries to extract method, id, and params from partial JSON
// /// 3. If the input is just a simple method name string, creates a request with no params
// ///
// /// # Arguments
// ///
// /// * `input` - A string slice that contains the input to parse
// ///
// /// # Returns
// ///
// /// Returns a Result containing the parsed Request on success, with parameters
// /// stored as dynamic `serde_json::Value` for maximum flexibility.
// ///
// /// # Examples
// ///
// /// ```
// /// // Parse a complete JSON-RPC request
// /// let json = r#"{"jsonrpc":"2.0","method":"eth_call","params":[{"to":"0x..."}, "latest"],"id":1}"#;
// /// let request = parse_json_rpc_request(json)?;
// ///
// /// // Parse a partial request
// /// let partial = r#"{"method":"eth_getBalance","params":["0x..."]}"#; // No jsonrpc, no id
// /// let request = parse_json_rpc_request(partial)?;
// ///
// /// // Parse just a method name
// /// let simple = "eth_blockNumber";
// /// let request = parse_json_rpc_request(simple)?;
// /// ```
// pub fn parse_json_rpc_request(input: &str) -> Result<Request<serde_json::Value>> {
//     // First, try to parse as a well-formed JSON-RPC request
//     if let Ok(request) = serde_json::from_str::<Request<serde_json::Value>>(input) {
//         return Ok(request);
//     } else {
//         return Err(eyre!("Could not parse input as JSON-RPC request"));
//     }
// }

// async fn y() {
//     let builder = ProviderBuilder::new();
//     let url = Url::parse("https://ethereum-rpc.publicnode.com").unwrap();
//     let provider_eth_mainnet = builder.with_chain(NamedChain::Mainnet).on_http(url);
//     let get_block_eth_mainnet = provider_eth_mainnet
//         .get_block_by_number(BlockNumberOrTag::Latest)
//         .await
//         .unwrap()
//         .unwrap();

//     // for tx in txes.int() {
//     //     println!("Tx: {:?}", tx);
//     // }

//     let get_tx_eth_mainnet = provider_eth_mainnet
//         .get_transaction_by_block_hash_and_index(get_block_eth_mainnet.header.hash, 0)
//         .await
//         .unwrap()
//         .unwrap();

//     let hash = get_tx_eth_mainnet.info().hash.unwrap();

//     let get_tx_eth_mainnet_2 = provider_eth_mainnet
//         .get_transaction_by_hash(hash)
//         .await
//         .unwrap()
//         .unwrap();

//     let addr = Address::from_str("0x742d35Cc6634C0532925a3b844Bc454e4438f44e").unwrap();
//     let balance = provider_eth_mainnet.get_balance(addr).await.unwrap();

//     // println!("Txes: {:?}", txes);
//     println!("Transaction: {:?}", get_tx_eth_mainnet);
//     println!("Tx2: {:?}", get_tx_eth_mainnet_2);
//     // println!("Block: {:?}", get_block_eth_mainnet);
//     println!("Balance: {:?}", balance);
// }

// #[tokio::main]
// async fn main() -> Result<()> {
//     y().await;
//     // // === PART 1: Testing our unified parser function ===
//     // println!("=== Testing parse_json_rpc_request function ===\n");

//     // // Test Case 1: Well-formed JSON-RPC request
//     // let test1 = r#"{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}"#;
//     // let result1 = parse_json_rpc_request(test1)?;
//     // println!("Test 1 (well-formed request): {:?}", result1);

//     // // Test Case 2: JSON with missing jsonrpc version
//     // let test2 = r#"{"jsonrpc":"2.0","method":"eth_getBalance","params":["0x742d35Cc6634C0532925a3b844Bc454e4438f44e"],"id":2}"#;
//     // let result2 = parse_json_rpc_request(test2)?;
//     // println!("Test 2: {:?}", result2);

//     // // Test Case 3: JSON with missing ID
//     // let test3 = r#"{"method":"eth_getBalance","params":["0x742d35Cc6634C0532925a3b844Bc454e4438f44e", "latest"]}"#;
//     // let result3 = parse_json_rpc_request(test3)?;
//     // println!("Test 3 (missing ID): {:?}", result3);

//     // // Test Case 4: Complex parameters
//     // let test4 = r#"{"method":"eth_call","params":[{"to":"0x742d35Cc6634C0532925a3b844Bc454e4438f44e","data":"0x70a08231000000000000000000000000742d35cc6634c0532925a3b844bc454e4438f44e"}, "latest"],"id":4}"#;
//     // let result4 = parse_json_rpc_request(test4)?;
//     // println!("Test 4 (complex params): {:?}", result4);

//     // // Test Case 5: Just a method name
//     // let test5 = "eth_blockNumber";
//     // let result5 = parse_json_rpc_request(test5)?;
//     // println!("Test 5 (just method name): {:?}", result5);

//     // // Test Case 6: Method with string ID
//     // let test6 = r#"{"method":"eth_getTransactionReceipt","params":["0x..."],"id":"request1"}"#;
//     // let result6 = parse_json_rpc_request(test6)?;
//     // println!("Test 6 (string ID): {:?}", result6);

//     // // Test Case 7: Method with null params
//     // let test7 = r#"{"method":"net_version","params":null,"id":7}"#;
//     // let result7 = parse_json_rpc_request(test7)?;
//     // println!("Test 7 (null params): {:?}", result7);

//     // // Test Case 8: Object params instead of array
//     // let test8 = r#"{"method":"eth_sendTransaction","params":{"to":"0x...","value":"0x1"},"id":8}"#;
//     // let result8 = parse_json_rpc_request(test8)?;
//     // println!("Test 8 (object params): {:?}", result8);

//     // // === PART 2: Working with parsed requests ===
//     // println!("\n=== Working with parsed requests ===\n");

//     // // Extract and work with metadata and parameters
//     // let sample_request = parse_json_rpc_request(
//     //     r#"{"method":"eth_call","params":[{"to":"0xcontract","data":"0xcalldata"}, "latest"],"id":42}"#,
//     // )?;

//     // // Accessing basic fields
//     // let method = &sample_request.meta.method;
//     // let id = &sample_request.meta.id;
//     // let params = &sample_request.params;

//     // println!("Method: {}", method);
//     // println!("ID: {:?}", id);
//     // println!("Raw params: {}", serde_json::to_string(params)?);

//     // // Working with parameters based on their structure
//     // match &params {
//     //     serde_json::Value::Array(array) => {
//     //         println!("Parameters are in array format with {} items", array.len());

//     //         // Handle array parameters
//     //         for (i, param) in array.iter().enumerate() {
//     //             match param {
//     //                 serde_json::Value::Object(obj) => {
//     //                     println!("Parameter {} is an object with {} fields", i, obj.len());
//     //                     for (key, value) in obj {
//     //                         println!("  {}: {}", key, value);
//     //                     }
//     //                 }
//     //                 serde_json::Value::String(s) => {
//     //                     println!("Parameter {} is a string: {}", i, s);
//     //                 }
//     //                 serde_json::Value::Number(n) => {
//     //                     println!("Parameter {} is a number: {}", i, n);
//     //                 }
//     //                 serde_json::Value::Bool(b) => {
//     //                     println!("Parameter {} is a boolean: {}", i, b);
//     //                 }
//     //                 serde_json::Value::Null => {
//     //                     println!("Parameter {} is null", i);
//     //                 }
//     //                 _ => println!("Parameter {} has unsupported type", i),
//     //             }
//     //         }
//     //     }
//     //     serde_json::Value::Object(obj) => {
//     //         println!("Parameters are in object format with {} fields", obj.len());
//     //         for (key, value) in obj {
//     //             println!("  {}: {}", key, value);
//     //         }
//     //     }
//     //     serde_json::Value::Null => {
//     //         println!("No parameters (null)");
//     //     }
//     //     _ => println!("Parameters in unsupported format"),
//     // }

//     // // === PART 3: Error handling ===
//     // println!("\n=== Error handling ===\n");

//     // // Try various kinds of invalid input
//     // let invalid_inputs = [
//     //     ("{invalid json}", "Invalid JSON syntax"),
//     //     (r#"{"foo":"bar"}"#, "Missing method field"),
//     //     ("", "Empty string"),
//     //     ("123", "Just a number, not a method name"),
//     // ];

//     // for (input, description) in invalid_inputs.iter() {
//     //     match parse_json_rpc_request(input) {
//     //         Ok(req) => println!("Unexpected success with {}: {:?}", description, req),
//     //         Err(e) => println!("{}: {:?}", description, e),
//     //     }
//     // }

//     // // === PART 4: Serialization for sending requests ===
//     // println!("\n=== Serialization for network transmission ===\n");

//     // // Creating a new request programmatically
//     // let new_request = Request::new(
//     //     "eth_getBalance",
//     //     Id::Number(99),
//     //     serde_json::json!([
//     //         "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045", // vitalik.eth
//     //         "latest"
//     //     ]),
//     // );

//     // // Serialize for sending over network
//     // let serialized = serde_json::to_string(&new_request)?;
//     // println!("Serialized for sending: {}", serialized);

//     // // Simulate receiving and parsing the request on the other end
//     // let received = parse_json_rpc_request(&serialized)?;
//     // println!("Parsed after receiving: {:?}", received);

//     Ok(())
// }
