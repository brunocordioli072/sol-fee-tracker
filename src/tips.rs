use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, RwLock};

use axum::{
    extract::{State, WebSocketUpgrade, ws::{Message, WebSocket}},
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use solana_sdk::pubkey::Pubkey;
use tokio::sync::broadcast;

use crate::config;
use crate::processor::Processor;
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
struct TipInfo {
    signature: String,
    lamports: u64,
}

pub struct TipTracker {
    data: HashMap<Processor, RollingWindow<TipInfo>>,
    account_to_processor: HashMap<Pubkey, Processor>,
}

impl TipTracker {
    pub fn new() -> Self {
        let max_blocks = config::Config::get().network.max_tip_blocks;
        let mut data = HashMap::new();
        let mut account_to_processor = HashMap::new();

        for proc in Processor::all() {
            data.insert(*proc, RollingWindow::new(max_blocks));
            for acc in proc.accounts() {
                if let Ok(pk) = Pubkey::from_str(acc) {
                    account_to_processor.insert(pk, *proc);
                }
            }
        }

        Self { data, account_to_processor }
    }

    pub fn get_processor(&self, pk: &Pubkey) -> Option<Processor> {
        self.account_to_processor.get(pk).copied()
    }

    pub fn add_tip(&mut self, processor: Processor, slot: u64, signature: String, lamports: u64) {
        if let Some(window) = self.data.get_mut(&processor) {
            window.add(slot, TipInfo { signature, lamports });
        }
    }

    pub fn should_broadcast(&mut self, slot: u64) -> bool {
        for window in self.data.values_mut() {
            if window.should_broadcast(slot) {
                return true;
            }
        }
        false
    }

    fn get_processor_stats(&self, proc: Processor, percentiles: &[u32]) -> ProcessorStats {
        let window = self.data.get(&proc).expect("Processor should exist in data map");
        let (slot_start, slot_end) = window.slot_range();
        let total_tips = window.blocks().iter().map(|(_, t)| t.len()).sum();

        let percentiles = percentiles.iter().map(|&pct| {
            // Calculate percentile per block using: index = min(pct, 9999) × N ÷ 10000
            // IMPORTANT: Exclude the most recent slot (still forming) - only use complete blocks
            let blocks: Vec<_> = window.blocks().iter().collect();
            let complete_blocks = if blocks.len() > 1 {
                &blocks[..blocks.len() - 1] // Exclude last block (still forming)
            } else {
                &blocks[..]
            };

            let per_block_percentiles: Vec<u64> = complete_blocks.iter()
                .filter_map(|(_, tips)| {
                    if tips.is_empty() {
                        return None;
                    }

                    let mut block_tips: Vec<u64> = tips.iter().map(|ti| ti.lamports).collect();
                    block_tips.sort_unstable();

                    Some(percentile(&block_tips, pct))
                })
                .collect();

            if per_block_percentiles.is_empty() {
                return LevelTip { level: pct, tip: 0 };
            }

            // Average the per-block percentiles
            let tip = per_block_percentiles.iter().sum::<u64>() / per_block_percentiles.len() as u64;

            LevelTip { level: pct, tip }
        }).collect();

        ProcessorStats {
            processor: proc,
            slot_start,
            slot_end,
            tips: total_tips,
            percentiles,
        }
    }

    fn get_update(&self, processors: &[Processor], percentiles: &[u32]) -> WsUpdate {
        let procs = if processors.is_empty() { Processor::all().to_vec() } else { processors.to_vec() };
        let data = procs.iter().map(|p| self.get_processor_stats(*p, percentiles)).collect();
        WsUpdate { data }
    }

    fn get_window(&self, processors: &[Processor]) -> Vec<ProcessorWindow> {
        let procs = if processors.is_empty() { Processor::all().to_vec() } else { processors.to_vec() };

        procs.iter().map(|proc| {
            let window = self.data.get(proc).expect("Processor should exist in data map");
            let blocks = window.blocks().iter().map(|(slot, tips)| {
                let tips = tips.iter().map(|ti| TipEntry {
                    signature: ti.signature.clone(),
                    tip: ti.lamports,
                }).collect();
                SlotTips { slot: *slot, tips }
            }).collect();

            ProcessorWindow { processor: *proc, blocks }
        }).collect()
    }
}

pub type SharedTracker = Arc<RwLock<TipTracker>>;

// --- Response Types ---

#[derive(Clone, Serialize)]
struct ProcessorStats {
    processor: Processor,
    slot_start: u64,
    slot_end: u64,
    tips: usize,
    percentiles: Vec<LevelTip>,
}

#[derive(Clone, Serialize)]
struct WsUpdate {
    data: Vec<ProcessorStats>,
}

#[derive(Serialize)]
pub(crate) struct RpcResponse {
    jsonrpc: String,
    id: serde_json::Value,
    result: Vec<ProcessorTips>,
}

#[derive(Serialize)]
struct ProcessorTips {
    processor: Processor,
    tips: Vec<LevelTip>,
}

#[derive(Clone, Serialize)]
struct LevelTip {
    level: u32,
    tip: u64,
}

#[derive(Serialize)]
struct TipEntry {
    signature: String,
    tip: u64,
}

#[derive(Serialize)]
struct SlotTips {
    slot: u64,
    tips: Vec<TipEntry>,
}

#[derive(Serialize)]
struct ProcessorWindow {
    processor: Processor,
    blocks: Vec<SlotTips>,
}

#[derive(Serialize)]
pub(crate) struct WindowResponse {
    jsonrpc: String,
    id: serde_json::Value,
    result: Vec<ProcessorWindow>,
}

// --- Handlers ---

pub(crate) fn parse_transfer(data: &[u8]) -> Option<u64> {
    if data.len() < 12 { return None; }
    let ix_type = u32::from_le_bytes(data[0..4].try_into().ok()?);
    if ix_type == 2 || ix_type == 11 {
        Some(u64::from_le_bytes(data[4..12].try_into().ok()?))
    } else {
        None
    }
}

pub(crate) async fn handle_rpc(State(state): State<AppState>, Json(req): Json<RpcRequest>) -> Json<RpcResponse> {
    let params = req.params.and_then(|p| p.into_iter().next()).unwrap_or_default();
    // Percentiles in range 0-10_000 (e.g., 5000 = 50.00%, 9000 = 90.00%)
    let percentiles = params.levels.unwrap_or_else(|| vec![5000]);
    let processors = params.processors.unwrap_or_default();

    let tracker = read_lock(&state.tracker);
    let update = tracker.get_update(&processors, &percentiles);
    let result = update.data.into_iter().map(|s| ProcessorTips {
        processor: s.processor,
        tips: s.percentiles,
    }).collect();

    Json(RpcResponse { jsonrpc: req.jsonrpc, id: req.id, result })
}

pub(crate) async fn handle_window(State(state): State<AppState>, Json(req): Json<WindowRequest>) -> Json<WindowResponse> {
    let params = req.params.and_then(|p| p.into_iter().next()).unwrap_or_default();
    let processors = params.processors.unwrap_or_default();

    let tracker = read_lock(&state.tracker);
    let result = tracker.get_window(&processors);

    Json(WindowResponse { jsonrpc: req.jsonrpc, id: req.id, result })
}

pub(crate) async fn handle_ws(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_client(socket, state.tracker, state.tx.subscribe()))
}

async fn handle_ws_client(mut socket: WebSocket, tracker: SharedTracker, mut rx: broadcast::Receiver<u64>) {
    // Percentiles in range 0-10_000 (e.g., 5000 = 50.00%, 9000 = 90.00%)
    let mut percentiles: Vec<u32> = vec![5000];
    let mut processors: Vec<Processor> = vec![];

    loop {
        tokio::select! {
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(sub) = serde_json::from_str::<WsSubscribe>(&text) {
                            if let Some(l) = sub.levels {
                                percentiles = l;
                            }
                            if let Some(p) = sub.processors {
                                processors = p;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            result = rx.recv() => {
                if result.is_err() { break; }
                let update = {
                    let t = read_lock(&tracker);
                    t.get_update(&processors, &percentiles)
                };
                match serde_json::to_string(&update) {
                    Ok(msg) => {
                        if socket.send(Message::Text(msg)).await.is_err() { break; }
                    }
                    Err(e) => {
                        eprintln!("Failed to serialize tip update: {}", e);
                    }
                }
            }
        }
    }
}
