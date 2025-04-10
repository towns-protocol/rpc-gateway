.PHONY: build docker-build dev docker-run clean docker-clean help test lint

# Docker image name
IMAGE_NAME := rpc-gateway
DOCKER_REGISTRY := whatsgood
FULL_IMAGE_NAME := $(DOCKER_REGISTRY)/$(IMAGE_NAME)
DOCKER_IMAGE_VERSION := $(shell git describe --tags --always --dirty)

# Ensure shell commands exit on error
.SHELLFLAGS := -e

# Default target
all: build

# Local development commands
build:
	@echo "Building Rust binary..."
	cargo build --release

# Set environment variables only for the dev command
dev: export RUST_BACKTRACE = 1
dev:
	@echo "Starting development server with file watching..."
	@echo "Press Ctrl+C to stop"
	@echo "----------------------------------------"
	@echo "Server will be available at: http://localhost:8080"
	@echo "----------------------------------------"
	@echo "Logs will be written to: logs/rpc-gateway.log"
	@echo "----------------------------------------"
	@echo "Starting server..."
	@mkdir -p logs
	watchexec -e rs -r cargo run -- -c $(PWD)/example.config.yml

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

docker-publish:
	@echo "Setting up Docker Buildx for multi-platform builds..."
	@if ! docker buildx inspect multiplatform >/dev/null 2>&1; then \
		docker buildx create --name multiplatform --driver docker-container --use; \
	fi
	@echo "Building and pushing multi-platform images..."
	docker buildx build \
		--platform linux/amd64,linux/arm64 \
		--tag $(FULL_IMAGE_NAME):$(DOCKER_IMAGE_VERSION) \
		--tag $(FULL_IMAGE_NAME):latest \
		--push \
		.
	@echo "Successfully published $(FULL_IMAGE_NAME):$(DOCKER_IMAGE_VERSION) for multiple platforms"
	@echo "Successfully published $(FULL_IMAGE_NAME):latest for multiple platforms"

helm-build:
	@echo "Building Helm chart..."
	cd ./helm && \
		helm package rpc-gateway && \
		helm repo index . --url https://whats-good.github.io/rpc-gateway/helm

helm-publish:
	@echo "Publishing Helm chart..."
	@HELM_VERSION=$$(yq '.version' ./helm/rpc-gateway/Chart.yaml) && \
		echo "Creating git tag helm-v$$HELM_VERSION" && \
		git tag -a "helm-v$$HELM_VERSION" -m "Helm chart version $$HELM_VERSION" && \
		git push origin "helm-v$$HELM_VERSION"

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
	@echo "  docker-publish - Publish the Docker image"
	@echo "  help          - Show this help message" 