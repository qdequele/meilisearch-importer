# Meilisearch Importer Server

This is an HTTP server version of the Meilisearch Importer CLI tool. It provides a REST API for importing data from URLs into Meilisearch instances.

## Features

- **Streaming imports**: Downloads and processes files in chunks to handle large datasets
- **Automatic format detection**: Identifies file format from content-type headers or URL extensions
- **Batch processing**: Sends data to Meilisearch in configurable batch sizes
- **Parallel processing**: Configurable number of parallel jobs for faster imports
- **Retry logic**: Built-in exponential backoff for failed requests
- **Compression**: Automatically compresses data before sending to Meilisearch

## Supported Formats

- **JSON**: Arrays of objects or single objects
- **NDJSON/JSONL**: Newline-delimited JSON
- **CSV**: Comma-separated values (configurable delimiter)

## Installation

Build the server binary:

```bash
cargo build --release --bin meilisearch-importer-server
```

## Configuration

Create a `config.json` file with your Meilisearch instance details:

```json
{
  "meilisearch_url": "https://your-meilisearch-instance.com",
  "index": "your-index-name",
  "primary_key": "id",
  "api_key": "your-api-key-here",
  "csv_delimiter": ",",
  "ignore_embeddings": false,
  "batch_size": "20 MiB",
  "jobs": 4,
  "upload_operation": "add_or_replace"
}
```

### Configuration Options

- `meilisearch_url`: Your Meilisearch instance URL
- `index`: Target index name
- `primary_key`: Field to use as unique identifier (optional)
- `api_key`: API key with documents.add permission
- `csv_delimiter`: Delimiter for CSV files (default: ",")
- `ignore_embeddings`: Skip embedding fields in NDJSON (default: false)
- `batch_size`: Size of batches sent to Meilisearch (default: "20 MiB")
- `jobs`: Number of parallel upload jobs (default: 1)
- `upload_operation`: Either "add_or_replace" or "add_or_update" (default: "add_or_replace")

## Usage

### Starting the Server

```bash
# Basic usage with default config
./target/release/meilisearch-importer-server

# Custom config file and port
./target/release/meilisearch-importer-server --config my-config.json --port 9000

# Enable debug logging
./target/release/meilisearch-importer-server --debug
```

### API Endpoints

#### Health Check
```
GET /health
```
Returns server status.

#### Import Data
```
POST /import
Content-Type: application/json

{
  "url": "https://example.com/data.json"
}
```

The request body should contain the URL of the file to import.

### Example Usage

1. **Start the server:**
   ```bash
   ./target/release/meilisearch-importer-server --config config.json
   ```

2. **Import data from a URL:**
   ```bash
   curl -X POST http://localhost:8080/import \
     -H "Content-Type: application/json" \
     -d '{"url": "https://example.com/large-dataset.json"}'
   ```

3. **Check server health:**
   ```bash
   curl http://localhost:8080/health
   ```

## How It Works

1. **URL Processing**: The server downloads the file from the provided URL
2. **Format Detection**: Automatically detects file format from content-type or URL extension
3. **Streaming**: Processes the file in chunks to handle large datasets efficiently
4. **Batching**: Accumulates data until batch size is reached, then sends to Meilisearch
5. **Compression**: Compresses each batch before sending to reduce network overhead
6. **Retry Logic**: Implements exponential backoff for failed requests

## Performance Considerations

- **Batch Size**: Larger batches reduce HTTP overhead but increase memory usage
- **Parallel Jobs**: More jobs can speed up imports but may overwhelm your Meilisearch instance
- **Network**: Ensure stable network connection for large file imports
- **Memory**: Monitor memory usage during large imports

## Error Handling

The server includes comprehensive error handling:
- Network failures with exponential backoff retry
- Invalid file formats
- Meilisearch API errors
- Configuration validation

## Security

- The server binds to localhost by default
- API keys are passed through to Meilisearch
- No persistent storage of sensitive data

## Monitoring

Enable debug logging to monitor import progress:
```bash
./target/release/meilisearch-importer-server --debug
```

## Troubleshooting

- **Import fails**: Check your Meilisearch instance URL and API key
- **Slow imports**: Adjust batch size and number of parallel jobs
- **Memory issues**: Reduce batch size for very large files
- **Network errors**: Check your internet connection and firewall settings