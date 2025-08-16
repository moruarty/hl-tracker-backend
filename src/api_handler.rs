// src/api_handler.rs

use crate::models;
use crate::models::ClearinghouseState;
use reqwest::Client;
use serde_json::json;
use std::error::Error;

const HYPERLIQUID_API_URL: &str = "https://api.hyperliquid.xyz/info";

pub async fn get_user_positions(user_address: &str) -> Result<Vec<models::AssetPosition>, Box<dyn Error>> {
    let client = Client::builder()
        .build()?;

    let request_body = json!({
        "type": "clearinghouseState",
        "user": user_address
    });

    let response = client
        .post(HYPERLIQUID_API_URL)
        .json(&request_body)
        .send()
        .await?;

    // Get the status before the response is moved
    let status = response.status();
    if !status.is_success() {
        // Now we can safely consume the response to get the error body
        let error_body = response.text().await.unwrap_or_else(|_| "Could not retrieve error body".to_string());
        // Use the 'status' variable we saved earlier
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