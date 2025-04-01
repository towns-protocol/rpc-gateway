#!/bin/bash

# Set logging environment variables
export RUST_LOG="rpc_gateway=debug,actix_web=info,reqwest=info"
export RUST_BACKTRACE=1

# Start the development server
cargo run --bin rpc-gateway 