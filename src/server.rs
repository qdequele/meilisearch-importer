use std::io::prelude::*;
use std::sync::Arc;
use std::time::Duration;

use actix_web::{web, App, Error, HttpResponse, HttpServer, Responder};
use anyhow::Context;
use byte_unit::Byte;
use exponential_backoff::Backoff;
use flate2::write::GzEncoder;
use flate2::Compression;
use futures::StreamExt;
use rayon::{ThreadPool, ThreadPoolBuilder};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::mime::Mime;

#[derive(Debug, Deserialize, Clone)]
pub struct ImportConfig {
    /// The URL of your Meilisearch instance
    pub meilisearch_url: String,
    /// The index name you want to send your documents to
    pub index: String,
    /// The name of the field that must be used by Meilisearch to uniquely identify your documents
    pub primary_key: Option<String>,
    /// The API key to access Meilisearch
    pub api_key: Option<String>,
    /// The delimiter to use for CSV files
    #[serde(default = "default_csv_delimiter")]
    pub csv_delimiter: u8,
    /// Whether to ignore embeddings
    #[serde(default)]
    pub ignore_embeddings: bool,
    /// The size of the batches sent to Meilisearch
    #[serde(default = "default_batch_size")]
    pub batch_size: String,
    /// The number of parallel jobs to use when uploading data
    #[serde(default = "default_jobs")]
    pub jobs: usize,
    /// The operation to perform when uploading a document
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
    pub success: bool,
    pub message: String,
    pub total_batches: u64,
    pub total_documents: u64,
}

#[derive(Debug, Serialize)]
pub struct ImportStatus {
    pub job_id: String,
    pub status: String,
    pub progress: f64,
    pub total_batches: u64,
    pub completed_batches: u64,
}

fn default_csv_delimiter() -> u8 {
    b','
}

fn default_batch_size() -> String {
    "20 MiB".to_string()
}

fn default_jobs() -> usize {
    1
}

fn default_upload_operation() -> DocumentOperation {
    DocumentOperation::AddOrReplace
}

impl ImportConfig {
    fn batch_size_bytes(&self) -> anyhow::Result<Byte> {
        self.batch_size.parse().context("Invalid batch size format")
    }
}

pub struct ImportService {
    config: ImportConfig,
    client: Client,
    thread_pool: ThreadPool,
}

impl ImportService {
    pub fn new(config: ImportConfig) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
        
        let thread_pool = ThreadPoolBuilder::new()
            .num_threads(config.jobs)
            .build()?;

        Ok(Self {
            config,
            client,
            thread_pool,
        })
    }

    pub async fn import_from_url(&self, url: String) -> anyhow::Result<ImportResponse> {
        // Download the file from the URL
        let response = self.client.get(&url).send().await?;
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        // Determine the format based on content type or URL extension
        let mime = self.detect_format(&url, content_type)?;
        
        // Stream the response body
        let mut stream = response.bytes_stream();
        let mut buffer = Vec::new();
        let mut total_batches = 0u64;
        let mut total_documents = 0u64;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            buffer.extend_from_slice(&chunk);

            // Process in batches
            if buffer.len() >= self.config.batch_size_bytes()?.as_u64() as usize {
                let batch_data = buffer.drain(..).collect::<Vec<_>>();
                self.send_batch(&mime, &batch_data).await?;
                total_batches += 1;
                total_documents += self.count_documents_in_batch(&mime, &batch_data)?;
            }
        }

        // Send remaining data
        if !buffer.is_empty() {
            self.send_batch(&mime, &buffer).await?;
            total_batches += 1;
            total_documents += self.count_documents_in_batch(&mime, &buffer)?;
        }

        Ok(ImportResponse {
            success: true,
            message: "Import completed successfully".to_string(),
            total_batches,
            total_documents,
        })
    }

    fn detect_format(&self, url: &str, content_type: &str) -> anyhow::Result<Mime> {
        // Try to detect from content type first
        if content_type.contains("json") {
            if content_type.contains("ndjson") || content_type.contains("jsonl") {
                return Ok(Mime::NdJson);
            }
            return Ok(Mime::Json);
        } else if content_type.contains("csv") {
            return Ok(Mime::Csv);
        }

        // Fall back to URL extension
        if let Some(mime) = Mime::from_path(std::path::Path::new(url)) {
            return Ok(mime);
        }

        anyhow::bail!("Could not determine file format from URL or content type")
    }

    async fn send_batch(&self, mime: &Mime, data: &[u8]) -> anyhow::Result<()> {
        let api_key = self.config.api_key.clone();
        let mut url = format!("{}/indexes/{}/documents", self.config.meilisearch_url, self.config.index);
        
        if let Some(primary_key) = &self.config.primary_key {
            url = format!("{}?primaryKey={}", url, primary_key);
        }

        // Compress the data
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data)?;
        let compressed_data = encoder.finish()?;

        let retries = 20;
        let min = Duration::from_millis(100);
        let max = Duration::from_secs(60 * 60);
        let backoff = Backoff::new(retries, min, max);

        for (attempt, duration) in backoff.into_iter().enumerate() {
            let mut request = self.client.request(
                match self.config.upload_operation {
                    DocumentOperation::AddOrReplace => reqwest::Method::POST,
                    DocumentOperation::AddOrUpdate => reqwest::Method::PUT,
                },
                &url,
            );

            request = request
                .header("Content-Type", mime.as_str())
                .header("Content-Encoding", "gzip")
                .header("X-Meilisearch-Client", "Meilisearch Importer Server");

            if let Some(api_key) = &api_key {
                request = request.header("Authorization", &format!("Bearer {}", api_key));
            }

            match request.body(compressed_data.clone()).send().await {
                Ok(response) if response.status().is_success() => return Ok(()),
                Ok(response) => {
                    let status = response.status();
                    let error_text = response.text().await.unwrap_or_default();
                    log::warn!("Attempt #{}: HTTP {} - {}", attempt + 1, status, error_text);
                    tokio::time::sleep(duration).await;
                }
                Err(e) => {
                    log::warn!("Attempt #{}: {}", attempt + 1, e);
                    tokio::time::sleep(duration).await;
                }
            }
        }

        anyhow::bail!("Too many errors. Stopping the retries.")
    }

    fn count_documents_in_batch(&self, mime: &Mime, data: &[u8]) -> anyhow::Result<u64> {
        match mime {
            Mime::Json => {
                // For JSON, count the number of objects in the array
                let text = String::from_utf8_lossy(data);
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(array) = json.as_array() {
                        return Ok(array.len() as u64);
                    }
                }
                Ok(1) // Single JSON object
            }
            Mime::NdJson => {
                // For NDJSON, count the number of lines
                let text = String::from_utf8_lossy(data);
                Ok(text.lines().filter(|line| !line.trim().is_empty()).count() as u64)
            }
            Mime::Csv => {
                // For CSV, count the number of lines minus header
                let text = String::from_utf8_lossy(data);
                let lines: Vec<&str> = text.lines().collect();
                if lines.len() > 1 {
                    Ok((lines.len() - 1) as u64)
                } else {
                    Ok(0)
                }
            }
        }
    }
}

pub async fn start_import(
    config: web::Json<ImportConfig>,
    url: web::Json<serde_json::Value>,
) -> Result<impl Responder, Error> {
    let url = url["url"]
        .as_str()
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Missing 'url' field"))?;

    let import_service = ImportService::new(config.into_inner())
        .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;

    let result = import_service.import_from_url(url.to_string()).await;
    
    match result {
        Ok(response) => Ok(HttpResponse::Ok().json(response)),
        Err(e) => Ok(HttpResponse::InternalServerError().json(ImportResponse {
            success: false,
            message: format!("Import failed: {}", e),
            total_batches: 0,
            total_documents: 0,
        })),
    }
}

pub async fn health_check() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy",
        "service": "meilisearch-importer-server"
    }))
}

pub async fn start_server(config: ImportConfig, port: u16) -> anyhow::Result<()> {
    let config = Arc::new(config);
    
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(config.clone()))
            .route("/health", web::get().to(health_check))
            .route("/import", web::post().to(start_import))
    })
    .bind(("127.0.0.1", port))?
    .run()
    .await?;

    Ok(())
}