use std::str::FromStr;

use {
    futures::{sink::SinkExt, stream::StreamExt}, yellowstone_grpc_client::{ClientTlsConfig, GeyserGrpcClient}, yellowstone_grpc_proto::prelude::{
            subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequest,
            SubscribeRequestFilterTransactions,
        }
};
use solana_sdk::pubkey::Pubkey;

use maplit;

pub mod config;

// Known Jito tip accounts
const TIP_ACCOUNTS: &[&str] = &[
    "Eb2KpSC8uMt9GmzyAEm5Eb1AAAgTjRaXWFjKyFXHZxF3",
    "FCjUJZ1qozm1e8romw216qyfQMaaWKxWsuySnumVCCNe",
    "ENxTEjSQ1YabmUpXAdCgevnHQ9MHdLv8tzFiuiYJqa13",
    "6rYLG55Q9RpsPGvqdPNJs4z5WTxJVatMB8zV3WJhs5EK",
    "Cix2bHfqPcKcM233mzxbLk14kSggUUiz2A87fJtGivXr",
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = config::Config::init();
    let config = config::Config::get();

    
    let mut client = GeyserGrpcClient::build_from_shared(config.network.grpc_url.clone())?
        .x_token(Some(config.network.grpc_token.clone()))?
        .tls_config(ClientTlsConfig::new().with_native_roots())?
        .connect()
        .await?;

    let (mut subscribe_tx, mut stream) = client.subscribe().await?;

    futures::try_join!(
        async move {
            subscribe_tx
                .send(SubscribeRequest {
                    transactions: maplit::hashmap! {
                        "tip_accounts".to_string() => SubscribeRequestFilterTransactions {
                            vote: Some(false),   // drop vote txs
                            failed: Some(true), // optional
                            account_include: TIP_ACCOUNTS
                                .iter()
                                .map(|s| s.to_string())
                                .collect(),
                            account_exclude: vec![],
                            account_required: vec![],
                            ..Default::default()
                        }
                    },
                    commitment: Some(CommitmentLevel::Processed as i32),
                    ..Default::default()
                })
                .await?;

            Ok::<(), anyhow::Error>(())
        },
        async move {
            while let Some(message) = stream.next().await {
                match message?.update_oneof.expect("valid message") {
                    UpdateOneof::Transaction(tx) => {
                        let info = tx.transaction.unwrap();
                        let sig = info.signature.clone();
                        let meta = match info.meta {
                            Some(m) => m,
                            None => continue,
                        };

                        let keys = info.transaction.unwrap().message.unwrap().account_keys;
                        // compute actual tip amount
                       let jito_keys: Vec<[u8; 32]> = TIP_ACCOUNTS
                        .iter()
                        .map(|s| Pubkey::from_str(s).unwrap().to_bytes())
                        .collect();

                        for (i, key) in keys.iter().enumerate() {
                            // 👇 SIMPLE byte comparison
                            if jito_keys.iter().any(|k| k.as_slice() == key.as_slice()) {
                                println!(
                                    "sig: {} slot: {}",
                                    solana_sdk::bs58::encode(&sig).into_string(),
                                    tx.slot,
                                );
                            }
                        }
                    }
                    msg => (),
                }
            }
            Ok::<(), anyhow::Error>(())
        }
    )?;

    Ok(())
}