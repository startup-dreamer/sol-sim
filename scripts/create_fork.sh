#!/bin/bash

# Simple fork creation example

API_URL="http://localhost:8080"
USDC_MINT="EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"

echo "Creating fork with USDC mint account..."

RESPONSE=$(curl -s -X POST "$API_URL/forks" \
  -H "Content-Type: application/json" \
  -d "{\"accounts\": [\"$USDC_MINT\"]}")

if [ $? -eq 0 ]; then
    echo "✅ Fork created!"
    echo "$RESPONSE" | jq .
    
    FORK_ID=$(echo "$RESPONSE" | jq -r .forkId)
    RPC_URL=$(echo "$RESPONSE" | jq -r .rpcUrl)
    
    echo ""
    echo "Your fork is ready:"
    echo "  Fork ID: $FORK_ID"
    echo "  RPC URL: $RPC_URL"
    echo ""
    echo "Use in your code:"
    echo "  const connection = new Connection('$RPC_URL');"
else
    echo "❌ Failed to create fork"
    echo "$RESPONSE"
fi
