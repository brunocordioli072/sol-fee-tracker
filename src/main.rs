use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::{Arc, RwLock};

use axum::{
    extract::{State, WebSocketUpgrade, ws::{Message, WebSocket}},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use tokio::sync::broadcast;
use yellowstone_grpc_client::{ClientTlsConfig, GeyserGrpcClient};
use yellowstone_grpc_proto::prelude::{
    subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequest,
    SubscribeRequestFilterTransactions,
};

pub mod config;

const SYSTEM_PROGRAM: Pubkey = solana_sdk::system_program::ID;
const PORT: u16 = 7000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Processor {
    Jito,
    Nextblock,
    Sender,
    Zeroslot,
    Bloxroute,
    Astralane,
    Blockrazor,
}

impl Processor {
    fn all() -> &'static [Processor] {
        &[
            Processor::Jito,
            Processor::Nextblock,
            Processor::Sender,
            Processor::Zeroslot,
            Processor::Bloxroute,
            Processor::Astralane,
            Processor::Blockrazor,
        ]
    }

    fn accounts(&self) -> &'static [&'static str] {
        match self {
            Processor::Jito => &[
                "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
                "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
                "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
                "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
                "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
                "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt",
                "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
                "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
            ],
            Processor::Nextblock => &[
                "NextbLoCkVtMGcV47JzewQdvBpLqT9TxQFozQkN98pE",
                "NexTbLoCkWykbLuB1NkjXgFWkX9oAtcoagQegygXXA2",
                "NeXTBLoCKs9F1y5PJS9CKrFNNLU1keHW71rfh7KgA1X",
                "NexTBLockJYZ7QD7p2byrUa6df8ndV2WSd8GkbWqfbb",
                "neXtBLock1LeC67jYd1QdAa32kbVeubsfPNTJC1V5At",
                "nEXTBLockYgngeRmRrjDV31mGSekVPqZoMGhQEZtPVG",
                "NEXTbLoCkB51HpLBLojQfpyVAMorm3zzKg7w9NFdqid",
                "nextBLoCkPMgmG8ZgJtABeScP35qLa2AMCNKntAP7Xc",
            ],
            Processor::Sender => &[
                "4ACfpUFoaSD9bfPdeu6DBt89gB6ENTeHBXCAi87NhDEE",
                "D2L6yPZ2FmmmTKPgzaMKdhu6EWZcTpLy1Vhx8uvZe7NZ",
                "9bnz4RShgq1hAnLnZbP8kbgBg1kEmcJBYQq3gQbmnSta",
                "5VY91ws6B2hMmBFRsXkoAAdsPHBJwRfBht4DXox3xkwn",
                "2nyhqdwKcJZR2vcqCyrYsaPVdAnFoJjiksCXJ7hfEYgD",
                "2q5pghRs6arqVjRvT5gfgWfWcHWmw1ZuCzphgd5KfWGJ",
                "wyvPkWjVZz1M8fHQnMMCDTQDbkManefNNhweYk5WkcF",
                "3KCKozbAaF75qEU33jtzozcJ29yJuaLJTy2jFdzUY8bT",
                "4vieeGHPYPG2MmyPRcYjdiDmmhN3ww7hsFNap8pVN3Ey",
                "4TQLFNWK8AovT1gFvda5jfw2oJeRMKEmw7aH6MGBJ3or",
            ],
            Processor::Zeroslot => &[
                "Eb2KpSC8uMt9GmzyAEm5Eb1AAAgTjRaXWFjKyFXHZxF3",
                "FCjUJZ1qozm1e8romw216qyfQMaaWKxWsuySnumVCCNe",
                "ENxTEjSQ1YabmUpXAdCgevnHQ9MHdLv8tzFiuiYJqa13",
                "6rYLG55Q9RpsPGvqdPNJs4z5WTxJVatMB8zV3WJhs5EK",
                "Cix2bHfqPcKcM233mzxbLk14kSggUUiz2A87fJtGivXr",
            ],
            Processor::Bloxroute => &[
                "HWEoBxYs7ssKuudEjzjmpfJVX7Dvi7wescFsVx2L5yoY",
                "95cfoy472fcQHaw4tPGBTKpn6ZQnfEPfBgDQx6gcRmRg",
            ],
            Processor::Astralane => &[
                "astrazznxsGUhWShqgNtAdfrzP2G83DzcWVJDxwV9bF",
                "astra4uejePWneqNaJKuFFA8oonqCE1sqF6b45kDMZm",
                "astra9xWY93QyfG6yM8zwsKsRodscjQ2uU2HKNL5prk",
                "astraRVUuTHjpwEVvNBeQEgwYx9w9CFyfxjYoobCZhL",
                "astraEJ2fEj8Xmy6KLG7B3VfbKfsHXhHrNdCQx7iGJK",
                "astraZW5GLFefxNPAatceHhYjfA1ciq9gvfEg2S47xk",
                "astraZW5GLFefxNPAatceHhYjfA1ciq9gvfEg2S47xk",
                "astrawVNP4xDBKT7rAdxrLYiTSTdqtUr63fSMduivXK",
            ],
            Processor::Blockrazor => &[
                "FjmZZrFvhnqqb9ThCuMVnENaM3JGVuGWNyCAxRJcFpg9",
                "6No2i3aawzHsjtThw81iq1EXPJN6rh8eSJCLaYZfKDTG",
                "A9cWowVAiHe9pJfKAj3TJiN9VpbzMUq6E4kEvf5mUT22",
                "Gywj98ophM7GmkDdaWs4isqZnDdFCW7B46TXmKfvyqSm",
                "68Pwb4jS7eZATjDfhmTXgRJjCiZmw1L7Huy4HNpnxJ3o",
                "4ABhJh5rZPjv63RBJBuyWzBK3g9gWMUQdTZP2kiW31V9",
                "B2M4NG5eyZp5SBQrSdtemzk5TqVuaWGQnowGaCBt8GyM",
                "5jA59cXMKQqZAVdtopv8q3yyw9SYfiE3vUCbt7p8MfVf",
                "5YktoWygr1Bp9wiS1xtMtUki1PeYuuzuCF98tqwYxf61",
                "295Avbam4qGShBYK7E9H5Ldew4B3WyJGmgmXfiWdeeyV",
                "EDi4rSy2LZgKJX74mbLTFk4mxoTgT6F7HxxzG2HBAFyK",
                "BnGKHAC386n4Qmv9xtpBVbRaUTKixjBe3oagkPFKtoy6",
                "Dd7K2Fp7AtoN8xCghKDRmyqr5U169t48Tw5fEd3wT9mq",
                "AP6qExwrbRgBAVaehg4b5xHENX815sMabtBzUzVB4v8S",
            ],
        }
    }
}

#[derive(Clone)]
struct TipInfo {
    signature: String,
    lamports: u64,
}

struct TipData {
    blocks: VecDeque<(u64, Vec<TipInfo>)>,
    last_broadcast_slot: u64,
}

impl TipData {
    fn new() -> Self {
        Self { blocks: VecDeque::new(), last_broadcast_slot: 0 }
    }

    fn add_tip(&mut self, slot: u64, signature: String, lamports: u64) {
        if let Some((last_slot, tips)) = self.blocks.back_mut() {
            if *last_slot == slot {
                tips.push(TipInfo { signature, lamports });
                return;
            }
        }
        self.blocks.push_back((slot, vec![TipInfo { signature, lamports }]));
        if self.blocks.len() > config::Config::get().network.max_blocks {
            self.blocks.pop_front();
        }
    }

    fn slot_range(&self) -> (u64, u64) {
        match (self.blocks.front(), self.blocks.back()) {
            (Some((f, _)), Some((l, _))) => (*f, *l),
            _ => (0, 0),
        }
    }
}

struct TipTracker {
    data: HashMap<Processor, TipData>,
    account_to_processor: HashMap<Pubkey, Processor>,
}

impl TipTracker {
    fn new() -> Self {
        let mut data = HashMap::new();
        let mut account_to_processor = HashMap::new();

        for proc in Processor::all() {
            data.insert(*proc, TipData::new());
            for acc in proc.accounts() {
                if let Ok(pk) = Pubkey::from_str(acc) {
                    account_to_processor.insert(pk, *proc);
                }
            }
        }

        Self { data, account_to_processor }
    }

    fn get_processor(&self, pk: &Pubkey) -> Option<Processor> {
        self.account_to_processor.get(pk).copied()
    }

    fn add_tip(&mut self, processor: Processor, slot: u64, signature: String, lamports: u64) {
        if let Some(data) = self.data.get_mut(&processor) {
            data.add_tip(slot, signature, lamports);
        }
    }

    fn should_broadcast(&mut self, slot: u64) -> bool {
        // Check if any processor has enough data and new slot
        let max_blocks = config::Config::get().network.max_blocks;
        for data in self.data.values_mut() {
            if data.blocks.len() >= max_blocks && slot != data.last_broadcast_slot {
                data.last_broadcast_slot = slot;
                return true;
            }
        }
        false
    }

    fn get_processor_stats(&self, proc: Processor, levels: &[u32]) -> ProcessorStats {
        let data = self.data.get(&proc).unwrap();
        let mut tips: Vec<u64> = data.blocks.iter().flat_map(|(_, t)| t.iter().map(|ti| ti.lamports)).collect();
        tips.sort_unstable();
        let (slot_start, slot_end) = data.slot_range();

        let percentiles = levels.iter().map(|&level| {
            let tip = if tips.is_empty() {
                0
            } else {
                let idx = ((tips.len() - 1) as f64 * (level as f64 / 10000.0).min(1.0)).round() as usize;
                tips[idx]
            };
            LevelTip { level, tip }
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

    fn get_percentiles(&self, processors: &[Processor], levels: &[u32]) -> Vec<ProcessorTips> {
        let procs = if processors.is_empty() { Processor::all().to_vec() } else { processors.to_vec() };

        procs.iter().map(|proc| {
            let data = self.data.get(proc).unwrap();
            let mut all_tips: Vec<u64> = data.blocks.iter().flat_map(|(_, t)| t.iter().map(|ti| ti.lamports)).collect();
            all_tips.sort_unstable();

            let tips = levels.iter().map(|&level| {
                let tip = if all_tips.is_empty() {
                    0
                } else {
                    let idx = ((all_tips.len() - 1) as f64 * (level as f64 / 10000.0).min(1.0)).round() as usize;
                    all_tips[idx]
                };
                LevelTip { level, tip }
            }).collect();

            ProcessorTips { processor: *proc, tips }
        }).collect()
    }

    fn get_window(&self, processors: &[Processor]) -> Vec<ProcessorWindow> {
        let procs = if processors.is_empty() { Processor::all().to_vec() } else { processors.to_vec() };

        procs.iter().map(|proc| {
            let data = self.data.get(proc).unwrap();
            let blocks = data.blocks.iter().map(|(slot, tips)| {
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

type SharedTracker = Arc<RwLock<TipTracker>>;

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

#[derive(Deserialize)]
struct RpcRequest {
    jsonrpc: String,
    id: serde_json::Value,
    #[serde(default)]
    params: Option<Vec<RpcParams>>,
}

#[derive(Deserialize, Default)]
struct RpcParams {
    levels: Option<Vec<u32>>,
    processors: Option<Vec<Processor>>,
}

#[derive(Serialize)]
struct RpcResponse {
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

#[derive(Deserialize)]
struct WsSubscribe {
    #[serde(default)]
    levels: Option<Vec<u32>>,
    #[serde(default)]
    processors: Option<Vec<Processor>>,
}

#[derive(Deserialize)]
struct WindowRequest {
    jsonrpc: String,
    id: serde_json::Value,
    #[serde(default)]
    params: Option<Vec<WindowParams>>,
}

#[derive(Deserialize, Default)]
struct WindowParams {
    processors: Option<Vec<Processor>>,
}

#[derive(Serialize)]
struct WindowResponse {
    jsonrpc: String,
    id: serde_json::Value,
    result: Vec<ProcessorWindow>,
}

#[derive(Clone)]
struct AppState {
    tracker: SharedTracker,
    tx: broadcast::Sender<u64>, // broadcasts slot number when new data available
}

fn parse_transfer(data: &[u8]) -> Option<u64> {
    if data.len() < 12 { return None; }
    let ix_type = u32::from_le_bytes(data[0..4].try_into().ok()?);
    if ix_type == 2 || ix_type == 11 {
        Some(u64::from_le_bytes(data[4..12].try_into().ok()?))
    } else {
        None
    }
}

async fn handle_rpc(State(state): State<AppState>, Json(req): Json<RpcRequest>) -> Json<RpcResponse> {
    let params = req.params.and_then(|p| p.into_iter().next()).unwrap_or_default();
    let levels = params.levels.unwrap_or_else(|| vec![5000]);
    let processors = params.processors.unwrap_or_default();

    let tracker = state.tracker.read().unwrap();
    let result = tracker.get_percentiles(&processors, &levels);

    Json(RpcResponse { jsonrpc: req.jsonrpc, id: req.id, result })
}

async fn handle_window(State(state): State<AppState>, Json(req): Json<WindowRequest>) -> Json<WindowResponse> {
    let params = req.params.and_then(|p| p.into_iter().next()).unwrap_or_default();
    let processors = params.processors.unwrap_or_default();

    let tracker = state.tracker.read().unwrap();
    let result = tracker.get_window(&processors);

    Json(WindowResponse { jsonrpc: req.jsonrpc, id: req.id, result })
}

async fn handle_ws(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_client(socket, state.tracker, state.tx.subscribe()))
}

async fn handle_ws_client(mut socket: WebSocket, tracker: SharedTracker, mut rx: broadcast::Receiver<u64>) {
    // Default subscription: p50
    let mut levels: Vec<u32> = vec![5000];
    let mut processors: Vec<Processor> = vec![];

    loop {
        tokio::select! {
            // Handle incoming messages from client (subscription updates)
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
            // Handle broadcast notifications
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

fn all_accounts() -> Vec<String> {
    Processor::all().iter().flat_map(|p| p.accounts().iter().map(|s| s.to_string())).collect()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = config::Config::init();
    let config = config::Config::get();

    let tracker = Arc::new(RwLock::new(TipTracker::new()));
    let (tx, _) = broadcast::channel::<u64>(16);

    let state = AppState { tracker: tracker.clone(), tx: tx.clone() };

    let app = Router::new()
        .route("/", post(handle_rpc))
        .route("/window", post(handle_window))
        .route("/ws", get(handle_ws))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], PORT));
    println!("HTTP: http://{}", addr);
    println!("WS:   ws://{}/ws", addr);

    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });


    let mut client = GeyserGrpcClient::build_from_shared(config.network.grpc_url.clone())?
        .x_token(Some(config.network.grpc_token.clone()))?
        .tls_config(ClientTlsConfig::new().with_native_roots())?
        .connect()
        .await?;

    let (mut subscribe_tx, mut stream) = client.subscribe().await?;

    subscribe_tx.send(SubscribeRequest {
        transactions: hashmap! {
            "tips".to_string() => SubscribeRequestFilterTransactions {
                vote: Some(false),
                account_include: all_accounts(),
                account_exclude: config.network.exclude_accounts.clone(),
                ..Default::default()
            }
        },
        commitment: Some(CommitmentLevel::Processed as i32),
        ..Default::default()
    }).await?;

    while let Some(msg) = stream.next().await {
        let UpdateOneof::Transaction(tx_update) = msg?.update_oneof.unwrap() else { continue };
        let slot = tx_update.slot;
        let info = tx_update.transaction.unwrap();
        let signature = bs58::encode(&info.signature).into_string();
        let meta = match &info.meta { Some(m) => m, None => continue };
        let message = match info.transaction.and_then(|t| t.message) { Some(m) => m, None => continue };

        let mut keys: Vec<Pubkey> = message.account_keys.iter()
            .filter_map(|k| Pubkey::try_from(k.as_slice()).ok())
            .collect();
        for addr in meta.loaded_writable_addresses.iter().chain(&meta.loaded_readonly_addresses) {
            if let Ok(pk) = Pubkey::try_from(addr.as_slice()) { keys.push(pk); }
        }

        let sys_idx = keys.iter().position(|k| *k == SYSTEM_PROGRAM);

        let all_ixs = message.instructions.iter().map(|ix| (&ix.program_id_index, &ix.accounts, &ix.data))
            .chain(meta.inner_instructions.iter().flat_map(|inner|
                inner.instructions.iter().map(|ix| (&ix.program_id_index, &ix.accounts, &ix.data))));

        for (prog_idx, accounts, data) in all_ixs {
            if sys_idx != Some(*prog_idx as usize) || accounts.len() < 2 { continue; }
            let Some(lamports) = parse_transfer(data) else { continue };
            let to_idx = accounts[1] as usize;
            if to_idx >= keys.len() { continue; }

            let tracker_read = tracker.read().unwrap();
            if let Some(processor) = tracker_read.get_processor(&keys[to_idx]) {
                drop(tracker_read);
                tracker.write().unwrap().add_tip(processor, slot, signature.clone(), lamports);
            }
        }

        let mut t = tracker.write().unwrap();
        if t.should_broadcast(slot) {
            let _ = tx.send(slot);
        }
    }

    Ok(())
}
