#!/bin/bash

# Test script for Meilisearch Importer Server
# This script demonstrates how to use the server

set -e

echo "🚀 Testing Meilisearch Importer Server"
echo "======================================"

# Check if the binary exists
if [ ! -f "target/debug/meilisearch-importer-server" ]; then
    echo "❌ Binary not found. Building first..."
    source /usr/local/cargo/env
    cargo build --bin meilisearch-importer-server
fi

# Create a test data file
echo "📝 Creating test data..."
cat > test_data.json << 'EOF'
[
  {"id": 1, "name": "Alice", "age": 30, "city": "New York"},
  {"id": 2, "name": "Bob", "age": 25, "city": "San Francisco"},
  {"id": 3, "name": "Charlie", "age": 35, "city": "Chicago"},
  {"id": 4, "name": "Diana", "age": 28, "city": "Boston"},
  {"id": 5, "name": "Eve", "age": 32, "city": "Seattle"}
]
EOF

# Start a simple HTTP server to serve the test data
echo "🌐 Starting test HTTP server..."
python3 -m http.server 8000 &
HTTP_SERVER_PID=$!

# Wait a moment for the server to start
sleep 2

# Test the health endpoint
echo "🏥 Testing health endpoint..."
curl -s http://localhost:8080/health | jq '.' || echo "Server not running yet"

# Start the Meilisearch Importer Server in the background
echo "🚀 Starting Meilisearch Importer Server..."
./target/debug/meilisearch-importer-server --config example_config.json --port 8080 &
SERVER_PID=$!

# Wait for the server to start
echo "⏳ Waiting for server to start..."
sleep 5

# Test the health endpoint
echo "🏥 Testing health endpoint..."
curl -s http://localhost:8080/health | jq '.'

# Test the import endpoint
echo "📤 Testing import endpoint..."
curl -X POST http://localhost:8080/import \
  -H "Content-Type: application/json" \
  -d '{"url": "http://localhost:8000/test_data.json"}' | jq '.'

# Wait a moment for the import to complete
sleep 3

# Clean up
echo "🧹 Cleaning up..."
kill $SERVER_PID 2>/dev/null || true
kill $HTTP_SERVER_PID 2>/dev/null || true
rm -f test_data.json

echo "✅ Test completed!"
echo ""
echo "To run the server manually:"
echo "  ./target/debug/meilisearch-importer-server --config example_config.json --port 8080"
echo ""
echo "To test with your own data:"
echo "  curl -X POST http://localhost:8080/import \\"
echo "    -H 'Content-Type: application/json' \\"
echo "    -d '{\"url\": \"https://example.com/your-data.json\"}'"