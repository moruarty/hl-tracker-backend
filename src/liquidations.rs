use std::{
    collections::{BTreeSet, VecDeque},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{
    fs::File,
    io::{AsyncBufReadExt, BufReader},
    sync::{broadcast, RwLock},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiqPos {
    pub coin: String,
    pub szi: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiquidationOut {
    pub ts: String, // ISO8601 from file
    pub tx_hash: String,
    pub account_value: f64,
    pub notional: f64,
    pub leverage_type: String,
    pub positions: Vec<LiqPos>,
    pub coins: Vec<String>,
}

#[derive(Clone)]
pub struct LiqConfig {
    pub misc_dir: PathBuf,       // e.g. ~/hl/data/misc_events/hourly
    pub allow: BTreeSet<String>, // {BTC,ETH,SOL}
    pub min_usd: f64,            // 25_000 or 50_000
    pub keep_for: Duration,      // 24h
    pub telegram_bot: Option<String>,
    pub telegram_chat: Option<String>,
}

#[derive(Clone)]
pub struct LiqState {
    pub tx: broadcast::Sender<LiquidationOut>,
    pub buf: std::sync::Arc<RwLock<VecDeque<LiquidationOut>>>,
    pub cfg: LiqConfig,
}

impl LiqState {
    pub fn new(cfg: LiqConfig) -> Self {
        let (tx, _rx) = broadcast::channel(1024);
        Self {
            tx,
            buf: std::sync::Arc::new(RwLock::new(VecDeque::new())),
            cfg,
        }
    }

    pub async fn push_and_broadcast(&self, liq: LiquidationOut) {
        // retain in memory for 24h (time-based prune)
        {
            let mut q = self.buf.write().await;
            q.push_front(liq.clone());
            self.prune_locked(&mut q).await;
        }
        let _ = self.tx.send(liq.clone());
        // telegram
        if let (Some(bot), Some(chat)) = (&self.cfg.telegram_bot, &self.cfg.telegram_chat) {
            let _ = send_telegram(bot, chat, &liq).await;
        }
    }

    async fn prune_locked(&self, q: &mut VecDeque<LiquidationOut>) {
        // drop anything older than keep_for comparing parsed ts
        let cutoff = Utc::now() - chrono::Duration::from_std(self.cfg.keep_for).unwrap();
        while let Some(back) = q.back() {
            if DateTime::parse_from_rfc3339(&back.ts)
                .map(|t| t.with_timezone(&Utc) < cutoff)
                .unwrap_or(false)
            {
                q.pop_back();
            } else {
                break;
            }
        }
    }

    pub async fn recent_since(&self, since_iso: &str) -> Vec<LiquidationOut> {
        let Ok(since) = DateTime::parse_from_rfc3339(since_iso) else {
            return vec![];
        };
        let since = since.with_timezone(&Utc);
        let q = self.buf.read().await;
        q.iter()
            .filter(|e| {
                DateTime::parse_from_rfc3339(&e.ts)
                    .map(|t| t.with_timezone(&Utc) >= since)
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    }
}

async fn send_telegram(bot: &str, chat: &str, liq: &LiquidationOut) -> Result<()> {
    let url = format!("https://api.telegram.org/bot{}/sendMessage", bot);
    let text = format!(
        "*HL Liquidation* (>= ${})\nTime: {}\nAccount Value: ${}\nNotional: ${}\nCoins: {}\nTx: `{}`",
        format_amount(liq.account_value),
        liq.ts,
        format_amount(liq.account_value),
        format_amount(liq.notional),
        liq.coins.join(", "),
        liq.tx_hash,
    );
    let body =
        serde_json::json!({ "chat_id": chat, "text": text, "parse_mode": "Markdown" });
    reqwest::Client::new()
        .post(url)
        .json(&body)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

fn format_amount(v: f64) -> String {
    (v.round() as i64).abs().to_string()
}

/// Robustly extract a liquidation from the JSON line.
/// We tolerate small schema differences by walking the JSON.
fn extract_liquidation(line: &str) -> Option<LiquidationOut> {
    let v: Value = serde_json::from_str(line).ok()?;
    // Required fields at the top level
    let ts = v.get("time")?.as_str()?.to_string();
    let tx = v
        .get("hash")
        .and_then(|h| h.as_str())
        .unwrap_or_default()
        .to_string();

    // Find inner.delta.Liquidation
    let inner = v.get("inner")?;
    // Sometimes inner is a LedgerUpdate directly, sometimes tagged. Try both.
    let delta = inner
        .get("delta")
        .or_else(|| inner.get("LedgerUpdate")?.get("delta"))?;
    let liq = delta.get("Liquidation")?;

    let account_value = liq.get("accountValue")?.as_f64()?;
    let notional = liq
        .get("liquidatedNtlPos")
        .and_then(|x| x.as_f64())
        .unwrap_or(0.0);
    let leverage = liq
        .get("leverageType")
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string();

    // positions array: [{ coin, szi }]
    let mut positions = Vec::new();
    if let Some(arr) = liq.get("liquidatedPositions").and_then(|x| x.as_array()) {
        for p in arr {
            let coin = p
                .get("coin")
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_string();
            let szi = p.get("szi").and_then(|x| x.as_f64()).unwrap_or(0.0);
            positions.push(LiqPos { coin, szi });
        }
    }
    let mut coins: BTreeSet<String> = positions
        .iter()
        .map(|p| p.coin.to_uppercase())
        .collect();
    if coins.is_empty() {
        // fallback: try coin at another path (rare)
        if let Some(c) = liq.get("coin").and_then(|x| x.as_str()) {
            coins.insert(c.to_uppercase());
        }
    }
    Some(LiquidationOut {
        ts,
        tx_hash: tx,
        account_value,
        notional,
        leverage_type: leverage,
        positions,
        coins: coins.into_iter().collect(),
    })
}

/// Tail a single file forever, yielding new lines as they are appended.
async fn tail_file(path: &Path, state: LiqState) -> Result<()> {
    // NOTE: this reads from the beginning of the file. If you want to start from EOF, you can
    // pre-read all existing lines before entering the main loop.
    let file = File::open(path).await?;
    let mut reader = BufReader::new(file);
    let mut buf = String::new();
    loop {
        buf.clear();
        let n = reader.read_line(&mut buf).await?;
        if n == 0 {
            // file hasn't grown; sleep briefly
            tokio::time::sleep(Duration::from_millis(200)).await;
            continue;
        }
        if let Some(liq) = extract_liquidation(&buf) {
            // filter
            if (liq.account_value >= state.cfg.min_usd)
                && liq
                    .coins
                    .iter()
                    .any(|c| state.cfg.allow.contains(c.as_str()))
            {
                state.push_and_broadcast(liq).await;
            }
        }
    }
}

/// Choose the latest file inside the latest date/hour directory.
fn pick_latest_file(base: &Path) -> Option<PathBuf> {
    let mut dates = std::fs::read_dir(base)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect::<Vec<_>>();
    dates.sort();
    let date = dates.pop()?;
    let mut hours = std::fs::read_dir(&date)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect::<Vec<_>>();
    hours.sort();
    let hour = hours.pop()?;
    let mut files = std::fs::read_dir(&hour)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect::<Vec<_>>();
    files.sort();
    files.pop()
}

/// Watch the directory tree; always (re)tail the newest file.
pub async fn run_filesystem_ingest(state: LiqState) -> Result<()> {
    let base = state.cfg.misc_dir.clone();
    tracing::info!("Watching {:?}", base);

    // start by tailing the current latest file (if any)
    if let Some(latest) = pick_latest_file(&base) {
        tracing::info!("Initial tail: {:?}", latest);
        let st = state.clone();
        tokio::spawn(async move {
            if let Err(e) = tail_file(&latest, st).await {
                tracing::error!("tail_file error: {e:#}");
            }
        });
    }

    // Bridge notify callback -> tokio mpsc
    let (tx, mut rx) =
        tokio::sync::mpsc::unbounded_channel::<Result<notify::Event, notify::Error>>();

    // recommended_watcher uses a blocking callback; forward into the tokio channel.
    let mut watcher: RecommendedWatcher =
        notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            let _ = tx.send(res);
        })?;
    watcher.configure(Config::default())?;
    watcher.watch(&base, RecursiveMode::Recursive)?;

    // On any filesystem event, retarget to the latest file.
    while let Some(res) = rx.recv().await {
        match res {
            Ok(_evt) => {
                if let Some(latest) = pick_latest_file(&base) {
                    tracing::info!("Retarget tail: {:?}", latest);
                    let st = state.clone();
                    tokio::spawn(async move {
                        if let Err(e) = tail_file(&latest, st).await {
                            tracing::error!("tail_file error: {e:#}");
                        }
                    });
                }
            }
            Err(e) => {
                tracing::warn!("notify error: {e:#}");
            }
        }
    }

    Ok(())
}
