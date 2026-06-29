//! CRAFT Web Dashboard - Main entry point
//!
//! Run: `cargo run -p craft-web -- serve`
//!
//! Environment variables:
//!   CRAFT_HOME      - Path to CRAFT home directory (default: ~/.craft)
//!   CRAFT_WEB_PORT  - Server port (default: 3000)
//!   CRAFT_WEB_HOST  - Server host (default: 127.0.0.1)
//!   CRAFT_WEB_STATIC - Path to static files directory (optional)

use std::env;
use std::net::{IpAddr, SocketAddr};
use tracing::{error, info};

#[tokio::main]
async fn main() {
    // Initialize tracing with environment filter
    tracing_subscriber::fmt()
        .with_env_filter(
            env::var("RUST_LOG").unwrap_or_else(|_| "craft_web=info,tower_http=debug".to_string()),
        )
        .init();

    info!("CRAFT Web Dashboard starting...");

    // Parse arguments
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 {
        match args[1].as_str() {
            "serve" => {
                if let Err(e) = serve_command().await {
                    error!("Server error: {}", e);
                    std::process::exit(1);
                }
            }
            "--version" | "-v" | "version" => {
                println!("craft-web {}", env!("CARGO_PKG_VERSION"));
            }
            "--help" | "-h" | "help" => {
                print_help();
            }
            arg => {
                error!("Unknown command: {}", arg);
                print_help();
                std::process::exit(1);
            }
        }
    } else {
        // Default: run server
        if let Err(e) = serve_command().await {
            error!("Server error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn serve_command() -> Result<(), Box<dyn std::error::Error>> {
    // Get configuration from environment
    let host = env::var("CRAFT_WEB_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("CRAFT_WEB_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3000u16);
    let static_dir = env::var("CRAFT_WEB_STATIC").ok();

    let host: IpAddr = host
        .parse()
        .map_err(|e| format!("Invalid CRAFT_WEB_HOST value: {e}"))?;
    let addr = SocketAddr::from((host, port));

    // Create CraftHome from environment
    let home = craft_core::CraftHome::from_env()
        .map_err(|e| format!("Failed to initialize CRAFT_HOME: {}", e))?;

    info!("CRAFT_HOME: {:?}", home.root());

    // Run the server
    craft_web::run_server(addr, home, static_dir.as_deref())
        .await
        .map_err(|e| format!("Server error: {}", e))?;

    Ok(())
}

fn print_help() {
    println!("CRAFT Web Dashboard {}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("Commands:");
    println!("  serve           Start the web server (default)");
    println!("  version         Show version information");
    println!("  help            Show this help message");
    println!();
    println!("Environment Variables:");
    println!("  CRAFT_HOME        Path to CRAFT home directory");
    println!("  CRAFT_WEB_PORT    Server port (default: 3000)");
    println!("  CRAFT_WEB_HOST    Server host (default: 127.0.0.1)");
    println!("  CRAFT_WEB_STATIC  Path to static files directory");
    println!();
    println!("API Endpoints:");
    println!("  GET  /api/v1/harnesses              List all installed harnesses");
    println!("  GET  /api/v1/harnesses/{{name}}        Get harness details");
    println!("  GET  /api/v1/harnesses/{{name}}/versions    List harness versions");
    println!("  POST /api/v1/compose/plan           Preview composition");
    println!("  POST /api/v1/compose               Compose harnesses");
    println!("  GET  /api/v1/memory/search?q=...   Search memory facts");
    println!("  GET  /api/v1/memory/facts           List memory facts");
    println!("  POST /api/v1/memory/facts          Create memory fact");
    println!("  GET  /api/v1/memory/scope/{{scope}}    Get facts by scope");
    println!("  GET  /api/v1/status                Runtime status");
    println!("  WS   /ws/validate                  WebSocket validation");
}
