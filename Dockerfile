# Build stage
FROM rust:1.85-slim-bullseye AS builder

RUN apt-get update && apt-get install -y \
  build-essential \
  pkg-config \
  libssl-dev \
  cmake \
  make \
  curl \
  git \
  clang \
  gcc \
  libc6-dev

RUN rm -rf /var/lib/apt/lists/*

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

# Set proper permissions
RUN chown -R rpc-gateway:rpc-gateway /app

# Switch to non-root user
USER rpc-gateway

# Expose the port the app runs on
EXPOSE 8080

VOLUME [ "/etc/rpc-gateway" ]

# Set the entrypoint
ENTRYPOINT ["/usr/local/bin/rpc-gateway", "-c", "/etc/rpc-gateway/config.yml"] 