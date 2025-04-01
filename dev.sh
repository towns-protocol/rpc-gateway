#!/bin/bash

export RUST_LOG="rpc_gateway=debug,actix_web=info,reqwest=info"
export RUST_BACKTRACE=1

# Watch for changes in .rs files and restart the server
watchexec -e rs -r cargo run 