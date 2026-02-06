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
        let max_blocks = config::Config::get().network.max_blocks;
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

    fn get_processor_stats(&self, proc: Processor, levels: &[u32]) -> ProcessorStats {
        let window = self.data.get(&proc).unwrap();
        let mut tips: Vec<u64> = window.blocks().iter().flat_map(|(_, t)| t.iter().map(|ti| ti.lamports)).collect();
        tips.sort_unstable();
        let (slot_start, slot_end) = window.slot_range();

        let percentiles = levels.iter().map(|&level| {
            LevelTip { level, tip: percentile(&tips, level) }
        }).collect();

        ProcessorStats {
            processor: proc,
            slot_start,
            slot_end,
            tips: tips.len(),
            percentiles,
        }
    }

    fn get_update(&self, processors: &[Processor], levels: &[u32]) -> WsUpdate {
        let procs = if processors.is_empty() { Processor::all().to_vec() } else { processors.to_vec() };
        let data = procs.iter().map(|p| self.get_processor_stats(*p, levels)).collect();
        WsUpdate { data }
    }

    fn get_window(&self, processors: &[Processor]) -> Vec<ProcessorWindow> {
        let procs = if processors.is_empty() { Processor::all().to_vec() } else { processors.to_vec() };

        procs.iter().map(|proc| {
            let window = self.data.get(proc).unwrap();
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
    let levels = params.levels.unwrap_or_else(|| vec![5000]);
    let processors = params.processors.unwrap_or_default();

    let tracker = state.tracker.read().unwrap();
    let update = tracker.get_update(&processors, &levels);
    let result = update.data.into_iter().map(|s| ProcessorTips {
        processor: s.processor,
        tips: s.percentiles.into_iter().map(|p| LevelTip { level: p.level, tip: p.tip }).collect(),
    }).collect();

    Json(RpcResponse { jsonrpc: req.jsonrpc, id: req.id, result })
}

pub(crate) async fn handle_window(State(state): State<AppState>, Json(req): Json<WindowRequest>) -> Json<WindowResponse> {
    let params = req.params.and_then(|p| p.into_iter().next()).unwrap_or_default();
    let processors = params.processors.unwrap_or_default();

    let tracker = state.tracker.read().unwrap();
    let result = tracker.get_window(&processors);

    Json(WindowResponse { jsonrpc: req.jsonrpc, id: req.id, result })
}

pub(crate) async fn handle_ws(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_client(socket, state.tracker, state.tx.subscribe()))
}

async fn handle_ws_client(mut socket: WebSocket, tracker: SharedTracker, mut rx: broadcast::Receiver<u64>) {
    let mut levels: Vec<u32> = vec![5000];
    let mut processors: Vec<Processor> = vec![];

    loop {
        tokio::select! {
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(sub) = serde_json::from_str::<WsSubscribe>(&text) {
                            if let Some(l) = sub.levels {
                                levels = l;
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
                    let t = tracker.read().unwrap();
                    t.get_update(&processors, &levels)
                };
                let msg = serde_json::to_string(&update).unwrap();
                if socket.send(Message::Text(msg)).await.is_err() { break; }
            }
        }
    }
}
