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

/// Hard-coded winsorization cap for the pooled-tip percentile computation.
/// Anything above this pooled percentile is clamped to the cap value, so
/// outlier tip transactions can't poison percentiles below the cap.
const WINSORIZE_CAP_PCT: u32 = 9800;

/// Tips below this value are treated as spam/dust and excluded from the
/// pooled sample. Without this, pooled p50 on any builder gets dominated by
/// the long tail of sub-thousand-lamport dust tips that are never
/// competitive for inclusion.
const DUST_FLOOR_LAMPORTS: u64 = 10_000;

/// Below this many post-dust-filter samples, the pool is too sparse for a
/// statistically meaningful percentile and `get_pooled_processor_stats`
/// returns zero for every requested level. Callers should treat zero as
/// "insufficient data" and fall back to a configured minimum tip.
const MIN_POOL_SIZE: usize = 20;

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
            // Store max_blocks + 1 since we exclude the most recent (incomplete) block
            data.insert(*proc, RollingWindow::new(max_blocks + 1));
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

    fn get_pooled_processor_stats(&self, proc: Processor, percentiles: &[u32]) -> ProcessorStats {
        let window = self.data.get(&proc).expect("Processor should exist in data map");
        let (slot_start, slot_end) = window.slot_range();
        let total_tips = window.blocks().iter().map(|(_, t)| t.len()).sum();

        // Same "complete blocks" rule as get_processor_stats: exclude the most
        // recent (still-forming) block, except when there's only one block total.
        let blocks: Vec<_> = window.blocks().iter().collect();
        let complete_blocks = if blocks.len() > 1 {
            &blocks[..blocks.len() - 1]
        } else {
            &blocks[..]
        };

        let mut pool: Vec<u64> = complete_blocks.iter()
            .flat_map(|(_, tips)| tips.iter().map(|ti| ti.lamports))
            .filter(|&lamports| lamports >= DUST_FLOOR_LAMPORTS)
            .collect();
        pool.sort_unstable();

        let percentiles = pooled_tip_percentiles(&pool, percentiles, WINSORIZE_CAP_PCT);

        ProcessorStats {
            processor: proc,
            slot_start,
            slot_end,
            tips: total_tips,
            percentiles,
        }
    }

    fn get_pooled_update(&self, processors: &[Processor], percentiles: &[u32]) -> WsUpdate {
        let procs = if processors.is_empty() { Processor::all().to_vec() } else { processors.to_vec() };
        let data = procs.iter().map(|p| self.get_pooled_processor_stats(*p, percentiles)).collect();
        WsUpdate { data }
    }

    /// Pool tips across the *union* of the requested processors, apply the
    /// same dust filter + winsorization as the per-processor pooled path,
    /// and return a single set of percentiles. Designed for the "what's a
    /// competitive fast-path tip right now" use case where the caller
    /// routes transactions to several non-Jito builders and pays a single
    /// tip value to whichever lands.
    ///
    /// `slot_start` / `slot_end` report the union of the per-processor
    /// windows (min start, max end). `count` is total tips observed across
    /// included processors (pre-dust-filter) for continuity with the
    /// per-processor pooled endpoint's `tips` field.
    fn get_pooled_aggregate_update(&self, processors: &[Processor], percentiles: &[u32]) -> AggregateUpdate {
        let procs = if processors.is_empty() { Processor::all().to_vec() } else { processors.to_vec() };

        let mut slot_start = u64::MAX;
        let mut slot_end = 0u64;
        let mut pool: Vec<u64> = Vec::new();
        let mut total_count = 0usize;

        for proc in &procs {
            let window = self.data.get(proc).expect("Processor should exist in data map");
            let (s_start, s_end) = window.slot_range();
            if s_start > 0 {
                slot_start = slot_start.min(s_start);
            }
            slot_end = slot_end.max(s_end);
            total_count += window.blocks().iter().map(|(_, t)| t.len()).sum::<usize>();

            let blocks: Vec<_> = window.blocks().iter().collect();
            let complete_blocks = if blocks.len() > 1 {
                &blocks[..blocks.len() - 1]
            } else {
                &blocks[..]
            };

            for (_, tips) in complete_blocks {
                for tip_info in tips {
                    if tip_info.lamports >= DUST_FLOOR_LAMPORTS {
                        pool.push(tip_info.lamports);
                    }
                }
            }
        }

        if slot_start == u64::MAX {
            slot_start = 0;
        }

        pool.sort_unstable();
        let percentiles = pooled_tip_percentiles(&pool, percentiles, WINSORIZE_CAP_PCT);

        AggregateUpdate {
            slot_start,
            slot_end,
            count: total_count,
            processors: procs,
            percentiles,
        }
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

#[derive(Clone, Serialize)]
pub(crate) struct AggregateUpdate {
    slot_start: u64,
    slot_end: u64,
    count: usize,
    processors: Vec<Processor>,
    percentiles: Vec<LevelTip>,
}

#[derive(Serialize)]
pub(crate) struct AggregateRpcResponse {
    jsonrpc: String,
    id: serde_json::Value,
    result: AggregateUpdate,
}

/// Pure helper: given a pre-sorted pool of tip lamports (already
/// dust-filtered by the caller) and a list of requested percentile levels
/// (0-10_000 scale), returns `LevelTip` entries where each tip is the
/// pooled percentile value clamped to the winsorization cap.
///
/// The cap is the smaller of:
/// - the percentile value at `cap_pct` (the classic winsorization), and
/// - the value just below the top `max(5, n/50)` samples (rank-based trim).
///
/// The rank-based component matters on sparse pools (a few hundred samples
/// or fewer). With only 164 tips, `cap_pct = 9800` would land at the
/// 4th-largest value — a single whale tip would still dominate the cap.
/// The rank trim pins the cap at the 6th-from-top (or deeper) regardless
/// of pool size, so a handful of outliers can never set the bar.
///
/// Pools below `MIN_POOL_SIZE` return zero for every level; callers should
/// treat that as "insufficient data" and fall back to a minimum tip.
fn pooled_tip_percentiles(sorted_pool: &[u64], levels: &[u32], cap_pct: u32) -> Vec<LevelTip> {
    let n = sorted_pool.len();
    if n < MIN_POOL_SIZE {
        return levels.iter().map(|&pct| LevelTip { level: pct, tip: 0 }).collect();
    }

    let trim_count = std::cmp::max(5, n / 50);
    let trim_cap = sorted_pool[n - trim_count - 1];
    let percentile_cap = percentile(sorted_pool, cap_pct);
    let cap = percentile_cap.min(trim_cap);

    levels.iter().map(|&pct| {
        let raw = percentile(sorted_pool, pct);
        LevelTip { level: pct, tip: raw.min(cap) }
    }).collect()
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

pub(crate) async fn handle_rpc_pooled(State(state): State<AppState>, Json(req): Json<RpcRequest>) -> Json<RpcResponse> {
    let params = req.params.and_then(|p| p.into_iter().next()).unwrap_or_default();
    // Percentiles in range 0-10_000 (e.g., 5000 = 50.00%, 9000 = 90.00%)
    let percentiles = params.levels.unwrap_or_else(|| vec![5000]);
    let processors = params.processors.unwrap_or_default();

    let tracker = read_lock(&state.tracker);
    let update = tracker.get_pooled_update(&processors, &percentiles);
    let result = update.data.into_iter().map(|s| ProcessorTips {
        processor: s.processor,
        tips: s.percentiles,
    }).collect();

    Json(RpcResponse { jsonrpc: req.jsonrpc, id: req.id, result })
}

pub(crate) async fn handle_ws_pooled(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_pooled_client(socket, state.tracker, state.tx.subscribe()))
}

async fn handle_ws_pooled_client(mut socket: WebSocket, tracker: SharedTracker, mut rx: broadcast::Receiver<u64>) {
    // Percentiles in range 0-10_000 (e.g., 5000 = 50.00%, 9000 = 90.00%)
    let mut percentiles: Vec<u32> = vec![5000];
    let mut processors: Vec<Processor> = vec![];

    loop {
        tokio::select! {
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(sub) = serde_json::from_str::<WsSubscribe>(&text) {
                            if let Some(l) = sub.levels { percentiles = l; }
                            if let Some(p) = sub.processors { processors = p; }
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
                    t.get_pooled_update(&processors, &percentiles)
                };
                match serde_json::to_string(&update) {
                    Ok(msg) => {
                        if socket.send(Message::Text(msg)).await.is_err() { break; }
                    }
                    Err(e) => {
                        eprintln!("Failed to serialize pooled tip update: {}", e);
                    }
                }
            }
        }
    }
}

pub(crate) async fn handle_rpc_pooled_aggregate(State(state): State<AppState>, Json(req): Json<RpcRequest>) -> Json<AggregateRpcResponse> {
    let params = req.params.and_then(|p| p.into_iter().next()).unwrap_or_default();
    let percentiles = params.levels.unwrap_or_else(|| vec![5000]);
    let processors = params.processors.unwrap_or_default();

    let tracker = read_lock(&state.tracker);
    let result = tracker.get_pooled_aggregate_update(&processors, &percentiles);

    Json(AggregateRpcResponse { jsonrpc: req.jsonrpc, id: req.id, result })
}

pub(crate) async fn handle_ws_pooled_aggregate(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_pooled_aggregate_client(socket, state.tracker, state.tx.subscribe()))
}

async fn handle_ws_pooled_aggregate_client(mut socket: WebSocket, tracker: SharedTracker, mut rx: broadcast::Receiver<u64>) {
    let mut percentiles: Vec<u32> = vec![5000];
    let mut processors: Vec<Processor> = vec![];

    loop {
        tokio::select! {
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(sub) = serde_json::from_str::<WsSubscribe>(&text) {
                            if let Some(l) = sub.levels { percentiles = l; }
                            if let Some(p) = sub.processors { processors = p; }
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
                    t.get_pooled_aggregate_update(&processors, &percentiles)
                };
                match serde_json::to_string(&update) {
                    Ok(msg) => {
                        if socket.send(Message::Text(msg)).await.is_err() { break; }
                    }
                    Err(e) => {
                        eprintln!("Failed to serialize pooled aggregate tip update: {}", e);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pooled_empty_returns_zero_for_every_level() {
        let result = pooled_tip_percentiles(&[], &[5000, 7000, 9000], 9800);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].tip, 0);
        assert_eq!(result[1].tip, 0);
        assert_eq!(result[2].tip, 0);
    }

    #[test]
    fn pooled_pool_below_min_size_returns_zeros() {
        // MIN_POOL_SIZE = 20; 19 samples is below the threshold.
        let pool: Vec<u64> = (1..=19).collect();
        let result = pooled_tip_percentiles(&pool, &[5000, 9000], 9800);
        for lt in &result {
            assert_eq!(lt.tip, 0, "level {} expected 0, got {}", lt.level, lt.tip);
        }
    }

    #[test]
    fn pooled_basic_percentile_no_clamping_on_dense_pool() {
        // n=100 → trim_count = max(5, 2) = 5, trim_cap = pool[94] = 95.
        // percentile_cap (p98) = pool[98] = 99. Effective cap = min(99, 95) = 95.
        // p50 raw = pool[50] = 51; 51 < 95, no clamp.
        let pool: Vec<u64> = (1..=100).collect();
        let result = pooled_tip_percentiles(&pool, &[5000], 9800);
        assert_eq!(result[0].level, 5000);
        assert_eq!(result[0].tip, 51);
    }

    #[test]
    fn pooled_clamps_values_above_cap() {
        // n=100 → effective cap = min(pool[98]=99, pool[94]=95) = 95.
        // p99 raw = pool[99] = 100, clamped to 95.
        let pool: Vec<u64> = (1..=100).collect();
        let result = pooled_tip_percentiles(&pool, &[9900], 9800);
        assert_eq!(result[0].tip, 95);
    }

    #[test]
    fn pooled_levels_at_or_above_cap_all_return_cap() {
        // Same pool as above, cap = 95. All high levels return the cap.
        let pool: Vec<u64> = (1..=100).collect();
        let result = pooled_tip_percentiles(&pool, &[9800, 9900, 9999], 9800);
        for lt in &result {
            assert_eq!(lt.tip, 95, "level {} expected 95, got {}", lt.level, lt.tip);
        }
    }

    #[test]
    fn pooled_level_field_echoes_request() {
        let pool: Vec<u64> = (1..=50).collect();
        let result = pooled_tip_percentiles(&pool, &[1000, 5000, 9000], 9800);
        assert_eq!(result[0].level, 1000);
        assert_eq!(result[1].level, 5000);
        assert_eq!(result[2].level, 9000);
    }

    #[test]
    fn pooled_outlier_does_not_poison_low_percentiles() {
        // 99 samples of value 10, plus one outlier of 1_000_000.
        // Cap = min(pool[98], pool[94]) = min(10, 10) = 10. Low percentiles
        // all return 10 regardless of the outlier — the core winsorization
        // property that motivated /tips/pooled.
        let mut pool: Vec<u64> = vec![10; 99];
        pool.push(1_000_000);
        pool.sort_unstable();
        let result = pooled_tip_percentiles(&pool, &[5000, 7000, 9000, 9800], 9800);
        for lt in &result {
            assert_eq!(lt.tip, 10, "level {} got poisoned: {}", lt.level, lt.tip);
        }
    }

    #[test]
    fn pooled_rank_trim_dominates_on_sparse_pool() {
        // Astralane-shaped scenario: 20 samples, top 5 are whale tips far
        // above the rest. cap_pct = 9800 alone would set the cap at the
        // 4th-largest (still a whale). Rank trim drops the top 5, pinning
        // the cap at pool[14] = the 6th-largest, which is the "normal"
        // range.
        //
        // Pool construction: 15 "normal" tips of 1_000, 5 whales of
        // 10_000_000, sorted ascending.
        let mut pool: Vec<u64> = vec![1_000; 15];
        pool.extend_from_slice(&[10_000_000; 5]);
        pool.sort_unstable();
        // n=20, trim_count = max(5, 0) = 5, trim_cap = pool[20-5-1] = pool[14] = 1_000.
        // percentile_cap (p98) = pool[idx = 9800*20/10000 = 19] = 10_000_000.
        // Effective cap = min(10_000_000, 1_000) = 1_000 — whales don't set the bar.
        let result = pooled_tip_percentiles(&pool, &[5000, 9000, 9800, 9999], 9800);
        for lt in &result {
            assert_eq!(lt.tip, 1_000, "level {} expected 1_000, got {}", lt.level, lt.tip);
        }
    }

    #[test]
    fn pooled_rank_trim_matches_p98_on_dense_pool() {
        // On a dense pool, the rank trim (max(5, n/50)) aligns with p98
        // index such that neither cap dominates the other — the signal
        // matches the pre-trim winsorization behavior. For n=1000,
        // trim_count = max(5, 20) = 20, trim_cap = pool[979]. p98 cap =
        // pool[idx = 9800*1000/10000 = 980]. They're adjacent; the lower
        // one wins, which is trim_cap = pool[979].
        let pool: Vec<u64> = (1..=1000).collect();
        let result = pooled_tip_percentiles(&pool, &[5000], 9800);
        // p50 raw = pool[500] = 501. Cap = pool[979] = 980. 501 < 980, no clamp.
        assert_eq!(result[0].tip, 501);
    }
}
