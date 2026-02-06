use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use axum::routing::{get, post};
use axum::Router;
use futures::{sink::SinkExt, stream::StreamExt};
use maplit::hashmap;
use serde::Deserialize;
use solana_sdk::pubkey::Pubkey;
use tokio::sync::broadcast;
use yellowstone_grpc_client::{ClientTlsConfig, GeyserGrpcClient};
use yellowstone_grpc_proto::prelude::{
    subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequest,
    SubscribeRequestFilterTransactions,
};

pub mod config;
pub mod processor;
pub mod window;
pub mod tips;
pub mod fees;

use processor::Processor;
use tips::{SharedTracker, TipTracker};
use fees::{SharedFeeTracker, FeeTracker};

const SYSTEM_PROGRAM: Pubkey = solana_sdk::system_program::ID;
const PORT: u16 = 7000;

// --- Shared Request Types ---

#[derive(Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(default)]
    pub params: Option<Vec<RpcParams>>,
}

#[derive(Deserialize, Default)]
pub struct RpcParams {
    pub levels: Option<Vec<u32>>,
    pub processors: Option<Vec<Processor>>,
}

#[derive(Deserialize)]
pub struct WindowRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(default)]
    pub params: Option<Vec<WindowParams>>,
}

#[derive(Deserialize, Default)]
pub struct WindowParams {
    pub processors: Option<Vec<Processor>>,
}

#[derive(Deserialize)]
pub struct WsSubscribe {
    #[serde(default)]
    pub levels: Option<Vec<u32>>,
    #[serde(default)]
    pub processors: Option<Vec<Processor>>,
}

#[derive(Clone)]
pub struct AppState {
    pub tracker: SharedTracker,
    pub fee_tracker: SharedFeeTracker,
    pub tx: broadcast::Sender<u64>,
    pub fee_tx: broadcast::Sender<u64>,
}

fn all_accounts() -> Vec<String> {
    Processor::all().iter().flat_map(|p| p.accounts().iter().map(|s| s.to_string())).collect()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = config::Config::init();
    let config = config::Config::get();

    let tracker = Arc::new(RwLock::new(TipTracker::new()));
    let fee_tracker = Arc::new(RwLock::new(FeeTracker::new()));
    let (tx, _) = broadcast::channel::<u64>(16);
    let (fee_tx, _) = broadcast::channel::<u64>(16);

    let state = AppState {
        tracker: tracker.clone(),
        fee_tracker: fee_tracker.clone(),
        tx: tx.clone(),
        fee_tx: fee_tx.clone(),
    };

    let app = Router::new()
        .route("/", post(tips::handle_rpc))
        .route("/window", post(tips::handle_window))
        .route("/ws", get(tips::handle_ws))
        .route("/fees", post(fees::handle_fee_rpc))
        .route("/fees/window", post(fees::handle_fee_window))
        .route("/fees/ws", get(fees::handle_fee_ws))
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
            },
            "fees".to_string() => SubscribeRequestFilterTransactions {
                vote: Some(false),
                account_exclude: config.network.exclude_accounts.clone(),
                ..Default::default()
            }
        },
        commitment: Some(CommitmentLevel::Processed as i32),
        ..Default::default()
    }).await?;

    while let Some(msg) = stream.next().await {
        let msg = msg?;
        let filters = msg.filters;
        let Some(UpdateOneof::Transaction(tx_update)) = msg.update_oneof else { continue };
        let slot = tx_update.slot;
        let info = tx_update.transaction.unwrap();
        let signature = bs58::encode(&info.signature).into_string();
        let meta = match &info.meta { Some(m) => m, None => continue };
        let message = match info.transaction.and_then(|t| t.message) { Some(m) => m, None => continue };

        let is_tip = filters.iter().any(|f| f == "tips");
        let is_fee = filters.iter().any(|f| f == "fees");

        // Tip processing
        if is_tip {
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
                let Some(lamports) = tips::parse_transfer(data) else { continue };
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

        // Priority fee processing (all transactions)
        if is_fee {
            if let Some(cu_consumed) = meta.compute_units_consumed {
                if cu_consumed > 0 {
                    let num_sigs = message.header.as_ref().map(|h| h.num_required_signatures).unwrap_or(1) as u64;
                    let priority_fee_lamports = meta.fee.saturating_sub(5000 * num_sigs);
                    let cu_price = priority_fee_lamports.saturating_mul(1_000_000) / cu_consumed;
                    fee_tracker.write().unwrap().add_fee(slot, signature.clone(), cu_price);
                }
            }

            let mut ft = fee_tracker.write().unwrap();
            if ft.should_broadcast(slot) {
                let _ = fee_tx.send(slot);
            }
        }
    }

    Ok(())
}
