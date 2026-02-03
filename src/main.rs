use serde::Deserialize;
use std::collections::VecDeque;
use std::fs;
use thorstreamer_grpc_client::{
    ClientConfig, ThorClient, parse_message,
    proto::thor_streamer::types::message_wrapper::EventMessage,
};

// Jito tip account addresses (base58 decoded to bytes for comparison)
const JITO_TIP_ACCOUNTS: &[&str] = &["DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh","ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt","DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL","HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe","96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5","Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY","3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT","ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49"];

#[derive(Deserialize)]
struct Config {
    server: ServerConfig,
}

#[derive(Deserialize)]
struct ServerConfig {
    addr: String,
    token: String,
}

#[derive(Debug, Clone)]
struct TxInfo {
    signature: String,
    jito_tip: Option<u64>, // tip amount in lamports if sent to Jito
}

#[derive(Debug)]
struct BlockData {
    slot: u64,
    transactions: Vec<TxInfo>,
    total_jito_tips: u64,
}

fn pubkey_to_base58(bytes: &[u8]) -> String {
    bs58::encode(bytes).into_string()
}

fn is_jito_tip_account(pubkey: &str) -> bool {
    JITO_TIP_ACCOUNTS.contains(&pubkey)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_str = fs::read_to_string("config.toml")?;
    let app_config: Config = toml::from_str(&config_str)?;

    let config = ClientConfig {
        server_addr: app_config.server.addr,
        token: app_config.server.token,
        ..Default::default()
    };

    let mut client = ThorClient::new(config).await?;

    // Store last 20 blocks
    let mut blocks: VecDeque<BlockData> = VecDeque::with_capacity(21);
    let mut current_block: Option<BlockData> = None;

    // Subscribe to transactions
    let mut stream = client.subscribe_to_transactions().await?;

    println!("Listening for transactions...\n");

    while let Some(response) = stream.message().await? {
        let msg = parse_message(&response.data)?;

        if let Some(EventMessage::Transaction(tx)) = msg.event_message {
            let slot = tx.slot;
            let signature = bs58::encode(&tx.signature).into_string();

            // Check for Jito tips in the transaction
            let mut jito_tip: Option<u64> = None;

            // Look through account keys and balance changes to detect Jito tips
            for (i, account) in tx.transaction.unwrap().message.unwrap().account_keys.iter().enumerate() {
                let pubkey = pubkey_to_base58(account);
                if is_jito_tip_account(&pubkey) {
                    // Check if there's a balance increase for this account
                    if let (Some(pre), Some(post)) = (
                        tx.transaction_status_meta.clone().unwrap().pre_balances.get(i),
                        tx.transaction_status_meta.clone().unwrap().post_balances.get(i),
                    ) {
                        if post > pre {
                            let tip = post - pre;
                            jito_tip = Some(jito_tip.unwrap_or(0) + tip);
                        }
                    }
                }
            }

            // Print transaction with Jito tip
            // if let Some(tip) = jito_tip {
            //     println!(
            //         "🎯 Jito tip detected! sig: {} | tip: {:.6} SOL",
            //         signature,
            //         tip as f64 / 1_000_000_000.0
            //     );
            // }

            let tx_info = TxInfo {
                signature: signature.clone(),
                jito_tip,
            };

            // Handle block transitions
            match &mut current_block {
                Some(block) if block.slot == slot => {
                    if let Some(tip) = jito_tip {
                        block.total_jito_tips += tip;
                    }
                    block.transactions.push(tx_info);
                }
                _ => {
                    // New block started - save the previous one
                    if let Some(completed_block) = current_block.take() {
                        println!(
                            "Block {} completed: {} txs, Jito tips: {} SOL",
                            completed_block.slot,
                            completed_block.transactions.len(),
                            completed_block.total_jito_tips as f64 / 1_000_000_000.0
                        );

                        blocks.push_back(completed_block);

                        // Keep only last 20 blocks
                        while blocks.len() > 20 {
                            blocks.pop_front();
                        }

                        // Print summary of last 20 blocks
                        print_summary(&blocks);
                    }

                    current_block = Some(BlockData {
                        slot,
                        transactions: vec![tx_info],
                        total_jito_tips: jito_tip.unwrap_or(0),
                    });
                }
            }
        }
    }

    Ok(())
}

fn print_summary(blocks: &VecDeque<BlockData>) {
    let total_txs: usize = blocks.iter().map(|b| b.transactions.len()).sum();
    let total_tips: u64 = blocks.iter().map(|b| b.total_jito_tips).sum();
    let txs_with_tips: usize = blocks
        .iter()
        .flat_map(|b| &b.transactions)
        .filter(|tx| tx.jito_tip.is_some())
        .count();

    println!("\n=== Last {} Blocks Summary ===", blocks.len());
    println!("Total transactions: {}", total_txs);
    println!("Transactions with Jito tips: {}", txs_with_tips);
    println!("Total Jito tips: {:.6} SOL", total_tips as f64 / 1_000_000_000.0);

    if txs_with_tips > 0 {
        let avg_tip = total_tips as f64 / txs_with_tips as f64 / 1_000_000_000.0;
        println!("Average tip: {:.6} SOL", avg_tip);
    }
    println!("===============================\n");
}