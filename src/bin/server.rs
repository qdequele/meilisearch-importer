use clap::Parser;
use log::info;

#[derive(Parser)]
#[command(name = "meilisearch-importer-server")]
#[command(about = "HTTP server for importing data into Meilisearch")]
struct Opt {
    /// Port to bind the server to
    #[arg(short, long, default_value_t = 8080)]
    port: u16,
    
    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::parse();
    
    // Initialize logging
    if opt.debug {
        std::env::set_var("RUST_LOG", "debug");
    } else {
        std::env::set_var("RUST_LOG", "info");
    }
    env_logger::init();
    
    info!("Starting Meilisearch Importer Server on port {}", opt.port);
    info!("Server will accept import requests with full configuration in the request body");
    
    // Start the server
    meilisearch_importer::server::start_server(opt.port).await?;
    
    Ok(())
}