// src/api_handler.rs

use crate::models::{self, AssetCtx, CalendarMetrics, ClearinghouseState, Universe, UserFill};
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

pub async fn get_user_fills(user_address: &str) -> Result<Vec<UserFill>, Box<dyn Error>> {
    let client = Client::new();
    let request_body = json!({
        "type": "userFills",
        "user": user_address,
        "aggregateByTime": true
    });

    let response = client
        .post(HYPERLIQUID_API_URL)
        .json(&request_body)
        .send()
        .await?;

    if !response.status().is_success() {
        let error_body = response.text().await.unwrap_or_else(|_| "Could not retrieve error body".to_string());
        let err_msg = format!("API request for fills failed. Body: {}", error_body);
        return Err(err_msg.into());
    }

    let fills: Vec<UserFill> = response.json().await?;
    Ok(fills)
}

fn compute_calendar_metrics(fills: &[UserFill]) -> CalendarMetrics {
    let mut daily_pnl: HashMap<String, f64> = HashMap::new();
    let mut hold_all: Vec<f64> = Vec::new();
    let mut hold_profit: Vec<f64> = Vec::new();
    let mut hold_loss: Vec<f64> = Vec::new();
    let mut long_trades = 0u64;
    let mut short_trades = 0u64;
    let mut long_wins = 0u64;
    let mut short_wins = 0u64;

    let mut fills_sorted = fills.to_vec();
    fills_sorted.sort_by_key(|f| f.time);

    let mut open_positions: HashMap<(String, String), Vec<(u64, f64)>> = HashMap::new();

    for f in fills_sorted {
        let pnl = f.closed_pnl.as_deref().unwrap_or("0").parse::<f64>().unwrap_or(0.0);
        let day_key = (f.time / 86_400_000).to_string();

        if f.dir.starts_with("Open") {
            let side = if f.dir.contains("Long") { "Long" } else { "Short" };
            let size = f.sz.parse::<f64>().unwrap_or(0.0);
            open_positions.entry((f.coin.clone(), side.to_string())).or_default().push((f.time, size));
        } else if f.dir.starts_with("Close") {
            let side = if f.dir.contains("Long") { "Long" } else { "Short" };
            let mut remaining = f.sz.parse::<f64>().unwrap_or(0.0);
            let positions = open_positions.entry((f.coin.clone(), side.to_string())).or_default();
            while remaining > 0.0 && !positions.is_empty() {
                let (open_time, open_size) = positions[0];
                let duration = f.time.saturating_sub(open_time) as f64;
                hold_all.push(duration);
                if pnl >= 0.0 { hold_profit.push(duration); } else { hold_loss.push(duration); }
                if open_size > remaining {
                    positions[0].1 = open_size - remaining;
                    remaining = 0.0;
                } else {
                    remaining -= open_size;
                    positions.remove(0);
                }
            }

            daily_pnl.entry(day_key).and_modify(|v| *v += pnl).or_insert(pnl);

            if side == "Long" {
                long_trades += 1;
                if pnl > 0.0 { long_wins += 1; }
            } else {
                short_trades += 1;
                if pnl > 0.0 { short_wins += 1; }
            }
        }
    }

    let avg = |v: &Vec<f64>| if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 };

    CalendarMetrics {
        daily_pnl,
        avg_hold_time: avg(&hold_all),
        avg_hold_time_profit: avg(&hold_profit),
        avg_hold_time_loss: avg(&hold_loss),
        long_short_ratio: if short_trades == 0 { long_trades as f64 } else { long_trades as f64 / short_trades as f64 },
        win_rate_long: if long_trades == 0 { 0.0 } else { long_wins as f64 / long_trades as f64 },
        win_rate_short: if short_trades == 0 { 0.0 } else { short_wins as f64 / short_trades as f64 },
    }
}

pub async fn get_calendar_metrics(user_address: &str) -> Result<CalendarMetrics, Box<dyn Error>> {
    let fills = get_user_fills(user_address).await?;
    Ok(compute_calendar_metrics(&fills))
}