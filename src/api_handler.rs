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
    use std::collections::HashMap;

    // --- outputs we build ---
    let mut daily_pnl: HashMap<String, f64> = HashMap::new();

    // Unweighted duration samples (ms) – preserves your prior behavior
    let mut hold_all: Vec<f64> = Vec::new();
    let mut hold_profit: Vec<f64> = Vec::new();
    let mut hold_loss: Vec<f64> = Vec::new();

    // Weighted (by notional USD) duration accumulators for long/short
    // We'll accumulate (sum(duration * weight), sum(weight)) to compute a weighted average.
    let mut w_sum_long: f64 = 0.0;
    let mut w_den_long: f64 = 0.0;
    let mut w_sum_short: f64 = 0.0;
    let mut w_den_short: f64 = 0.0;

    // Trade counters / win counters
    let mut long_trades: u64 = 0;
    let mut short_trades: u64 = 0;
    let mut long_wins: u64 = 0;
    let mut short_wins: u64 = 0;

    // Sort fills in time order (copy to avoid mutating caller's slice)
    let mut fills_sorted = fills.to_vec();
    fills_sorted.sort_by_key(|f| f.time);

    // Track open positions FIFO by (coin, side) -> Vec<(open_time_ms, remaining_size)>
    // side = "Long" or "Short"
    let mut open_positions: HashMap<(String, String), Vec<(u64, f64)>> = HashMap::new();

    // Helper: parse f64 safely
    let parse_f64 = |s: &str| -> f64 { s.parse::<f64>().unwrap_or(0.0) };

    for f in fills_sorted {
        // day key is UTC epoch day (matching your frontend)
        let day_key = (f.time / 86_400_000).to_string();

        // Closed PnL for the day tally
        let pnl = f.closed_pnl.as_deref().unwrap_or("0").parse::<f64>().unwrap_or(0.0);

        // Determine side from "dir"
        let is_long = f.dir.contains("Long");
        let side_str = if is_long { "Long" } else { "Short" }.to_string();

        if f.dir.starts_with("Open") {
            // Record an open leg
            let size = parse_f64(&f.sz);
            if size > 0.0 {
                open_positions
                    .entry((f.coin.clone(), side_str.clone()))
                    .or_default()
                    .push((f.time, size));
            }
        } else if f.dir.starts_with("Close") {
            // We are closing size; match against oldest opens (FIFO)
            let mut remaining = parse_f64(&f.sz);
            let close_px = parse_f64(&f.px); // USD price for notional weighting

            // Match against the appropriate queue
            let positions = open_positions
                .entry((f.coin.clone(), side_str.clone()))
                .or_default();

            while remaining > 0.0 && !positions.is_empty() {
                let (open_time, open_size) = positions[0];
                let matched = open_size.min(remaining);

                // Duration for this matched chunk (milliseconds)
                let duration_ms = f.time.saturating_sub(open_time) as f64;

                // Record in unweighted vectors
                hold_all.push(duration_ms);
                if pnl >= 0.0 {
                    hold_profit.push(duration_ms);
                } else {
                    hold_loss.push(duration_ms);
                }

                // Notional weight = matched_size * close_px (USD)
                let weight = matched * close_px;
                if is_long {
                    w_sum_long += duration_ms * weight;
                    w_den_long += weight;
                } else {
                    w_sum_short += duration_ms * weight;
                    w_den_short += weight;
                }

                // Reduce or pop the open chunk
                if open_size > matched {
                    positions[0].1 = open_size - matched;
                } else {
                    positions.remove(0);
                }
                remaining -= matched;
            }

            // Tally daily PnL
            *daily_pnl.entry(day_key).or_insert(0.0) += pnl;

            // Count trades & wins (per close fill)
            if is_long {
                long_trades += 1;
                if pnl > 0.0 {
                    long_wins += 1;
                }
            } else {
                short_trades += 1;
                if pnl > 0.0 {
                    short_wins += 1;
                }
            }
        }
    }

    // Average helpers
    let avg_unweighted = |v: &Vec<f64>| -> f64 {
        if v.is_empty() {
            0.0
        } else {
            v.iter().sum::<f64>() / v.len() as f64
        }
    };
    let avg_weighted = |sum_wx: f64, sum_w: f64| -> f64 {
        if sum_w > 0.0 { sum_wx / sum_w } else { 0.0 }
    };

    let avg_hold_time = avg_unweighted(&hold_all);
    let avg_hold_time_profit = avg_unweighted(&hold_profit);
    let avg_hold_time_loss = avg_unweighted(&hold_loss);

    // NEW: notional-weighted averages for long/short
    let avg_hold_time_long = avg_weighted(w_sum_long, w_den_long);
    let avg_hold_time_short = avg_weighted(w_sum_short, w_den_short);

    let long_short_ratio = if short_trades == 0 {
        long_trades as f64
    } else {
        long_trades as f64 / short_trades as f64
    };

    let win_rate_long = if long_trades == 0 {
        0.0
    } else {
        long_wins as f64 / long_trades as f64
    };
    let win_rate_short = if short_trades == 0 {
        0.0
    } else {
        short_wins as f64 / short_trades as f64
    };

    CalendarMetrics {
        daily_pnl,
        avg_hold_time,
        avg_hold_time_profit,
        avg_hold_time_loss,
        avg_hold_time_long,   // new
        avg_hold_time_short,  // new
        long_short_ratio,
        win_rate_long,
        win_rate_short,
    }
}
pub async fn get_calendar_metrics(user_address: &str) -> Result<CalendarMetrics, Box<dyn Error>> {
    let fills = get_user_fills(user_address).await?;
    Ok(compute_calendar_metrics(&fills))
}