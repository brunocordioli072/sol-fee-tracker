use std::collections::HashMap;
use std::env;

use futures_util::{SinkExt, StreamExt};
use reqwest::Client;
use serde_json::{json, Value};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const BASE_URL: &str = "http://127.0.0.1:7000";
const WS_URL: &str = "ws://127.0.0.1:7000";

async fn rpc(endpoint: &str, params: Value) -> Value {
    let client = Client::new();
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "params": [params]
    });
    client
        .post(format!("{BASE_URL}{endpoint}"))
        .json(&body)
        .send()
        .await
        .expect("request failed — is the server running?")
        .json()
        .await
        .expect("failed to parse response")
}

fn pretty(v: &Value) {
    println!("{}", serde_json::to_string_pretty(v).unwrap());
}

async fn test_tip_percentiles() {
    let data = rpc("/tips", json!({"levels": [5000, 7500, 9000]})).await;
    pretty(&data);
}

async fn test_tip_window() {
    let data = rpc("/tips/window", json!({"processors": ["jito"]})).await;
    pretty(&data);
}

async fn test_tip_ws() {
    let (mut socket, _) = connect_async(format!("{WS_URL}/tips/ws"))
        .await
        .expect("ws connect failed — is the server running?");

    let subscribe = json!({"levels": [5000, 7500, 9000], "processors": ["jito"]});
    socket.send(Message::Text(subscribe.to_string())).await.unwrap();
    println!("Connected");

    while let Some(Ok(msg)) = socket.next().await {
        match msg {
            Message::Text(text) => {
                let v: Value = serde_json::from_str(&text).unwrap();
                pretty(&v);
            }
            Message::Close(_) => {
                println!("Disconnected");
                break;
            }
            _ => {}
        }
    }
}

async fn test_fee_percentiles() {
    let data = rpc("/fees", json!({"levels": [5000, 7500, 9000]})).await;
    pretty(&data);
}

async fn test_fee_window() {
    let data = rpc("/fees/window", json!({})).await;
    pretty(&data);
}

async fn test_fee_ws() {
    let (mut socket, _) = connect_async(format!("{WS_URL}/fees/ws"))
        .await
        .expect("ws connect failed — is the server running?");

    let subscribe = json!({"levels": [5000, 8500, 9000]});
    socket.send(Message::Text(subscribe.to_string())).await.unwrap();
    println!("Connected");

    while let Some(Ok(msg)) = socket.next().await {
        match msg {
            Message::Text(text) => {
                let v: Value = serde_json::from_str(&text).unwrap();
                pretty(&v);
            }
            Message::Close(_) => {
                println!("Disconnected");
                break;
            }
            _ => {}
        }
    }
}

async fn test_fee_pooled_percentiles() {
    let data = rpc("/fees/pooled", json!({"levels": [5000, 7500, 9000]})).await;
    pretty(&data);
}

async fn test_fee_pooled_ws() {
    let (mut socket, _) = connect_async(format!("{WS_URL}/fees/pooled/ws"))
        .await
        .expect("ws connect failed — is the server running?");

    let subscribe = json!({"levels": [5000, 8500, 9000]});
    socket.send(Message::Text(subscribe.to_string())).await.unwrap();
    println!("Connected");

    while let Some(Ok(msg)) = socket.next().await {
        match msg {
            Message::Text(text) => {
                let v: Value = serde_json::from_str(&text).unwrap();
                pretty(&v);
            }
            Message::Close(_) => {
                println!("Disconnected");
                break;
            }
            _ => {}
        }
    }
}

async fn test_fee_compare_ws() {
    let levels = [5000u64, 7000, 9000, 9800];
    let subscribe = json!({"levels": levels});

    let (mut fees_sock, _) = connect_async(format!("{WS_URL}/fees/ws"))
        .await
        .expect("ws connect failed — is the server running?");
    let (mut pooled_sock, _) = connect_async(format!("{WS_URL}/fees/pooled/ws"))
        .await
        .expect("ws connect failed — is the server running?");

    fees_sock.send(Message::Text(subscribe.to_string())).await.unwrap();
    pooled_sock.send(Message::Text(subscribe.to_string())).await.unwrap();
    println!("Connected — comparing /fees/ws vs /fees/pooled/ws");
    println!("Levels: {:?}\n", levels);

    let mut pending_fees: HashMap<u64, Value> = HashMap::new();
    let mut pending_pooled: HashMap<u64, Value> = HashMap::new();

    loop {
        tokio::select! {
            msg = fees_sock.next() => match msg {
                Some(Ok(Message::Text(text))) => {
                    let v: Value = serde_json::from_str(&text).unwrap();
                    let slot = v["slot_end"].as_u64().unwrap_or(0);
                    if let Some(pooled) = pending_pooled.remove(&slot) {
                        print_compare(slot, &v, &pooled);
                    } else {
                        pending_fees.insert(slot, v);
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                _ => {}
            },
            msg = pooled_sock.next() => match msg {
                Some(Ok(Message::Text(text))) => {
                    let v: Value = serde_json::from_str(&text).unwrap();
                    let slot = v["slot_end"].as_u64().unwrap_or(0);
                    if let Some(fees) = pending_fees.remove(&slot) {
                        print_compare(slot, &fees, &v);
                    } else {
                        pending_pooled.insert(slot, v);
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                _ => {}
            },
        }
    }
}

fn print_compare(slot: u64, fees: &Value, pooled: &Value) {
    let count = fees["count"].as_u64().unwrap_or(0);
    let fees_pcts = fees["percentiles"].as_array().unwrap();
    let pooled_pcts = pooled["percentiles"].as_array().unwrap();

    println!("slot={slot} count={count}");
    println!("  {:>6} {:>15} {:>15}  {}", "level", "/fees", "/fees/pooled", "Δ");
    for (f, p) in fees_pcts.iter().zip(pooled_pcts.iter()) {
        let level = f["level"].as_u64().unwrap_or(0);
        let fees_fee = f["fee"].as_u64().unwrap_or(0);
        let pooled_fee = p["fee"].as_u64().unwrap_or(0);
        let delta = if fees_fee > 0 {
            format!("{:+.0}%", (pooled_fee as f64 / fees_fee as f64 - 1.0) * 100.0)
        } else {
            "—".into()
        };
        println!("  {:>6} {:>15} {:>15}  {}", level, fmt_num(fees_fee), fmt_num(pooled_fee), delta);
    }
    println!();
}

fn fmt_num(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 { out.insert(0, ','); }
        out.insert(0, c);
    }
    out
}

#[tokio::main]
async fn main() {
    let cmd = env::args().nth(1);

    match cmd.as_deref() {
        Some("tip-percentiles") => test_tip_percentiles().await,
        Some("tip-window") => test_tip_window().await,
        Some("tip-ws") => test_tip_ws().await,
        Some("fee-percentiles") => test_fee_percentiles().await,
        Some("fee-window") => test_fee_window().await,
        Some("fee-ws") => test_fee_ws().await,
        Some("fee-pooled-percentiles") => test_fee_pooled_percentiles().await,
        Some("fee-pooled-ws") => test_fee_pooled_ws().await,
        Some("fee-compare-ws") => test_fee_compare_ws().await,
        _ => {
            eprintln!("Usage: cargo run --example test -- <tip-percentiles|tip-window|tip-ws|fee-percentiles|fee-window|fee-ws|fee-pooled-percentiles|fee-pooled-ws|fee-compare-ws>");
            std::process::exit(1);
        }
    }
}
