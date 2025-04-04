#!/bin/bash

# Exit on error
set -e

# Configuration
IMAGE_NAME="whatsgood/rpc-gateway"
VERSION=$(git describe --tags --always --dirty)

# Build the image
make docker-build

# Tag the image for the registry
echo "Tagging image..."
docker tag ${IMAGE_NAME}:${VERSION} ${IMAGE_NAME}:latest

# Push the image
echo "Pushing image to Docker Hub..."
docker push ${IMAGE_NAME}:${VERSION}
docker push ${IMAGE_NAME}:latest

echo "Successfully published ${IMAGE_NAME}:${VERSION}"
echo "Successfully published ${IMAGE_NAME}:latest" 