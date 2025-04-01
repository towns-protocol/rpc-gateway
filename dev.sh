#!/bin/bash

# Watch for changes in .rs files and restart the server
watchexec -e rs -r cargo run 