# Build stage
FROM rust:1.85-slim-bullseye as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
  pkg-config \
  libssl-dev \
  && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /usr/src/rpc-gateway

# Copy the source code
COPY . .

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bullseye-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
  libssl1.1 \
  ca-certificates \
  && rm -rf /var/lib/apt/lists/*

# Copy the binary from the builder stage
COPY --from=builder /usr/src/rpc-gateway/target/release/rpc-gateway /usr/local/bin/rpc-gateway

# Set the working directory
WORKDIR /app

# Copy the example config
COPY example.config.toml /app/config.toml

# Expose the port (adjust if your application uses a different port)
EXPOSE 8080

# Run the application with config file path
CMD ["rpc-gateway", "/app/config.toml"] 