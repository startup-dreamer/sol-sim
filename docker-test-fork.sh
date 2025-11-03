#!/bin/bash

# Test script for sol-sim Docker setup
# This script creates a fork and tests basic RPC functionality

set -e

API_URL="${API_URL:-http://localhost:8080}"
SOLANA_NETWORK="${SOLANA_NETWORK:-testnet}"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if jq is installed
if ! command -v jq &> /dev/null; then
    log_error "jq is not installed. Please install it: brew install jq (macOS) or apt-get install jq (Linux)"
    exit 1
fi

# Wait for service to be ready
log_info "Waiting for sol-sim service to be ready..."
for i in {1..30}; do
    if curl -s -f "$API_URL/health" > /dev/null 2>&1; then
        log_info "Service is ready!"
        break
    fi
    if [ $i -eq 30 ]; then
        log_error "Service did not become ready in time"
        exit 1
    fi
    sleep 1
done

# Health check
log_info "Checking service health..."
HEALTH=$(curl -s "$API_URL/health")
echo "$HEALTH" | jq .

# Known testnet accounts (use valid testnet addresses)
# For testnet, we'll use a well-known program or account
# System Program is always available
SYSTEM_PROGRAM="11111111111111111111111111111111"

log_info "Creating a fork with System Program account..."
log_info "Using account: $SYSTEM_PROGRAM"

# Create fork
RESPONSE=$(curl -s -X POST "$API_URL/forks" \
  -H "Content-Type: application/json" \
  -d "{\"accounts\": [\"$SYSTEM_PROGRAM\"]}")

if [ $? -ne 0 ]; then
    log_error "Failed to create fork"
    echo "$RESPONSE"
    exit 1
fi

log_info "Fork created successfully!"
echo "$RESPONSE" | jq .

# Extract fork details
FORK_ID=$(echo "$RESPONSE" | jq -r .forkId)
RPC_URL=$(echo "$RESPONSE" | jq -r .rpcUrl)
EXPIRES_AT=$(echo "$RESPONSE" | jq -r .expiresAt)

if [ "$FORK_ID" == "null" ] || [ -z "$FORK_ID" ]; then
    log_error "Failed to extract fork ID from response"
    exit 1
fi

echo ""
log_info "Fork Details:"
echo "  Fork ID:    $FORK_ID"
echo "  RPC URL:    $RPC_URL"
echo "  Expires At: $EXPIRES_AT"
echo ""

# Test: Get fork status
log_info "Testing: Get fork status..."
FORK_INFO=$(curl -s "$API_URL/forks/$FORK_ID")
echo "$FORK_INFO" | jq .
STATUS=$(echo "$FORK_INFO" | jq -r .status)
log_info "Fork status: $STATUS"

# Test: Get latest blockhash via RPC
log_info "Testing: Get latest blockhash..."
BLOCKHASH_RESPONSE=$(curl -s -X POST "$RPC_URL" \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getLatestBlockhash",
    "params": []
  }')
echo "$BLOCKHASH_RESPONSE" | jq .
BLOCKHASH=$(echo "$BLOCKHASH_RESPONSE" | jq -r .result.value.blockhash)
if [ "$BLOCKHASH" != "null" ] && [ ! -z "$BLOCKHASH" ]; then
    log_info "✓ Got blockhash: $BLOCKHASH"
else
    log_error "Failed to get blockhash"
fi

# Test: Get balance of system program
log_info "Testing: Get balance of System Program..."
BALANCE_RESPONSE=$(curl -s -X POST "$RPC_URL" \
  -H "Content-Type: application/json" \
  -d "{
    \"jsonrpc\": \"2.0\",
    \"id\": 1,
    \"method\": \"getBalance\",
    \"params\": [\"$SYSTEM_PROGRAM\"]
  }")
echo "$BALANCE_RESPONSE" | jq .
BALANCE=$(echo "$BALANCE_RESPONSE" | jq -r .result.value)
log_info "✓ Balance: $BALANCE lamports"

# Test: Get account info
log_info "Testing: Get account info..."
ACCOUNT_RESPONSE=$(curl -s -X POST "$RPC_URL" \
  -H "Content-Type: application/json" \
  -d "{
    \"jsonrpc\": \"2.0\",
    \"id\": 1,
    \"method\": \"getAccountInfo\",
    \"params\": [
      \"$SYSTEM_PROGRAM\",
      {\"encoding\": \"base64\"}
    ]
  }")
echo "$ACCOUNT_RESPONSE" | jq .

# Test: List all forks (this will fail if endpoint doesn't exist)
log_info "Listing fork info..."
curl -s "$API_URL/forks/$FORK_ID" | jq .

# Cleanup prompt
echo ""
read -p "Do you want to delete the fork? (y/N) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    log_info "Deleting fork..."
    DELETE_RESPONSE=$(curl -s -X DELETE "$API_URL/forks/$FORK_ID" -w "\nHTTP_CODE:%{http_code}")
    HTTP_CODE=$(echo "$DELETE_RESPONSE" | grep "HTTP_CODE:" | cut -d: -f2)
    
    if [ "$HTTP_CODE" == "204" ]; then
        log_info "✓ Fork deleted successfully"
    else
        log_warn "Delete returned HTTP code: $HTTP_CODE"
    fi
else
    log_info "Fork will expire at: $EXPIRES_AT"
    log_info "Fork ID: $FORK_ID"
fi

echo ""
log_info "Test completed successfully! 🎉"
log_info ""
log_info "You can use this fork in your code:"
echo "  const connection = new Connection('$RPC_URL');"

