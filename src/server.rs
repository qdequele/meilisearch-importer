use std::io::{Read, Cursor, Write};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use actix_web::{get, post, web, App, Error, HttpRequest, HttpResponse, HttpServer, Result};
use anyhow::anyhow;
use byte_unit::Byte;
use flate2::write::GzEncoder;
use flate2::Compression;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use bzip2::read::BzDecoder;
use xz2::read::XzDecoder;
use zstd::stream::Decoder;

use crate::mime::Mime;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompressionType {
    None,
    Gzip,
    Bzip2,
    Xz,
    Zstd,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ImportRequest {
    pub url: String,
    pub meilisearch_url: String,
    pub index: String,
    pub primary_key: Option<String>,
    pub api_key: Option<String>,
    #[serde(default = "default_csv_delimiter")]
    pub csv_delimiter: u8,
    #[serde(default)]
    pub ignore_embeddings: bool,
    #[serde(default = "default_batch_size")]
    pub batch_size: String,
    #[serde(default = "default_jobs")]
    pub jobs: usize,
    #[serde(default = "default_upload_operation")]
    pub upload_operation: DocumentOperation,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum DocumentOperation {
    AddOrReplace,
    AddOrUpdate,
}

#[derive(Debug, Serialize)]
pub struct ImportResponse {
    pub status: ImportStatus,
    pub message: String,
    pub batches_processed: Option<usize>,
    pub total_documents: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportStatus {
    Success,
    Error,
}

fn default_csv_delimiter() -> u8 {
    b','
}

fn default_batch_size() -> String {
    "1 MiB".to_string()
}

fn default_jobs() -> usize {
    2
}

fn default_upload_operation() -> DocumentOperation {
    DocumentOperation::AddOrReplace
}

pub struct ImportService {
    client: Client,
}

impl ImportService {
    pub fn new() -> Self {
        let client = Client::new();
        
        Self {
            client,
        }
    }

    pub async fn import_from_url(&self, config: ImportRequest) -> Result<ImportResponse, anyhow::Error> {
        log::info!("Starting import from URL: {}", config.url);
        
        // Download the file
        let response = self.client.get(&config.url).send().await?;
        
        if !response.status().is_success() {
            return Err(anyhow!("Failed to download file: HTTP {}", response.status()));
        }

        let content_type = response.headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .to_string();

        // Get the first few bytes to detect compression
        let mut stream = response.bytes_stream();
        let mut first_chunk = Vec::new();
        let mut buffer = Vec::new();
        let mut batch = Vec::new();
        let mut total_documents = 0;
        let mut batches_processed = 0;
        
        // Read first chunk to detect compression
        if let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            first_chunk = chunk.to_vec();
            buffer.extend_from_slice(&first_chunk);
        }

        // Detect format and compression
        let mime_type = self.detect_format(&config.url, &content_type);
        let compression = self.detect_compression(&config.url, &content_type, &first_chunk);
        
        log::info!("Detected format: {:?}, compression: {:?}", mime_type, compression);

        let batch_size_bytes = Byte::from_str(&config.batch_size)?.as_u64() as usize;

        // If compressed, we need to collect more data before decompressing
        if compression != CompressionType::None {
            // Collect all compressed data first
            let mut compressed_data = first_chunk;
            while let Some(chunk_result) = stream.next().await {
                let chunk = chunk_result?;
                compressed_data.extend_from_slice(&chunk);
            }
            
            // Decompress the data
            let mut decompressed_data = Vec::new();
            let mut reader = self.create_decompressor(compression, compressed_data)?;
            reader.read_to_end(&mut decompressed_data)?;
            
            // Process the decompressed data
            return self.process_decompressed_data(&decompressed_data, mime_type, batch_size_bytes, &config).await;
        }

        // Process uncompressed stream
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            buffer.extend_from_slice(&chunk);
            
            // Process buffer line by line
            let lines: Vec<&[u8]> = buffer.split(|&b| b == b'\n').collect();
            
            if lines.len() <= 1 {
                // Only one line (potentially incomplete), keep it in buffer
                continue;
            }
            
            // Process all complete lines except the last one
            for line in &lines[..lines.len()-1] {
                let line_str = String::from_utf8_lossy(line);
                if line_str.trim().is_empty() {
                    continue;
                }
                
                match mime_type {
                    Mime::Json => {
                        // For JSON, we need to handle the array structure
                        let trimmed = line_str.trim();
                        if trimmed.starts_with('[') || trimmed.ends_with(']') {
                            continue;
                        }
                        if trimmed.ends_with(',') {
                            let json_line = trimmed.trim_end_matches(',');
                            if !json_line.is_empty() {
                                batch.push(json_line.to_string());
                            }
                        } else if !trimmed.is_empty() {
                            batch.push(trimmed.to_string());
                        }
                    }
                    Mime::NdJson => {
                        batch.push(line_str.trim().to_string());
                    }
                    Mime::Csv => {
                        batch.push(line_str.trim().to_string());
                    }
                }
                
                // Check if batch is full
                if batch.len() * 1000 >= batch_size_bytes { // Rough estimate
                    let batch_data = batch.join("\n");
                    self.send_batch(&batch_data, &config, &mime_type).await?;
                    total_documents += batch.len();
                    batches_processed += 1;
                    batch.clear();
                }
            }
            
            // Keep the last (potentially incomplete) line in buffer
            if let Some(last_line) = lines.last() {
                buffer = last_line.to_vec();
            } else {
                buffer.clear();
            }
        }

        // Send remaining batch
        if !batch.is_empty() {
            let batch_data = batch.join("\n");
            self.send_batch(&batch_data, &config, &mime_type).await?;
            total_documents += batch.len();
            batches_processed += 1;
        }

        log::info!("Import completed. Total documents: {}, Batches: {}", total_documents, batches_processed);

        Ok(ImportResponse {
            status: ImportStatus::Success,
            message: "Import completed successfully".to_string(),
            batches_processed: Some(batches_processed),
            total_documents: Some(total_documents),
        })
    }

    async fn process_decompressed_data(&self, data: &[u8], mime_type: Mime, batch_size_bytes: usize, config: &ImportRequest) -> Result<ImportResponse, anyhow::Error> {
        let mut batch = Vec::new();
        let mut total_documents = 0;
        let mut batches_processed = 0;
        
        // Process the decompressed data line by line
        let lines: Vec<&[u8]> = data.split(|&b| b == b'\n').collect();
        
        for line in lines {
            let line_str = String::from_utf8_lossy(line);
            if line_str.trim().is_empty() {
                continue;
            }
            
            match mime_type {
                Mime::Json => {
                    // For JSON, we need to handle the array structure
                    let trimmed = line_str.trim();
                    if trimmed.starts_with('[') || trimmed.ends_with(']') {
                        continue;
                    }
                    if trimmed.ends_with(',') {
                        let json_line = trimmed.trim_end_matches(',');
                        if !json_line.is_empty() {
                            batch.push(json_line.to_string());
                        }
                    } else if !trimmed.is_empty() {
                        batch.push(trimmed.to_string());
                    }
                }
                Mime::NdJson => {
                    batch.push(line_str.trim().to_string());
                }
                Mime::Csv => {
                    batch.push(line_str.trim().to_string());
                }
            }
            
            // Check if batch is full
            if batch.len() * 1000 >= batch_size_bytes { // Rough estimate
                let batch_data = batch.join("\n");
                self.send_batch(&batch_data, config, &mime_type).await?;
                total_documents += batch.len();
                batches_processed += 1;
                batch.clear();
            }
        }

        // Send remaining batch
        if !batch.is_empty() {
            let batch_data = batch.join("\n");
            self.send_batch(&batch_data, config, &mime_type).await?;
            total_documents += batch.len();
            batches_processed += 1;
        }

        log::info!("Import completed. Total documents: {}, Batches: {}", total_documents, batches_processed);

        Ok(ImportResponse {
            status: ImportStatus::Success,
            message: "Import completed successfully".to_string(),
            batches_processed: Some(batches_processed),
            total_documents: Some(total_documents),
        })
    }

    fn detect_format(&self, url: &str, content_type: &str) -> Mime {
        // Try to detect from content-type header first
        if content_type.contains("application/json") {
            return Mime::Json;
        }
        if content_type.contains("text/csv") {
            return Mime::Csv;
        }

        // Fall back to URL extension
        if url.ends_with(".json") {
            Mime::Json
        } else if url.ends_with(".ndjson") || url.ends_with(".jsonl") {
            Mime::NdJson
        } else if url.ends_with(".csv") {
            Mime::Csv
        } else {
            // Default to JSON
            Mime::Json
        }
    }

    fn detect_compression(&self, url: &str, content_type: &str, first_bytes: &[u8]) -> CompressionType {
        // Check content-encoding header first
        if content_type.contains("gzip") || content_type.contains("x-gzip") {
            return CompressionType::Gzip;
        }
        if content_type.contains("bzip2") || content_type.contains("x-bzip2") {
            return CompressionType::Bzip2;
        }
        if content_type.contains("xz") || content_type.contains("x-xz") {
            return CompressionType::Xz;
        }
        if content_type.contains("zstd") || content_type.contains("x-zstd") {
            return CompressionType::Zstd;
        }

        // Check file extension
        if url.ends_with(".gz") || url.ends_with(".gzip") {
            return CompressionType::Gzip;
        }
        if url.ends_with(".bz2") || url.ends_with(".bzip2") {
            return CompressionType::Bzip2;
        }
        if url.ends_with(".xz") || url.ends_with(".lzma") {
            return CompressionType::Xz;
        }
        if url.ends_with(".zst") || url.ends_with(".zstd") {
            return CompressionType::Zstd;
        }

        // Check magic bytes
        if first_bytes.len() >= 2 {
            if first_bytes[0] == 0x1f && first_bytes[1] == 0x8b {
                return CompressionType::Gzip;
            }
            if first_bytes.len() >= 3 && first_bytes[0] == 0x42 && first_bytes[1] == 0x5a && first_bytes[2] == 0x68 {
                return CompressionType::Bzip2;
            }
            if first_bytes.len() >= 6 && first_bytes[0] == 0xfd && first_bytes[1] == 0x37 && first_bytes[2] == 0x7a && first_bytes[3] == 0x58 && first_bytes[4] == 0x5a && first_bytes[5] == 0x00 {
                return CompressionType::Xz;
            }
            if first_bytes.len() >= 4 && first_bytes[0] == 0x28 && first_bytes[1] == 0xb5 && first_bytes[2] == 0x2f && first_bytes[3] == 0xfd {
                return CompressionType::Zstd;
            }
        }

        CompressionType::None
    }

    fn create_decompressor(&self, compression: CompressionType, data: Vec<u8>) -> Result<Box<dyn Read>, anyhow::Error> {
        match compression {
            CompressionType::None => Ok(Box::new(Cursor::new(data))),
            CompressionType::Gzip => {
                let decoder = flate2::read::GzDecoder::new(Cursor::new(data));
                Ok(Box::new(decoder))
            }
            CompressionType::Bzip2 => {
                let decoder = BzDecoder::new(Cursor::new(data));
                Ok(Box::new(decoder))
            }
            CompressionType::Xz => {
                let decoder = XzDecoder::new(Cursor::new(data));
                Ok(Box::new(decoder))
            }
            CompressionType::Zstd => {
                let decoder = Decoder::new(Cursor::new(data))?;
                Ok(Box::new(decoder))
            }
        }
    }

    async fn send_batch(&self, batch_data: &str, config: &ImportRequest, mime_type: &Mime) -> Result<(), anyhow::Error> {
        for attempt in 0..20 {
            let mut gz = GzEncoder::new(Vec::new(), Compression::default());
            gz.write_all(batch_data.as_bytes())?;
            let compressed_data = gz.finish()?;

            let mut request_builder = self.client
                .post(&format!("{}/indexes/{}/documents", config.meilisearch_url, config.index))
                .header("Content-Type", mime_type.as_str())
                .header("Content-Encoding", "gzip");

            if let Some(ref api_key) = config.api_key {
                request_builder = request_builder.header("Authorization", &format!("Bearer {}", api_key));
            }
            
            let response = request_builder
                .body(compressed_data)
                .send()
                .await?;

            let status = response.status();
            
            if status.is_success() {
                log::info!("Batch sent successfully");
                return Ok(());
            } else {
                let error_text = response.text().await.unwrap_or_default();
                log::warn!("Attempt #{}: Failed to send batch: HTTP {} - {}", attempt + 1, status, error_text);
                
                if attempt < 19 { // Don't sleep on the last attempt
                    let duration = Duration::from_secs(2_u64.pow(attempt as u32)); // Exponential backoff
                    log::info!("Retrying in {:?}", duration);
                    tokio::time::sleep(duration).await;
                }
            }
        }

        Err(anyhow!("Failed to send batch after all retries"))
    }
}

#[post("/import")]
async fn start_import(
    _req: HttpRequest,
    payload: web::Json<ImportRequest>,
    service: web::Data<Arc<ImportService>>,
) -> Result<HttpResponse, Error> {
    log::info!("Received import request for URL: {}", payload.url);
    
    match service.import_from_url(payload.into_inner()).await {
        Ok(response) => Ok(HttpResponse::Ok().json(response)),
        Err(e) => {
            log::error!("Import failed: {}", e);
            Ok(HttpResponse::InternalServerError().json(ImportResponse {
                status: ImportStatus::Error,
                message: format!("Import failed: {}", e),
                batches_processed: None,
                total_documents: None,
            }))
        }
    }
}

#[get("/health")]
async fn health_check() -> Result<HttpResponse, Error> {
    let response = serde_json::json!({
        "status": "healthy",
        "service": "meilisearch-importer-server",
        "version": "0.2.4"
    });
    
    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(response.to_string()))
}

pub async fn start_server(port: u16) -> std::io::Result<()> {
    let service = Arc::new(ImportService::new());
    
    log::info!("Starting Meilisearch Importer Server on port {}", port);
    
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(service.clone()))
            .service(start_import)
            .service(health_check)
    })
    .bind(("127.0.0.1", port))?
    .run()
    .await
}