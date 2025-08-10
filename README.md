# Meilisearch Importer Server

A high-performance HTTP server for importing data into Meilisearch instances. This server provides the same functionality as the CLI tool but through a RESTful HTTP API, making it easy to integrate data imports into your applications and automation pipelines.

## Features

- 🚀 **High Performance**: Built with Actix Web for maximum throughput
- 📊 **Multiple Formats**: Supports JSON, NDJSON, and CSV files
- 🔄 **Streaming**: Processes large files without loading them entirely into memory
- 📦 **Batch Processing**: Configurable batch sizes for optimal performance
- ⚡ **Parallel Processing**: Multiple concurrent jobs for faster imports
- 🗜️ **Compression**: Gzip compression for efficient data transfer
- 🔁 **Error Handling**: Exponential backoff retry logic for failed requests
- 🏥 **Health Monitoring**: Built-in health check endpoint
- 🔒 **Secure**: API key authentication support

## Installation

### Prerequisites

- Rust 1.70+ and Cargo
- OpenSSL development libraries

### Build

```bash
# Clone the repository
git clone <repository-url>
cd meilisearch-importer

# Build the server
cargo build --bin meilisearch-importer-server

# Or build in release mode
cargo build --release --bin meilisearch-importer-server
```

### System Dependencies

On Ubuntu/Debian:
```bash
sudo apt-get install libssl-dev pkg-config
```

On CentOS/RHEL:
```bash
sudo yum install openssl-devel pkg-config
```

## Usage

### Starting the Server

```bash
# Start on default port (8080)
./target/debug/meilisearch-importer-server

# Start on custom port
./target/debug/meilisearch-importer-server --port 9000

# Enable debug logging
./target/debug/meilisearch-importer-server --debug
```

### API Endpoints

#### Health Check

```bash
GET /health
```

Response:
```json
{
  "status": "healthy",
  "service": "meilisearch-importer-server",
  "version": "0.2.4"
}
```

#### Import Data

```bash
POST /import
Content-Type: application/json

{
  "url": "https://example.com/data.json",
  "meilisearch_url": "http://localhost:7700",
  "index": "documents",
  "primary_key": "id",
  "api_key": "your_master_key",
  "batch_size": "2 MiB",
  "jobs": 4,
  "upload_operation": "add_or_replace"
}
```

**Required Fields:**
- `url`: The URL of the data file to import
- `meilisearch_url`: Your Meilisearch instance URL
- `index`: The target index name

**Optional Fields:**
- `primary_key`: Field name for document identification
- `api_key`: Meilisearch API key for authentication
- `csv_delimiter`: CSV delimiter (default: ",")
- `ignore_embeddings`: Whether to ignore embeddings (default: false)
- `batch_size`: Batch size in human-readable format (default: "1 MiB")
- `jobs`: Number of parallel jobs (default: 2)
- `upload_operation`: "add_or_replace" or "add_or_update" (default: "add_or_replace")

### Response Format

**Success Response:**
```json
{
  "status": "success",
  "message": "Import completed successfully",
  "batches_processed": 5,
  "total_documents": 1250
}
```

**Error Response:**
```json
{
  "status": "error",
  "message": "Import failed: Failed to download file: HTTP 404",
  "batches_processed": null,
  "total_documents": null
}
```

## How It Works

1. **Request Processing**: Server receives import request with full configuration
2. **Data Download**: Downloads data from the specified URL using streaming
3. **Format Detection**: Automatically detects file format (JSON, NDJSON, CSV)
4. **Batch Processing**: Parses data and creates batches according to size configuration
5. **Parallel Upload**: Sends batches to Meilisearch using multiple concurrent jobs
6. **Compression**: Applies Gzip compression for efficient transfer
7. **Error Handling**: Implements retry logic with exponential backoff

## Supported File Formats

### JSON
- Standard JSON arrays of objects
- Automatically handles array structure
- Example: `[{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]`

### NDJSON (Newline-Delimited JSON)
- One JSON object per line
- Efficient for large datasets
- Example: `{"id": 1, "name": "Alice"}\n{"id": 2, "name": "Bob"}`

### CSV
- Comma-separated values with header row
- Configurable delimiter
- Example: `id,name\n1,Alice\n2,Bob`

## Performance Considerations

- **Batch Sizes**: Larger batches reduce HTTP overhead but increase memory usage
- **Parallel Jobs**: More jobs increase throughput but may overwhelm Meilisearch
- **Memory Usage**: Streaming ensures constant memory usage regardless of file size
- **Network**: Gzip compression reduces transfer time and bandwidth usage

## Error Handling

The server implements comprehensive error handling:

- **Download Failures**: Retries with exponential backoff
- **Upload Failures**: Automatic retry for transient errors
- **Format Errors**: Graceful handling of malformed data
- **Network Issues**: Connection timeout and retry logic

## Security

- **API Key Authentication**: Secure handling of Meilisearch API keys
- **Input Validation**: URL and configuration validation
- **Error Messages**: No sensitive information in error responses
- **Rate Limiting**: Built-in batch processing prevents overwhelming Meilisearch

## Monitoring

- **Health Endpoint**: Monitor server status and version
- **Structured Logging**: Comprehensive logging for debugging and monitoring
- **Progress Tracking**: Batch-level progress information
- **Error Reporting**: Detailed error messages for troubleshooting

## Examples

### Basic Import

```bash
curl -X POST http://localhost:8080/import \
  -H "Content-Type: application/json" \
  -d '{
    "url": "https://example.com/users.json",
    "meilisearch_url": "http://localhost:7700",
    "index": "users",
    "primary_key": "user_id"
  }'
```

### Import with Custom Configuration

```bash
curl -X POST http://localhost:8080/import \
  -H "Content-Type: application/json" \
  -d '{
    "url": "https://example.com/products.csv",
    "meilisearch_url": "http://localhost:7700",
    "index": "products",
    "primary_key": "sku",
    "batch_size": "5 MiB",
    "jobs": 8,
    "upload_operation": "add_or_update"
  }'
```

### Health Check

```bash
curl http://localhost:8080/health
```

## Troubleshooting

### Common Issues

1. **Build Failures**: Ensure OpenSSL development libraries are installed
2. **Connection Errors**: Verify Meilisearch instance is running and accessible
3. **Authentication Failures**: Check API key and permissions
4. **Memory Issues**: Reduce batch size or number of parallel jobs

### Debug Mode

Enable debug logging for detailed information:

```bash
./target/debug/meilisearch-importer-server --debug
```

### Logs

The server provides comprehensive logging:
- Request processing details
- Format detection results
- Batch processing progress
- Error details and retry attempts

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Support

For issues and questions:
1. Check the troubleshooting section
2. Review the logs with debug mode enabled
3. Open an issue on GitHub

---

**Note**: This server maintains 100% compatibility with the CLI tool's import logic while providing a clean HTTP interface for easy integration.
