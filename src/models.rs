// src/models.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AssetMeta { pub name: String }

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AssetCtx { pub mark_px: String }

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Universe { pub universe: Vec<AssetMeta> }

// THIS IS THE CRITICAL FIX: The #[serde(rename_all...)] line was missing here.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TimeframeData {
    pub pnl_history: Vec<(u64, String)>,
    pub account_value_history: Vec<(u64, String)>,
    pub vlm: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CumFunding {
    pub all_time: String,
    pub since_open: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ClearinghouseState { pub asset_positions: Vec<AssetPosition> }

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AssetPosition {
    pub position: Position,
    #[serde(rename = "type")]
    pub position_type: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub coin: String,
    pub cum_funding: CumFunding,
    pub entry_px: Option<String>,
    pub leverage: Leverage,
    pub liquidation_px: Option<String>,
    pub margin_used: String,
    pub position_value: String,
    pub return_on_equity: String,
    pub szi: String,
    pub unrealized_pnl: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Leverage {
    pub value: u32,
    #[serde(rename = "type")]
    pub leverage_type: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UserFill {
    #[serde(default)]
    pub closed_pnl: Option<String>,
    pub coin: String,
    pub dir: String,
    pub px: String,
    pub side: String,
    pub start_position: Option<String>,
    pub sz: String,
    pub time: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CalendarMetrics {
    pub daily_pnl: HashMap<String, f64>,
    pub avg_hold_time: f64,
    pub avg_hold_time_profit: f64,
    pub avg_hold_time_loss: f64,
    pub long_short_ratio: f64,
    pub win_rate_long: f64,
    pub win_rate_short: f64,
}