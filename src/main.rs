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
use ethers_core::types::Address;
use ethers_core::utils::to_checksum;
use std::{env, net::SocketAddr, str::FromStr, time::Duration};
use tower_http::cors::{Any, CorsLayer};

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
    //
    // You can keep these two ENV vars optional:
    //   FRONTEND_ORIGIN=http://localhost:5173
    //   VERCEL_ORIGIN=https://your-vercel-domain.vercel.app
    //
    // If not set, we include sensible defaults.
    let mut origins: Vec<String> = vec![
        "http://localhost:5173".to_string(), // local dev
    ];
    if let Ok(v) = env::var("FRONTEND_ORIGIN") {
        origins.push(v);
    }
    if let Ok(v) = env::var("VERCEL_ORIGIN") {
        origins.push(v);
    }

    let allow_list = origins
        .into_iter()
        .filter_map(|o| o.parse().ok())
        .collect::<Vec<_>>();

    let cors = CorsLayer::new()
        .allow_origin(allow_list) // explicit allow-list (recommended)
        .allow_methods(Any)
        .allow_headers(Any)
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
    // Cloud Run supplies PORT; default to 3000 for local dev.
    let port: u16 = env::var("PORT").ok().and_then(|s| s.parse().ok()).unwrap_or(8080);


    // Bind to 0.0.0.0 for container / Cloud Run; fine locally too.
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("🚀 Server listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}



// gcloud run deploy hl-tracker-backend   --region=europe-west1   --source .   --allow-unauthenticated