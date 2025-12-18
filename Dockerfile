# Multi-stage build for Polymarket Arbitrage Bot
# Stage 1: Build
FROM rust:latest as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app

# Copy dependency files first (for better caching)
COPY Cargo.toml ./

# Create a dummy src/main.rs to build dependencies
# Cargo.lock will be generated automatically if missing
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo generate-lockfile 2>/dev/null || true && \
    cargo build --release && \
    rm -rf src

# Copy source code
COPY src ./src
COPY tests ./tests
COPY scripts ./scripts

# Build the actual application
RUN touch src/main.rs && \
    cargo build --release

# Stage 2: Runtime
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create app user for security
RUN useradd -m -u 1000 appuser

# Set working directory
WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/arb-bot /app/arb-bot

# Copy scripts if needed
COPY --from=builder /app/scripts ./scripts

# Change ownership
RUN chown -R appuser:appuser /app

# Switch to non-root user
USER appuser

# Expose any ports if needed (currently none, but WebSocket connections)
# EXPOSE 8080

# Set environment variables defaults
ENV RUST_LOG=info
ENV DRY_RUN=1

# Run the bot
CMD ["./arb-bot"]

