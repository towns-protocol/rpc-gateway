//! Simple Goose load test example. Duplicates the simple example on the
//! Locust project page (<https://locust.io/>).
//!
//! ## License
//!
//! Copyright 2020-2022 Jeremy Andrews
//!
//! Licensed under the Apache License, Version 2.0 (the "License");
//! you may not use this file except in compliance with the License.
//! You may obtain a copy of the License at
//!
//! <http://www.apache.org/licenses/LICENSE-2.0>
//!
//! Unless required by applicable law or agreed to in writing, software
//! distributed under the License is distributed on an "AS IS" BASIS,
//! WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//! See the License for the specific language governing permissions and
//! limitations under the License.

use goose::prelude::*;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), GooseError> {
    GooseAttack::initialize()?
        .register_scenario(
            scenario!("RPCUser")
                .register_transaction(transaction!(eth_block_number).set_name("eth_blockNumber"))
                .register_transaction(transaction!(eth_get_balance).set_name("eth_getBalance"))
                .register_transaction(
                    transaction!(eth_get_block_by_number).set_name("eth_getBlockByNumber"),
                ),
        )
        .execute()
        .await?;

    Ok(())
}

/// Get the current block number
async fn eth_block_number(user: &mut GooseUser) -> TransactionResult {
    let request = json!({
        "jsonrpc": "2.0",
        "method": "eth_blockNumber",
        "params": [],
        "id": 1
    });

    let _response = user.post_json("/1", &request).await?;
    Ok(())
}

async fn eth_get_block_by_number(user: &mut GooseUser) -> TransactionResult {
    let request = json!({
        "jsonrpc": "2.0",
        "method": "eth_getBlockByNumber",
        "params": ["latest", false],
        "id": 1
    });

    let _response = user.post_json("/1", &request).await?;
    Ok(())
}

/// Get the balance of a random address
async fn eth_get_balance(user: &mut GooseUser) -> TransactionResult {
    // Using a random address for testing
    let address = "0x742d35Cc6634C0532925a3b844Bc454e4438f44e";
    let request = json!({
        "jsonrpc": "2.0",
        "method": "eth_getBalance",
        "params": [address, "latest"],
        "id": 1
    });

    let _response = user.post_json("/1", &request).await?;
    Ok(())
}
