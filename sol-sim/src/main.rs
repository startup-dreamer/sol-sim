use anyhow::Result;
use axum::{
    routing::{delete, get, post},
    Router,
    http::{HeaderMap, Request, Response, StatusCode},
};
use clap::Parser;
use sol_sim::{api, fork::ForkManager, storage::Storage};
use std::sync::Arc;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "sol-sim")]
#[command(about = "Solana Fork Simulation Engine - MVP")]
struct Args {
    /// Port to listen on
    #[arg(long, default_value = "8080")]
    port: u16,

    /// Redis URL
    #[arg(long, default_value = "redis://localhost:6379")]
    redis_url: String,

    /// Solana RPC URL (mainnet/testnet/devnet)
    #[arg(long, default_value = "https://api.testnet.solana.com")]
    solana_rpc: String,

    /// Base URL for fork RPC endpoints
    #[arg(long, default_value = "http://localhost:8080")]
    base_url: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sol_sim=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    info!("Starting Solana Fork Simulation Engine");
    info!("Port: {}", args.port);
    info!("Redis: {}", args.redis_url);
    info!("Solana RPC: {}", args.solana_rpc);

    // Initialize storage
    let storage = Storage::new(&args.redis_url)?;

    // Test Redis connection
    let client = redis::Client::open(args.redis_url.clone())?;
    let mut conn = client.get_multiplexed_async_connection().await?;
    redis::cmd("PING").query_async::<String>(&mut conn).await?;
    info!("Redis connection established");

    // Initialize fork manager
    let manager = Arc::new(ForkManager::new(storage, args.base_url, args.solana_rpc));

    // Build router
    let app = Router::new()
        .route("/health", get(api::health))
        // Fork management endpoints
        .route("/rpc/{fork_id}", post(api::handle_rpc))
        .route("/forks", post(api::create_fork))
        .route("/forks/{fork_id}", get(api::get_fork))
        .route("/forks/{fork_id}", delete(api::delete_fork))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(manager);

    // Start server
    let addr = format!("0.0.0.0:{}", args.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!("Server listening on {}", addr);
    info!("API documentation:");
    info!("  POST   /forks              - Create new fork");
    info!("  GET    /forks/:id          - Get fork info");
    info!("  DELETE /forks/:id          - Delete fork");
    info!("  POST   /rpc/:id            - Send JSON-RPC request");

    axum::serve(listener, app).await?;

    Ok(())
}
