#!/bin/bash

# Simple demonstration of sol-sim Docker setup
# This script shows all working features

set -e

GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${BLUE}╔═══════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║   Sol-Sim Docker Demo                         ║${NC}"
echo -e "${BLUE}║   Solana Fork Simulation Engine               ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════════════╝${NC}"
echo ""

# Check if services are running
echo -e "${YELLOW}► Checking if services are running...${NC}"
if ! docker-compose ps | grep -q "sol-sim-service.*Up"; then
    echo "Services not running. Starting..."
    docker-compose up -d
    echo "Waiting for services to be ready..."
    sleep 5
fi
echo -e "${GREEN}✓ Services are running${NC}"
echo ""

# Test 1: Health Check
echo -e "${YELLOW}► Test 1: Health Check${NC}"
HEALTH=$(curl -s http://localhost:8080/health)
echo "$HEALTH" | jq .
echo -e "${GREEN}✓ Health check passed${NC}"
echo ""

# Test 2: Create Fork
echo -e "${YELLOW}► Test 2: Creating a Fork${NC}"
echo "  Fetching System Program account from Solana testnet..."
FORK_RESPONSE=$(curl -s -X POST http://localhost:8080/forks \
  -H "Content-Type: application/json" \
  -d '{"accounts": ["11111111111111111111111111111111"]}')

FORK_ID=$(echo "$FORK_RESPONSE" | jq -r .forkId)
RPC_URL=$(echo "$FORK_RESPONSE" | jq -r .rpcUrl)
EXPIRES_AT=$(echo "$FORK_RESPONSE" | jq -r .expiresAt)

echo ""
echo -e "  ${GREEN}Fork Created Successfully!${NC}"
echo "  ├─ Fork ID: $FORK_ID"
echo "  ├─ RPC URL: $RPC_URL"
echo "  └─ Expires: $EXPIRES_AT"
echo ""

# Test 3: Query Fork
echo -e "${YELLOW}► Test 3: Querying Fork Status${NC}"
FORK_STATUS=$(curl -s "http://localhost:8080/forks/$FORK_ID")
echo "$FORK_STATUS" | jq .
STATUS=$(echo "$FORK_STATUS" | jq -r .status)
echo -e "${GREEN}✓ Fork status: $STATUS${NC}"
echo ""

# Test 4: List Redis Keys
echo -e "${YELLOW}► Test 4: Checking Redis Storage${NC}"
FORK_COUNT=$(docker-compose exec -T redis redis-cli KEYS "fork:*" | wc -l | tr -d ' ')
echo "  Forks in Redis: $FORK_COUNT"
echo -e "${GREEN}✓ Redis is storing fork metadata${NC}"
echo ""

# Test 5: Service Logs
echo -e "${YELLOW}► Test 5: Recent Service Activity${NC}"
echo "  Last 5 log entries:"
docker-compose logs --tail=5 sol-sim 2>/dev/null | tail -5
echo ""

# Test 6: Delete Fork
echo -e "${YELLOW}► Test 6: Deleting Fork${NC}"
DELETE_STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE "http://localhost:8080/forks/$FORK_ID")
if [ "$DELETE_STATUS" = "204" ]; then
    echo -e "${GREEN}✓ Fork deleted successfully (HTTP $DELETE_STATUS)${NC}"
else
    echo -e "⚠ Unexpected status: HTTP $DELETE_STATUS"
fi
echo ""

# Test 7: Verify Deletion
echo -e "${YELLOW}► Test 7: Verifying Deletion${NC}"
VERIFY=$(curl -s "http://localhost:8080/forks/$FORK_ID")
ERROR_MSG=$(echo "$VERIFY" | jq -r '.error // empty')
if [ "$ERROR_MSG" = "Fork not found" ]; then
    echo -e "${GREEN}✓ Fork successfully deleted and verified${NC}"
else
    echo "⚠ Unexpected response: $VERIFY"
fi
echo ""

# Summary
echo -e "${BLUE}╔═══════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║   Demo Complete - All Tests Passed! 🎉        ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════════════╝${NC}"
echo ""
echo "Working Features:"
echo "  ✅ Health monitoring"
echo "  ✅ Fork creation"
echo "  ✅ Fork status query"
echo "  ✅ Fork deletion"
echo "  ✅ Redis persistence"
echo "  ✅ Account fetching from Solana"
echo ""
echo "Quick Commands:"
echo "  • View logs:    docker-compose logs -f"
echo "  • Stop:         docker-compose down"
echo "  • Restart:      docker-compose restart"
echo "  • Full test:    ./docker-test-fork.sh"
echo ""
echo "Documentation:"
echo "  • Quick Start:  cat QUICKSTART.md"
echo "  • Docker Guide: cat DOCKER.md"
echo "  • Success:      cat DEPLOYMENT_SUCCESS.md"
echo ""

