.PHONY: build docker-build dev docker-run clean docker-clean help test lint

# Docker image name
IMAGE_NAME := rpc-gateway

# Default target
all: build

# Local development commands
build:
	@echo "Building Rust binary..."
	cargo build --release

# Set environment variables only for the dev command
dev: export RUST_LOG = rpc_gateway=debug,actix_web=info,reqwest=info
dev: export RUST_BACKTRACE = 1
dev:
	@echo "Starting development server with file watching..."
	@echo "Press Ctrl+C to stop"
	@echo "----------------------------------------"
	@echo "Server will be available at: http://localhost:9090"
	@echo "----------------------------------------"
	@echo "Logs will be written to: logs/rpc-gateway.log"
	@echo "----------------------------------------"
	@echo "Starting server..."
	@mkdir -p logs
	@cd rpc-gateway-core && watchexec -e rs -r cargo run -- -c $(PWD)/example.config.toml

test:
	@echo "Running tests..."
	cargo test

lint:
	@echo "Running lints..."
	cargo clippy -- -D warnings
	cargo fmt -- --check

# Docker commands
docker-build:
	@echo "Building Docker image..."
	docker build -t $(IMAGE_NAME) .

docker-clean:
	@echo "Cleaning up Docker resources..."
	docker rmi $(IMAGE_NAME) || true

# Show help
help:
	@echo "Available targets:"
	@echo "  build         - Build the Rust binary"
	@echo "  dev           - Start development server with file watching"
	@echo "  test          - Run tests"
	@echo "  lint          - Run lints"
	@echo "  docker-build  - Build the Docker image"
	@echo "  docker-run    - Run the Docker container"
	@echo "  docker-clean  - Remove the Docker image"
	@echo "  help          - Show this help message" 