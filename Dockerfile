# Build stage
FROM rustlang/rust:nightly-slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    perl \
    make \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy Cargo files first for better caching
COPY sol-sim/Cargo.toml sol-sim/Cargo.lock ./

# Copy source code
COPY sol-sim/src ./src

RUN cargo build --release

# Runtime stage
FROM debian:trixie-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /app/target/release/sol-sim /usr/local/bin/sol-sim

# Create a non-root user
RUN useradd -m -u 1000 solsim && \
    chown -R solsim:solsim /app

USER solsim

# Expose the port
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Run the application
ENTRYPOINT ["sol-sim"]
CMD ["--port", "8080", "--base-url", "http://localhost:8080"]