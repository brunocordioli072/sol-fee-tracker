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
    let data = rpc("/tips", json!({"levels": [5000, 7500, 9800]})).await;
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

    let subscribe = json!({"levels": [5000, 7500, 9800], "processors": ["jito"]});
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
    let data = rpc("/fees", json!({"levels": [5000, 7500, 9800]})).await;
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
        _ => {
            eprintln!("Usage: cargo run --example test -- <tip-percentiles|tip-window|tip-ws|fee-percentiles|fee-window|fee-ws>");
            std::process::exit(1);
        }
    }
}
