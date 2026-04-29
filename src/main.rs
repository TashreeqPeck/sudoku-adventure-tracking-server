//! Sudoku Adventure tracker — Axum + SQLite + sheet CSV sync.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::routing::{get, post, put};
use axum::Router;
use tokio::sync::RwLock;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

mod csv_util;
mod db;
mod error;
mod handlers;
mod logic;
mod models;
mod sync;

#[derive(Clone, Default)]
pub struct Meta {
    pub last_sync_at: Option<String>,
    pub last_sync_error: Option<String>,
}

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::SqlitePool,
    pub client: reqwest::Client,
    pub export_url: String,
    pub sheet_sync_interval_ms: u64,
    pub cache: Arc<RwLock<Vec<models::Puzzle>>>,
    pub meta: Arc<RwLock<Meta>>,
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

async fn tick_sync(state: &AppState) {
    match sync::sync_from_sheet(&state.client, &state.export_url).await {
        Ok(puzzles) => {
            *state.cache.write().await = puzzles;
            let mut m = state.meta.write().await;
            m.last_sync_error = None;
            m.last_sync_at = Some(chrono::Utc::now().to_rfc3339());
        }
        Err(e) => {
            state.meta.write().await.last_sync_error = Some(e.to_string());
            tracing::error!(error = %e, "sheet sync failed");
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let data_dir = PathBuf::from(env_or("DATA_DIR", "./data"));
    let sheet_id = env_or(
        "SHEET_ID",
        "1y4BYBEuXbzReb_tx3bTUdKwynL2JveL3ob55g6c-D-Y",
    );
    let sheet_gid = env_or("SHEET_GID", "0");
    let export_url = format!(
        "https://docs.google.com/spreadsheets/d/{}/export?format=csv&gid={}",
        sheet_id, sheet_gid
    );
    let port: u16 = env_or("PORT", "3840").parse().unwrap_or(3840);
    let sync_ms: u64 = std::env::var("SHEET_SYNC_INTERVAL_MS")
        .or_else(|_| std::env::var("SYNC_INTERVAL_MS"))
        .unwrap_or_else(|_| "86400000".to_string())
        .parse()
        .unwrap_or(86_400_000)
        .max(60_000);

    let pool = db::init_pool(&data_dir).await?;
    let client = reqwest::Client::builder()
        .user_agent("sa-tracker/1.0")
        .build()?;

    let cache = Arc::new(RwLock::new(Vec::new()));
    let meta = Arc::new(RwLock::new(Meta::default()));

    let state = AppState {
        pool,
        client: client.clone(),
        export_url: export_url.clone(),
        sheet_sync_interval_ms: sync_ms,
        cache: cache.clone(),
        meta: meta.clone(),
    };

    match sync::sync_from_sheet(&client, &export_url).await {
        Ok(puzzles) => {
            *cache.write().await = puzzles;
            let mut m = meta.write().await;
            m.last_sync_error = None;
            m.last_sync_at = Some(chrono::Utc::now().to_rfc3339());
        }
        Err(e) => {
            meta.write().await.last_sync_error = Some(e.to_string());
            tracing::error!(error = %e, "initial sheet sync failed");
        }
    }

    let bg = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(sync_ms));
        loop {
            interval.tick().await;
            tick_sync(&bg).await;
        }
    });

    let static_dir = PathBuf::from(env_or("STATIC_DIR", "public"));

    let api = Router::new()
        .route("/health", get(handlers::api_health))
        .route("/state", get(handlers::api_state))
        .route("/progress/{number}", put(handlers::api_progress_put))
        .route("/refresh", post(handlers::api_refresh))
        .route("/import-from-url", post(handlers::api_import))
        .with_state(state);

    let app = Router::new()
        .nest("/api", api)
        .fallback_service(ServeDir::new(static_dir))
        .layer(TraceLayer::new_for_http());

    let addr = format!("0.0.0.0:{port}");
    tracing::info!(%addr, sync_hours = %((sync_ms as f64) / 3_600_000.0), "Sudoku Adventure tracker (Rust)");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
