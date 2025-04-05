#!/bin/bash

# Exit on error
set -e

# Configuration
IMAGE_NAME="whatsgood/rpc-gateway"
VERSION=$(git describe --tags --always --dirty)

# Check if buildx is available and set up if needed
if ! docker buildx inspect multiplatform >/dev/null 2>&1; then
  echo "Setting up Docker Buildx for multi-platform builds..."
  docker buildx create --name multiplatform --driver docker-container --use
fi

# Use buildx to build and push multi-platform images
echo "Building and pushing multi-platform images..."
docker buildx build \
  --platform linux/amd64,linux/arm64 \
  --tag ${IMAGE_NAME}:${VERSION} \
  --tag ${IMAGE_NAME}:latest \
  --push \
  .

echo "Successfully published ${IMAGE_NAME}:${VERSION} for multiple platforms"
echo "Successfully published ${IMAGE_NAME}:latest for multiple platforms" 