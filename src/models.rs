// src/models.rs
use serde::{Deserialize, Serialize};

// This struct now represents the entire JSON object returned by the API
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ClearinghouseState {
    pub asset_positions: Vec<AssetPosition>,
}

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