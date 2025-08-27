// src/main.rs

mod api_handler;
mod models;

use axum::{
    extract::Path,
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use ethers_core::types::Address;
use ethers_core::utils::to_checksum;
use std::{env, net::SocketAddr, str::FromStr, time::Duration};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

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

#[tokio::main]
async fn main() {
    // --- CORS ---
    // Explicit allow-list. Important: NO trailing slash on origins.
    // You can also extend this list from env if you like.
    let mut origin_values: Vec<HeaderValue> = vec![
        "http://localhost:5173".parse().unwrap(),
        "https://hl-tracker-frontend.vercel.app".parse().unwrap(),
    ];
    if let Ok(v) = env::var("FRONTEND_ORIGIN") {
        if let Ok(hv) = v.parse() { origin_values.push(hv); }
    }
    if let Ok(v) = env::var("VERCEL_ORIGIN") {
        if let Ok(hv) = v.parse() { origin_values.push(hv); }
    }

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origin_values))
        .allow_methods(Any)   // all methods (you currently use only GET)
        .allow_headers(Any)   // allow custom headers if you add any later
        // If you ever send cookies/auth from the frontend, flip this to true:
        .allow_credentials(false)
        .max_age(Duration::from_secs(86400));

    // --- Router ---
    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/positions/:address", get(fetch_positions))
        .route("/mark-prices", get(fetch_mark_prices))
        .route("/portfolio/:address", get(fetch_portfolio))
        .route("/calendar/:address", get(fetch_calendar))
        .layer(cors);

    // --- Bind address/port ---
    let port: u16 = env::var("PORT").ok().and_then(|s| s.parse().ok()).unwrap_or(8080);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("🚀 Server listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
