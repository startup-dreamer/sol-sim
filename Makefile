.PHONY: build run clean test help

help:
	@echo "Solana Fork Simulation Engine (MVP)"
	@echo ""
	@echo "Commands:"
	@echo "  make build   - Build release binary"
	@echo "  make run     - Start the service"
	@echo "  make clean   - Clean build artifacts"
	@echo "  make test    - Run tests"
	@echo "  make dev     - Run in development mode"

build:
	@echo "Building..."
	cargo build --release

run: build
	@echo "Starting service..."
	@./scripts/start.sh

dev:
	@echo "Running in development mode..."
	RUST_LOG=debug cargo run

clean:
	@echo "Cleaning..."
	cargo clean

test:
	@echo "Running tests..."
	cargo test

check:
	@echo "Checking code..."
	cargo clippy

format:
	@echo "Formatting code..."
	cargo fmt
