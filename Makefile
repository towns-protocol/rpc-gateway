# Docker image name
IMAGE_NAME := rpc-gateway
DOCKER_REGISTRY := whatsgood
FULL_IMAGE_NAME := $(DOCKER_REGISTRY)/$(IMAGE_NAME)
DOCKER_IMAGE_VERSION := $(shell git describe --tags --always --dirty)

# Ensure shell commands exit on error
.SHELLFLAGS := -e

##@ Build
.PHONY: build	
build: ## Build the Rust binary.
	@echo "Building Rust binary..."
	cargo build --release

.PHONY: docker-build
docker-build: ## Build the Docker image.
	@echo "Building Docker image..."
	docker build -t $(IMAGE_NAME) .

.PHONY: docker-clean
docker-clean: ## Clean up Docker resources.
	@echo "Cleaning up Docker resources..."
	docker rmi $(IMAGE_NAME) || true

.PHONY: docker-publish
docker-publish: ## Publish the Docker image to the Docker Hub repository.
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

.PHONY: helm-build
helm-build: ## Build the Helm chart.
	@echo "Building Helm chart..."
	cd ./helm && \
		helm package rpc-gateway && \
		helm repo index . --url https://whats-good.github.io/rpc-gateway/helm

.PHONY: helm-publish
helm-publish: ## Publish the Helm chart to the GitHub Pages repository.
	@echo "Publishing Helm chart..."
	@HELM_VERSION=$$(yq '.version' ./helm/rpc-gateway/Chart.yaml) && \
		echo "Creating git tag helm-v$$HELM_VERSION" && \
		git tag -a "helm-v$$HELM_VERSION" -m "Helm chart version $$HELM_VERSION" && \
		git push origin "helm-v$$HELM_VERSION"


##@ Development

.PHONY: dev
dev: ## Start development server with file watching.
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
	
.PHONY: test
test: ## Run all tests.
	@echo "Running tests..."
	cargo test --workspace

.PHONY: coverage
coverage: ## Generate test coverage report.
	@echo "Generating test coverage report..."
	cargo tarpaulin --workspace --out Html --output-dir ./target/coverage
	@echo "Coverage report generated at ./target/coverage/tarpaulin-report.html"

.PHONY: check
check: ## Run all checks.
	@echo "Running checks..."
	cargo check --workspace

.PHONY: lint
lint: ## Run all linting checks.
	@echo "Running linting checks..."
	cargo clippy --workspace


##@ Help
# Show help
.PHONY: help
help: ## Display this help.
	@awk 'BEGIN {FS = ":.*##"; printf "Usage:\n  make \033[36m<target>\033[0m\n"} /^[a-zA-Z_0-9-]+:.*?##/ { printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)