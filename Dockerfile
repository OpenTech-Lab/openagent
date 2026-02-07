# ==============================================================================
# OpenAgent Dockerfile
# Multi-stage build for optimized production image
# ==============================================================================

# Stage 1: Build environment
FROM rust:1.88-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy manifests first for better layer caching
COPY Cargo.toml Cargo.lock ./

# Create a dummy src to cache dependencies
RUN mkdir -p src/bin && \
    echo "fn main() {}" > src/bin/cli.rs && \
    echo "fn main() {}" > src/bin/gateway.rs && \
    echo "fn main() {}" > src/bin/tui.rs && \
    echo "pub fn dummy() {}" > src/lib.rs

# Build dependencies only (this layer will be cached)
RUN cargo build --release && \
    rm -rf src && \
    rm -rf target/release/.fingerprint/openagent-* && \
    rm -rf target/release/deps/openagent* && \
    rm -rf target/release/deps/libopenagent* && \
    rm -rf target/release/incremental/openagent*

# Copy actual source code
COPY src ./src

# Copy migrations if they exist (create empty dir if not)
RUN mkdir -p migrations
COPY migrations/ ./migrations/

COPY SOUL.md ./

# Build the actual binaries
RUN cargo build --release

# Collect ONNX Runtime shared library (needed by fastembed at runtime)
RUN mkdir -p /app/onnxruntime && \
    find target/release/build -name "libonnxruntime.so*" -exec cp {} /app/onnxruntime/ \;

# Stage 2: Runtime environment
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libgomp1 \
    docker.io \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binaries from builder
COPY --from=builder /app/target/release/openagent /usr/local/bin/openagent
COPY --from=builder /app/target/release/openagent-gateway /usr/local/bin/openagent-gateway
COPY --from=builder /app/target/release/openagent-tui /usr/local/bin/openagent-tui

# Copy ONNX Runtime library for fastembed
COPY --from=builder /app/onnxruntime/ /usr/lib/
RUN ldconfig

# Copy essential files
COPY --from=builder /app/SOUL.md /app/SOUL.md
COPY --from=builder /app/migrations /app/migrations
# Create workspace directory and model cache directory
RUN mkdir -p /app/workspace /app/.cache

# Set environment variables
ENV RUST_LOG=info,openagent=debug
ENV ALLOWED_DIR=/app/workspace
ENV HF_HOME=/app/.cache

# Default command (can be overridden)
ENTRYPOINT ["openagent"]
CMD ["--help"]

# ==============================================================================
# Gateway variant (for running the gateway service)
# ==============================================================================
FROM runtime AS gateway

EXPOSE 8080

ENTRYPOINT ["openagent-gateway"]
CMD []
