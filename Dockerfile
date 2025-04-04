# Build stage
FROM rust:1.85-slim-bullseye AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
  pkg-config \
  libssl-dev \
  && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /usr/src/rpc-gateway

# Copy only the necessary files for building
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bullseye-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
  libssl1.1 \
  ca-certificates \
  && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 rpc-gateway

# Set the working directory
WORKDIR /app

# Copy the binary from the builder stage
COPY --from=builder /usr/src/rpc-gateway/target/release/rpc-gateway /usr/local/bin/rpc-gateway

# Copy the example config
COPY docker.config.toml /app/config.toml

# Set proper permissions
RUN chown -R rpc-gateway:rpc-gateway /app

# Switch to non-root user
USER rpc-gateway

# Expose the port the app runs on
EXPOSE 8080

# Set the entrypoint
ENTRYPOINT ["/usr/local/bin/rpc-gateway", "-c", "/app/config.toml"] 