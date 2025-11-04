#!/bin/bash

set -e

echo "🚀 Starting Solana Fork Simulation Engine (MVP)"

# Build
echo "📦 Building..."
cargo build --release

# Run
echo "🎯 Starting server on testnet..."
./target/release/sol-sim \
    --port 8080 \
    --solana-rpc https://api.mainnet-beta.solana.com

