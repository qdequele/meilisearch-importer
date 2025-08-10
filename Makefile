.PHONY: build build-release test run clean help

# Default target
all: build

# Build debug version
build:
	@echo "🔨 Building debug version..."
	source /usr/local/cargo/env && cargo build --bin meilisearch-importer-server

# Build release version
build-release:
	@echo "🚀 Building release version..."
	source /usr/local/cargo/env && cargo build --release --bin meilisearch-importer-server

# Run tests
test:
	@echo "🧪 Running tests..."
	source /usr/local/cargo/env && cargo test

# Run the server
run: build
	@echo "🚀 Starting server..."
	./target/debug/meilisearch-importer-server --config example_config.json --port 8080

# Run the server in release mode
run-release: build-release
	@echo "🚀 Starting server in release mode..."
	./target/release/meilisearch-importer-server --config example_config.json --port 8080

# Run the test script
test-server: build
	@echo "🧪 Running server tests..."
	./test_server.sh

# Run the demo script
demo: build
	@echo "🎯 Running demo..."
	./demo.sh

# Check code
check:
	@echo "🔍 Checking code..."
	source /usr/local/cargo/env && cargo check

# Format code
fmt:
	@echo "✨ Formatting code..."
	source /usr/local/cargo/env && cargo fmt

# Clippy linting
clippy:
	@echo "🔍 Running clippy..."
	source /usr/local/cargo/env && cargo clippy

# Clean build artifacts
clean:
	@echo "🧹 Cleaning build artifacts..."
	source /usr/local/cargo/env && cargo clean

# Install dependencies (if needed)
install-deps:
	@echo "📦 Installing system dependencies..."
	sudo apt-get update
	sudo apt-get install -y libssl-dev pkg-config jq

# Show help
help:
	@echo "Meilisearch Importer Server - Available commands:"
	@echo ""
	@echo "  build          - Build debug version"
	@echo "  build-release  - Build release version"
	@echo "  test           - Run tests"
	@echo "  run            - Build and run server (debug)"
	@echo "  run-release    - Build and run server (release)"
	@echo "  test-server    - Run server tests"
	@echo "  demo           - Run demo script"
	@echo "  check          - Check code without building"
	@echo "  fmt            - Format code"
	@echo "  clippy         - Run clippy linting"
	@echo "  clean          - Clean build artifacts"
	@echo "  install-deps   - Install system dependencies"
	@echo "  help           - Show this help message"
	@echo ""
	@echo "Examples:"
	@echo "  make run       # Build and start server"
	@echo "  make demo      # Run comprehensive demo"
	@echo "  make test      # Run all tests"