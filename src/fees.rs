use std::sync::{Arc, RwLock};

use axum::{
    extract::{State, WebSocketUpgrade, ws::{Message, WebSocket}},
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use tokio::sync::broadcast;

use crate::config;
use crate::window::{RollingWindow, percentile};
use crate::{AppState, RpcRequest, WindowRequest, WsSubscribe};

// Helper function to handle RwLock poisoning gracefully
fn read_lock<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|e| {
        eprintln!("RwLock poisoned (read), recovering");
        e.into_inner()
    })
}

#[derive(Clone)]
struct FeeInfo {
    signature: String,
    priority_fee: u64,
}

pub struct FeeTracker {
    data: RollingWindow<FeeInfo>,
}

impl FeeTracker {
    pub fn new() -> Self {
        // Store max_blocks + 1 since we exclude the most recent (incomplete) block
        Self { data: RollingWindow::new(config::Config::get().network.max_fee_blocks + 1) }
    }

    pub fn add_fee(&mut self, slot: u64, signature: String, priority_fee: u64) {
        self.data.add(slot, FeeInfo { signature, priority_fee });
    }

    pub fn should_broadcast(&mut self, slot: u64) -> bool {
        self.data.should_broadcast(slot)
    }

    fn get_update(&self, percentiles: &[u32]) -> FeeWsUpdate {
        let (slot_start, slot_end) = self.data.slot_range();
        let count = self.data.blocks().iter().map(|(_, f)| f.len()).sum();

        let percentiles = percentiles.iter().map(|&pct| {
            // Calculate percentile per block using: index = min(pct, 9999) × N ÷ 10000
            // IMPORTANT: Exclude the most recent slot (still forming) - only use complete blocks
            let blocks: Vec<_> = self.data.blocks().iter().collect();
            let complete_blocks = if blocks.len() > 1 {
                &blocks[..blocks.len() - 1] // Exclude last block (still forming)
            } else {
                &blocks[..]
            };

            let per_block_percentiles: Vec<u64> = complete_blocks.iter()
                .filter_map(|(_, fees)| {
                    if fees.is_empty() {
                        return None;
                    }

                    let mut block_fees: Vec<u64> = fees.iter().map(|fi| fi.priority_fee).collect();
                    block_fees.sort_unstable();

                    Some(percentile(&block_fees, pct))
                })
                .collect();

            if per_block_percentiles.is_empty() {
                return LevelFee { level: pct, fee: 0 };
            }

            // Average the per-block percentiles
            let fee = per_block_percentiles.iter().sum::<u64>() / per_block_percentiles.len() as u64;

            LevelFee { level: pct, fee }
        }).collect();

        FeeWsUpdate { slot_start, slot_end, count, percentiles }
    }

    fn get_window(&self) -> Vec<SlotFees> {
        self.data.blocks().iter().map(|(slot, fees)| {
            let fees = fees.iter().map(|fi| FeeEntry {
                signature: fi.signature.clone(),
                priority_fee: fi.priority_fee,
            }).collect();
            SlotFees { slot: *slot, fees }
        }).collect()
    }
}

pub type SharedFeeTracker = Arc<RwLock<FeeTracker>>;

// --- Response Types ---

#[derive(Clone, Serialize)]
struct FeeWsUpdate {
    slot_start: u64,
    slot_end: u64,
    count: usize,
    percentiles: Vec<LevelFee>,
}

#[derive(Serialize)]
pub(crate) struct FeeRpcResponse {
    jsonrpc: String,
    id: serde_json::Value,
    result: Vec<LevelFee>,
}

#[derive(Clone, Serialize)]
struct LevelFee {
    level: u32,
    fee: u64,
}

#[derive(Serialize)]
struct FeeEntry {
    signature: String,
    priority_fee: u64,
}

#[derive(Serialize)]
struct SlotFees {
    slot: u64,
    fees: Vec<FeeEntry>,
}

#[derive(Serialize)]
pub(crate) struct FeeWindowResponse {
    jsonrpc: String,
    id: serde_json::Value,
    result: Vec<SlotFees>,
}

// --- Handlers ---

pub(crate) async fn handle_fee_rpc(State(state): State<AppState>, Json(req): Json<RpcRequest>) -> Json<FeeRpcResponse> {
    let params = req.params.and_then(|p| p.into_iter().next()).unwrap_or_default();
    // Percentiles in range 0-10_000 (e.g., 5000 = 50.00%, 9000 = 90.00%)
    let percentiles = params.levels.unwrap_or_else(|| vec![5000]);

    let fee_tracker = read_lock(&state.fee_tracker);
    let update = fee_tracker.get_update(&percentiles);

    Json(FeeRpcResponse { jsonrpc: req.jsonrpc, id: req.id, result: update.percentiles })
}

pub(crate) async fn handle_fee_window(State(state): State<AppState>, Json(req): Json<WindowRequest>) -> Json<FeeWindowResponse> {
    let fee_tracker = read_lock(&state.fee_tracker);
    let result = fee_tracker.get_window();

    Json(FeeWindowResponse { jsonrpc: req.jsonrpc, id: req.id, result })
}

pub(crate) async fn handle_fee_ws(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_fee_ws_client(socket, state.fee_tracker, state.fee_tx.subscribe()))
}

async fn handle_fee_ws_client(mut socket: WebSocket, fee_tracker: SharedFeeTracker, mut rx: broadcast::Receiver<u64>) {
    // Percentiles in range 0-10_000 (e.g., 5000 = 50.00%, 9000 = 90.00%)
    let mut percentiles: Vec<u32> = vec![5000];

    loop {
        tokio::select! {
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(sub) = serde_json::from_str::<WsSubscribe>(&text) {
                            if let Some(l) = sub.levels { percentiles = l; }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            result = rx.recv() => {
                if result.is_err() { break; }
                let update = {
                    let t = read_lock(&fee_tracker);
                    t.get_update(&percentiles)
                };
                match serde_json::to_string(&update) {
                    Ok(msg) => {
                        if socket.send(Message::Text(msg)).await.is_err() { break; }
                    }
                    Err(e) => {
                        eprintln!("Failed to serialize fee update: {}", e);
                    }
                }
            }
        }
    }
}
