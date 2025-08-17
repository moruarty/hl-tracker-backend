// src/main.rs

mod api_handler;
mod models;

use axum::{
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use std::net::SocketAddr;
use std::str::FromStr;
use ethers_core::types::Address;
use ethers_core::utils::to_checksum;
use tower_http::cors::CorsLayer;

async fn fetch_positions(Path(address_str): Path<String>) -> impl IntoResponse {
    let address = match Address::from_str(&address_str) {
        Ok(addr) => addr,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "Invalid EVM address format"}))).into_response();
        }
    };
    let checksummed_address = to_checksum(&address, None);

    match api_handler::get_user_positions(&checksummed_address).await {
        Ok(positions) => (StatusCode::OK, Json(positions)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn fetch_mark_prices() -> impl IntoResponse {
    match api_handler::get_all_mark_prices().await {
        Ok(prices) => (StatusCode::OK, Json(prices)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/positions/:address", get(fetch_positions))
        .route("/mark-prices", get(fetch_mark_prices))
        .layer(CorsLayer::permissive());

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("🚀 Server listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}