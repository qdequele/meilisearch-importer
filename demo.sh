#!/bin/bash

# Demo script for Meilisearch Importer Server
# Shows different use cases and data formats

set -e

echo "🎯 Meilisearch Importer Server Demo"
echo "==================================="

# Function to wait for server to be ready
wait_for_server() {
    echo "⏳ Waiting for server to be ready..."
    for i in {1..30}; do
        if curl -s http://localhost:8080/health >/dev/null 2>&1; then
            echo "✅ Server is ready!"
            return 0
        fi
        sleep 1
    done
    echo "❌ Server failed to start"
    return 1
}

# Function to create test data
create_test_data() {
    local format=$1
    local filename=$2
    
    case $format in
        "json")
            cat > "$filename" << 'EOF'
[
  {"id": 1, "title": "Introduction to Rust", "author": "Alice Smith", "year": 2023, "tags": ["programming", "rust"]},
  {"id": 2, "title": "Web Development with Actix", "author": "Bob Johnson", "year": 2023, "tags": ["web", "actix", "rust"]},
  {"id": 3, "title": "Search Engine Optimization", "author": "Charlie Brown", "year": 2022, "tags": ["seo", "marketing"]},
  {"id": 4, "title": "Data Science Fundamentals", "author": "Diana Prince", "year": 2023, "tags": ["data-science", "python"]},
  {"id": 5, "title": "Machine Learning Basics", "author": "Eve Wilson", "year": 2022, "tags": ["ml", "ai", "python"]}
]
EOF
            ;;
        "ndjson")
            cat > "$filename" << 'EOF'
{"id": 1, "title": "Introduction to Rust", "author": "Alice Smith", "year": 2023, "tags": ["programming", "rust"]}
{"id": 2, "title": "Web Development with Actix", "author": "Bob Johnson", "year": 2023, "tags": ["web", "actix", "rust"]}
{"id": 3, "title": "Search Engine Optimization", "author": "Charlie Brown", "year": 2022, "tags": ["seo", "marketing"]}
{"id": 4, "title": "Data Science Fundamentals", "author": "Diana Prince", "year": 2023, "tags": ["data-science", "python"]}
{"id": 5, "title": "Machine Learning Basics", "author": "Eve Wilson", "year": 2022, "tags": ["ml", "ai", "python"]}
EOF
            ;;
        "csv")
            cat > "$filename" << 'EOF'
id,title,author,year,tags
1,Introduction to Rust,Alice Smith,2023,"programming,rust"
2,Web Development with Actix,Bob Johnson,2023,"web,actix,rust"
3,Search Engine Optimization,Charlie Brown,2022,"seo,marketing"
4,Data Science Fundamentals,Diana Prince,2023,"data-science,python"
5,Machine Learning Basics,Eve Wilson,2022,"ml,ai,python"
EOF
            ;;
    esac
}

# Function to test import
test_import() {
    local format=$1
    local filename=$2
    
    echo "📤 Testing import with $format format..."
    echo "   File: $filename"
    
    # Create test data
    create_test_data "$format" "$filename"
    
    # Start HTTP server to serve the file
    python3 -m http.server 8000 >/dev/null 2>&1 &
    local http_pid=$!
    sleep 2
    
    # Test import
    local response=$(curl -s -X POST http://localhost:8080/import \
        -H "Content-Type: application/json" \
        -d "{\"url\": \"http://localhost:8000/$filename\"}")
    
    echo "   Response: $response"
    
    # Clean up
    kill $http_pid 2>/dev/null || true
    rm -f "$filename"
    
    echo ""
}

# Check if binary exists
if [ ! -f "target/debug/meilisearch-importer-server" ]; then
    echo "❌ Binary not found. Building first..."
    source /usr/local/cargo/env
    cargo build --bin meilisearch-importer-server
fi

# Start the server
echo "🚀 Starting Meilisearch Importer Server..."
./target/debug/meilisearch-importer-server --config example_config.json --port 8080 &
SERVER_PID=$!

# Wait for server to be ready
if ! wait_for_server; then
    echo "❌ Failed to start server"
    exit 1
fi

# Show server info
echo "🏥 Server health check:"
curl -s http://localhost:8080/health | jq '.' || echo "Health check failed"

echo ""
echo "🧪 Testing different data formats..."
echo "=================================="

# Test JSON format
test_import "json" "test_data.json"

# Test NDJSON format
test_import "ndjson" "test_data.ndjson"

# Test CSV format
test_import "csv" "test_data.csv"

# Test with different batch sizes
echo "🔧 Testing with different batch sizes..."
echo "   Small batch (100 KiB):"
curl -s -X POST http://localhost:8080/import \
    -H "Content-Type: application/json" \
    -d '{"url": "http://localhost:8000/test_data.json", "batch_size": "100 KiB"}' | jq '.'

echo ""
echo "   Large batch (5 MiB):"
curl -s -X POST http://localhost:8080/import \
    -H "Content-Type: application/json" \
    -d '{"url": "http://localhost:8000/test_data.json", "batch_size": "5 MiB"}' | jq '.'

# Clean up
echo ""
echo "🧹 Cleaning up..."
kill $SERVER_PID 2>/dev/null || true

echo ""
echo "✅ Demo completed!"
echo ""
echo "Key features demonstrated:"
echo "  • Multiple data formats (JSON, NDJSON, CSV)"
echo "  • Configurable batch sizes"
echo "  • Health monitoring"
echo "  • RESTful API interface"
echo ""
echo "To run the server manually:"
echo "  ./target/debug/meilisearch-importer-server --config example_config.json --port 8080"