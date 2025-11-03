#!/bin/bash

# Startup script for Solana Fork Simulation Engine

set -e

# Configuration
REDIS_PORT=6379
API_GATEWAY_PORT=8080
RPC_GATEWAY_PORT=8081
FORK_MANAGER_PORT=8082

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Function to check if a port is in use
check_port() {
    local port=$1
    local service=$2
    if lsof -Pi :$port -sTCP:LISTEN -t >/dev/null 2>&1; then
        print_warning "$service port $port is already in use"
        return 1
    fi
    return 0
}

# Function to wait for service to be ready
wait_for_service() {
    local url=$1
    local service=$2
    local timeout=${3:-30}
    
    print_status "Waiting for $service to be ready..."
    
    for i in $(seq 1 $timeout); do
        if curl -s "$url" >/dev/null 2>&1; then
            print_success "$service is ready!"
            return 0
        fi
        sleep 1
    done
    
    print_error "$service failed to start within ${timeout}s"
    return 1
}

# Function to cleanup background processes
cleanup() {
    print_status "Shutting down services..."
    
    if [ ! -z "$API_GATEWAY_PID" ]; then
        kill $API_GATEWAY_PID 2>/dev/null || true
    fi
    
    if [ ! -z "$RPC_GATEWAY_PID" ]; then
        kill $RPC_GATEWAY_PID 2>/dev/null || true
    fi
    
    # Kill any remaining fork-worker processes
    pkill -f fork-worker 2>/dev/null || true
    
    print_success "Services stopped"
    exit 0
}

# Set up signal handlers
trap cleanup SIGINT SIGTERM

print_status "Starting Solana Fork Simulation Engine..."

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ]; then
    print_error "Please run this script from the project root directory"
    exit 1
fi

# Build the project
print_status "Building the project..."
if ! cargo build --release; then
    print_error "Failed to build the project"
    exit 1
fi
print_success "Project built successfully"

# Check Redis
print_status "Checking Redis connection..."
if ! redis-cli ping >/dev/null 2>&1; then
    print_warning "Redis is not running. Starting Redis..."
    # Try to start Redis in the background
    redis-server --daemonize yes --port $REDIS_PORT
    sleep 2
    
    if ! redis-cli ping >/dev/null 2>&1; then
        print_error "Failed to start Redis. Please install and start Redis manually."
        exit 1
    fi
fi
print_success "Redis is running"

# Check ports availability
print_status "Checking port availability..."
if ! check_port $API_GATEWAY_PORT "API Gateway"; then
    exit 1
fi

if ! check_port $RPC_GATEWAY_PORT "RPC Gateway"; then
    exit 1
fi

# Set environment variables
export RUST_LOG="${RUST_LOG:-info}"
export REDIS_URL="${REDIS_URL:-redis://localhost:$REDIS_PORT}"
export MAINNET_RPC_URL="${MAINNET_RPC_URL:-https://api.mainnet-beta.solana.com}"
export BASE_RPC_URL="http://localhost:$RPC_GATEWAY_PORT"

print_status "Environment configuration:"
print_status "  RUST_LOG: $RUST_LOG"
print_status "  REDIS_URL: $REDIS_URL"
print_status "  MAINNET_RPC_URL: $MAINNET_RPC_URL"
print_status "  BASE_RPC_URL: $BASE_RPC_URL"

# Start RPC Gateway
print_status "Starting RPC Gateway on port $RPC_GATEWAY_PORT..."
./target/release/rpc-gateway --port $RPC_GATEWAY_PORT &
RPC_GATEWAY_PID=$!
wait_for_service "http://localhost:$RPC_GATEWAY_PORT/health" "RPC Gateway"

# Start API Gateway
print_status "Starting API Gateway on port $API_GATEWAY_PORT..."
./target/release/api-gateway --port $API_GATEWAY_PORT &
API_GATEWAY_PID=$!
wait_for_service "http://localhost:$API_GATEWAY_PORT/health" "API Gateway"

print_success "All services are running!"
print_status ""
print_status "Service endpoints:"
print_status "  API Gateway:  http://localhost:$API_GATEWAY_PORT"
print_status "  RPC Gateway:  http://localhost:$RPC_GATEWAY_PORT"
print_status ""
print_status "Example API usage:"
print_status "  curl -X POST http://localhost:$API_GATEWAY_PORT/forks \\"
print_status "    -H 'Content-Type: application/json' \\"
print_status "    -H 'Authorization: Bearer test-api-key-12345678901234567890' \\"
print_status "    -d '{\"accounts\": [\"EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v\"]}'"
print_status ""
print_status "Press Ctrl+C to stop all services"

# Keep the script running
wait
