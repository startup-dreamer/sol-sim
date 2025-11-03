#!/bin/bash

set -e

echo "🚀 Starting Solana Fork Simulation Engine (MVP)"

# Check Redis
if ! redis-cli ping > /dev/null 2>&1; then
    echo "⚠️  Redis not running. Starting Redis..."
    redis-server --daemonize yes
    sleep 1
fi

echo "✅ Redis is running"

# Build
echo "📦 Building..."
cargo build --release

# Run
echo "🎯 Starting server on testnet..."
./target/release/sol-sim \
    --port 8080 \
    --redis-url redis://localhost:6379 \
    --solana-rpc https://api.testnet.solana.com \
    --base-url http://localhost:8080


