// src/main.rs

mod api_handler;
mod models;
mod liquidations;

use axum::{
    extract::{Path, Query, State, WebSocketUpgrade},
    http::{HeaderValue, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use axum::extract::ws::{Message, WebSocket};
use ethers_core::types::Address;
use ethers_core::utils::to_checksum;
use futures::{SinkExt, StreamExt};
use std::{collections::HashMap, env, net::SocketAddr, path::PathBuf, str::FromStr, time::Duration};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tracing_subscriber::{fmt, EnvFilter};

/// ---------- Your existing handlers (unchanged) ----------

async fn fetch_positions(Path(address_str): Path<String>) -> impl IntoResponse {
    let address = match Address::from_str(&address_str) {
        Ok(addr) => addr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid EVM address format"})),
            )
                .into_response();
        }
    };
    let checksummed_address = to_checksum(&address, None);

    match api_handler::get_user_positions(&checksummed_address).await {
        Ok(positions) => (StatusCode::OK, Json(positions)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn fetch_mark_prices() -> impl IntoResponse {
    match api_handler::get_all_mark_prices().await {
        Ok(prices) => (StatusCode::OK, Json(prices)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn fetch_portfolio(Path(address_str): Path<String>) -> impl IntoResponse {
    let address = match Address::from_str(&address_str) {
        Ok(addr) => addr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid EVM address format"})),
            )
                .into_response();
        }
    };
    let checksummed_address = to_checksum(&address, None);

    match api_handler::get_user_portfolio(&checksummed_address).await {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

async fn fetch_calendar(Path(address_str): Path<String>) -> impl IntoResponse {
    let address = match Address::from_str(&address_str) {
        Ok(addr) => addr,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid EVM address format"})),
            )
                .into_response();
        }
    };
    let checksummed_address = to_checksum(&address, None);

    match api_handler::get_calendar_metrics(&checksummed_address).await {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

/// ---------- Liquidations wiring ----------

#[derive(Clone)]
struct AppState {
    liq: liquidations::LiqState,
}

#[derive(serde::Deserialize)]
struct SinceQuery {
    since: Option<String>,
}

async fn get_recent_liqs(
    State(app): State<AppState>,
    Query(q): Query<SinceQuery>,
) -> impl IntoResponse {
    // default to last 24h if not provided
    let since = q
        .since
        .unwrap_or_else(|| (chrono::Utc::now() - chrono::Duration::hours(24)).to_rfc3339());
    let v = app.liq.recent_since(&since).await;
    (StatusCode::OK, Json(v))
}

async fn ws_liqs(State(app): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move { liq_ws_loop(app.liq.clone(), socket).await })
}

async fn liq_ws_loop(state: liquidations::LiqState, socket: WebSocket) {
    let (mut sink, mut _source) = socket.split();
    // Optional: greet client
    let _ = sink
        .send(Message::Text("{\"hello\":\"liquidations\"}".into()))
        .await;

    let mut rx = state.tx.subscribe();
    while let Ok(evt) = rx.recv().await {
        if sink
            .send(Message::Text(serde_json::to_string(&evt).unwrap()))
            .await
            .is_err()
        {
            break;
        }
    }
}

/// Resolve default HL misc events dir without extra deps.
/// If HL_MISC_DIR is not set, use $HOME/hl/data/misc_events/hourly.
fn default_misc_dir() -> PathBuf {
    if let Ok(p) = env::var("HL_MISC_DIR") {
        return PathBuf::from(p);
    }
    let home = env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(format!("{home}/hl/data/misc_events/hourly"))
}

#[tokio::main]
async fn main() {
    // logging (RUST_LOG=info to enable)
    fmt().with_env_filter(EnvFilter::from_default_env()).init();

    // --- CORS ---
    let mut origin_values: Vec<HeaderValue> = vec![
        "http://localhost:5173".parse().unwrap(),
        "https://hl-tracker-frontend.vercel.app".parse().unwrap(),
    ];
    if let Ok(v) = env::var("FRONTEND_ORIGIN") {
        if let Ok(hv) = v.parse() {
            origin_values.push(hv);
        }
    }
    if let Ok(v) = env::var("VERCEL_ORIGIN") {
        if let Ok(hv) = v.parse() {
            origin_values.push(hv);
        }
    }

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origin_values))
        .allow_methods(Any)
        .allow_headers(Any)
        .allow_credentials(false)
        .max_age(Duration::from_secs(86400));

    // --- Liquidation engine config from env ---
    let allow: std::collections::BTreeSet<String> = env::var("LIQ_ALLOW_COINS")
        .unwrap_or_else(|_| "BTC,ETH,SOL".into())
        .split(',')
        .map(|s| s.trim().to_uppercase())
        .filter(|s| !s.is_empty())
        .collect();

    let min_usd: f64 = env::var("LIQ_MIN_USD")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50_000.0);

    let keep_for = Duration::from_secs(24 * 3600);

    let liq_cfg = liquidations::LiqConfig {
        misc_dir: default_misc_dir(),
        allow,
        min_usd,
        keep_for,
        telegram_bot: env::var("TELEGRAM_BOT").ok(),
        telegram_chat: env::var("TELEGRAM_CHAT").ok(),
    };
    let liq_state = liquidations::LiqState::new(liq_cfg);

    // Spawn filesystem ingest task
    {
        let st = liq_state.clone();
        tokio::spawn(async move {
            if let Err(e) = liquidations::run_filesystem_ingest(st).await {
                tracing::error!("ingest error: {e:#}");
            }
        });
    }

    // --- Router ---
    let app_state = AppState { liq: liq_state };
    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/positions/:address", get(fetch_positions))
        .route("/mark-prices", get(fetch_mark_prices))
        .route("/portfolio/:address", get(fetch_portfolio))
        .route("/calendar/:address", get(fetch_calendar))
        // Liquidations bootstrap + live
        .route("/liquidations", get(get_recent_liqs))
        .route("/ws/liquidations", get(ws_liqs))
        .with_state(app_state)
        .layer(cors);

    // --- Bind address/port ---
    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("🚀 Server listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
