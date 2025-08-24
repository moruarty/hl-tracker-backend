// src/api_handler.rs

use crate::models::{self, AssetCtx, ClearinghouseState, Universe};
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::error::Error;
use crate::models::TimeframeData;

const HYPERLIQUID_API_URL: &str = "https://api.hyperliquid.xyz/info";

pub async fn get_user_positions(user_address: &str) -> Result<Vec<models::AssetPosition>, Box<dyn Error>> {
    let client = Client::new();

    let request_body = json!({
        "type": "clearinghouseState",
        "user": user_address
    });

    let response = client
        .post(HYPERLIQUID_API_URL)
        .json(&request_body)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let error_body = response.text().await.unwrap_or_else(|_| "Could not retrieve error body".to_string());
        let err_msg = format!("API request failed with status: {}. Body: {}", status, error_body);
        return Err(err_msg.into());
    }

    let state: ClearinghouseState = response.json().await?;
    
    let active_positions = state
        .asset_positions
        .into_iter()
        .filter(|ap| ap.position.szi.parse::<f64>().unwrap_or(0.0) != 0.0)
        .collect();

    Ok(active_positions)
}

pub async fn get_all_mark_prices() -> Result<HashMap<String, String>, Box<dyn Error>> {
    let client = Client::new();
    let request_body = json!({ "type": "metaAndAssetCtxs" });

    let response = client
        .post(HYPERLIQUID_API_URL)
        .json(&request_body)
        .send()
        .await?;

    if !response.status().is_success() {
        let error_body = response.text().await.unwrap_or_else(|_| "Could not retrieve error body".to_string());
        let err_msg = format!("API request for mark prices failed. Body: {}", error_body);
        return Err(err_msg.into());
    }

    let (meta, ctxs): (Universe, Vec<AssetCtx>) = response.json().await?;

    let mark_prices = meta
        .universe
        .into_iter()
        .zip(ctxs.into_iter())
        .map(|(meta, ctx)| (meta.name, ctx.mark_px))
        .collect::<HashMap<String, String>>();

    Ok(mark_prices)
}

pub async fn get_user_portfolio(
    user_address: &str,
) -> Result<HashMap<String, TimeframeData>, Box<dyn Error>> {
    let client = Client::new();
    let request_body = json!({
        "type": "portfolio",
        "user": user_address
    });

    let response = client
        .post(HYPERLIQUID_API_URL)
        .json(&request_body)
        .send()
        .await?;

    if !response.status().is_success() {
        let error_body = response.text().await.unwrap_or_else(|_| "Could not retrieve error body".to_string());
        let err_msg = format!("API request for portfolio failed. Body: {}", error_body);
        return Err(err_msg.into());
    }

    // New two-step parsing logic
    // 1. Parse the response into a generic vector of [String, Value] tuples.
    let generic_data: Vec<(String, Value)> = response.json().await?;

    // 2. Manually iterate and convert each Value into our specific TimeframeData struct.
    let mut portfolio_data = HashMap::new();
    for (key, value) in generic_data {
        let timeframe_data: TimeframeData = serde_json::from_value(value)?;
        portfolio_data.insert(key, timeframe_data);
    }

    Ok(portfolio_data)
}