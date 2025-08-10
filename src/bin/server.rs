use std::env;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use log::info;
use serde_json;

use meilisearch_importer::server::{ImportConfig, start_server};

#[derive(Parser)]
#[command(name = "meilisearch-importer-server")]
#[command(about = "HTTP server for importing data to Meilisearch")]
struct Opt {
    /// Path to the configuration file
    #[arg(short, long, default_value = "config.json")]
    config: PathBuf,

    /// Port to bind the server to
    #[arg(short, long, default_value_t = 8080)]
    port: u16,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();

    // Initialize logging
    if opt.debug {
        env::set_var("RUST_LOG", "debug");
    } else {
        env::set_var("RUST_LOG", "info");
    }
    env_logger::init();

    info!("Starting Meilisearch Importer Server...");

    // Read configuration file
    let config_content = std::fs::read_to_string(&opt.config)
        .with_context(|| format!("Failed to read config file: {:?}", opt.config))?;
    
    let config: ImportConfig = serde_json::from_str(&config_content)
        .with_context(|| "Failed to parse config file")?;

    info!("Configuration loaded successfully");
    info!("Meilisearch URL: {}", config.meilisearch_url);
    info!("Index: {}", config.index);
    info!("Batch size: {}", config.batch_size);
    info!("Jobs: {}", config.jobs);

    // Start the server
    info!("Starting server on port {}", opt.port);
    start_server(config, opt.port).await?;

    Ok(())
}