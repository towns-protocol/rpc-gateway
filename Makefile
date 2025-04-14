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

##@ Minikube

.PHONY: minikube-deploy
minikube-deploy: ## Deploy the Helm chart to Minikube.
	@echo "Building Helm dependencies..."
	cd ./helm/minikube-example && helm dependency build
	@echo "Deploying Helm chart to Minikube..."
	helm upgrade --install rpc-gateway ./helm/minikube-example \
		--namespace default \
		--create-namespace \
		--set image.pullPolicy=Always

.PHONY: minikube-delete
minikube-delete: ## Delete the Helm chart from Minikube.
	@echo "Deleting Helm chart from Minikube..."
	helm uninstall rpc-gateway --namespace default

.PHONY: minikube-set-rpc-urls
minikube-set-rpc-urls: ## Set RPC URLs in Minikube secret. Reads Alchemy URL from stdin.
	@echo "Setting RPC URLs in Minikube secret..."
	@read -p "Enter Alchemy URL (or pipe it in): " ALCHEMY_URL; \
	if ! kubectl get secret rpc-gateway-upstream-urls >/dev/null 2>&1; then \
		kubectl create secret generic rpc-gateway-upstream-urls \
			--from-literal=ALCHEMY_URL="$$ALCHEMY_URL"; \
	else \
		kubectl create secret generic rpc-gateway-upstream-urls \
			--from-literal=ALCHEMY_URL="$$ALCHEMY_URL" \
			--dry-run=client -o yaml | \
		kubectl apply -f -; \
	fi

.PHONY: minikube-test
minikube-test: ## Test the Minikube gateway by sending an eth_getBlock request.
	@echo "Testing Minikube gateway..."
	@echo "Starting port-forward..."
	@kubectl port-forward svc/rpc-gateway 8080:8080 > /dev/null 2>&1 & \
		PORT_FORWARD_PID=$$!; \
		sleep 2; \
		echo "Sending test request..."; \
		curl -X POST http://localhost:8080/1 \
			-H "Content-Type: application/json" \
			-d '{"jsonrpc":"2.0","method":"eth_getBlockByNumber","params":["latest",false],"id":1}'; \
		kill $$PORT_FORWARD_PID

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