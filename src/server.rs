use std::io::Write;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use actix_web::{get, post, web, App, Error, HttpRequest, HttpResponse, HttpServer, Result};
use anyhow::anyhow;
use byte_unit::Byte;
use exponential_backoff::Backoff;
use flate2::write::GzEncoder;
use flate2::Compression;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::mime::Mime;

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
        let status = response.status();
        
        if !status.is_success() {
            return Err(anyhow!("Failed to download file: HTTP {}", status));
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|ct| ct.to_str().ok())
            .unwrap_or("");

        let mime_type = self.detect_format(&config.url, content_type);
        log::info!("Detected format: {:?}", mime_type);

        let batch_size_bytes = Byte::from_str(&config.batch_size)
            .map_err(|e| anyhow!("Invalid batch size: {}", e))?
            .as_u64() as usize;

        let mut batch = Vec::new();
        let mut total_documents = 0;
        let mut batches_processed = 0;

        let mut stream = response.bytes_stream();
        let mut buffer = Vec::new();
        
        // Process the stream in chunks
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

    async fn send_batch(&self, batch_data: &str, config: &ImportRequest, mime_type: &Mime) -> Result<(), anyhow::Error> {
        let retry_policy = Backoff::new(20, Duration::from_millis(100), Some(Duration::from_secs(60 * 60)));
        
        for (attempt, duration) in retry_policy.into_iter().enumerate() {
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
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy",
        "service": "meilisearch-importer-server",
        "version": env!("CARGO_PKG_VERSION")
    })))
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