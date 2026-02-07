# Sol Fee Tracker

A Solana monitoring service that tracks tips sent to block builders and priority fees across all transactions, providing real-time estimates.

## What It Does

- Connects to Solana via Yellowstone gRPC to stream transaction data
- Tracks tip amounts sent to 7 block builders: Jito, Nextblock, Sender, Zeroslot, Bloxroute, Astralane, and Blockrazor
- Tracks priority fees across all transactions
- Maintains configurable rolling windows
- Calculates percentile-based tip and fee estimates (e.g., p50, p75, p98)
- Exposes data via RPC and WebSocket APIs

## How Percentiles Are Calculated

### Percentile Format
Percentiles are specified in the range **0 to 10,000** representing 0.00% to 100.00%:
- `5000` = 50.00% (median)
- `7500` = 75.00%
- `9000` = 90.00%
- `9900` = 99.00%
- `9999` = 99.99%

This format allows for 0.01% precision in percentile calculations.

### Calculation Method
The service uses a **per-block averaging approach**:

1. **Per-Block Percentiles**: For each complete block (slot), calculate the percentile using:
   ```
   index = min(percentile, 9999) × N ÷ 10000
   value = sorted_array[index]
   ```
   Where `N` is the number of transactions in that block.

2. **Average Across Blocks**: Take the arithmetic mean of the per-block percentile values across all blocks in the rolling window.

3. **Incomplete Block Exclusion**: The most recent block (still forming) is excluded from calculations to avoid skewed results from incomplete data.

**Example:** If you request percentile `9000` (90th percentile):
- Block 1 (complete): 1000 txs → sorts them → returns value at index 900
- Block 2 (complete): 800 txs → sorts them → returns value at index 720
- Block 3 (forming): excluded from calculation
- **Result**: Average of Block 1 and Block 2 percentile values

This approach gives equal weight to each complete block, providing stable estimates that aren't overly influenced by high-volume blocks.

## Setup

1. Create `config.toml`:
```toml
[network]
grpc_url = "https://grpc.ny.shyft.to"
grpc_token = "your-token"
max_tip_blocks = 25
max_fee_blocks = 10
exclude_accounts = []
```

2. Run:
```bash
cargo run --release
```

Server starts on `http://0.0.0.0:7000`

## Usage

### RPC (POST /tips)

Request tip percentiles:
```bash
curl -X POST http://127.0.0.1:7000/tips \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"params":[{"levels":[5000,9800]}]}'
```

Response:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": [
    {
      "processor": "jito",
      "tips": [
        {"level": 5000, "tip": 10000},
        {"level": 9800, "tip": 100000}
      ]
    },
    {
      "processor": "nextblock",
      "tips": [
        {"level": 5000, "tip": 8000},
        {"level": 9800, "tip": 85000}
      ]
    }
  ]
}
```

- `levels`: percentiles in range 0-10,000 (5000 = 50.00%, 9800 = 98.00%)
- `processors`: optional filter (e.g., `["jito","nextblock"]`)
- `tip`: tip amount in lamports

### Window (POST /tips/window)

Get the raw rolling window data (all tips per slot):
```bash
curl -X POST http://127.0.0.1:7000/tips/window \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"params":[{"processors":["jito"]}]}'
```

Response:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": [
    {
      "processor": "jito",
      "blocks": [
        {
          "slot": 312345678,
          "tips": [
            {"signature": "5K8s...abc", "tip": 10000},
            {"signature": "3Jx2...def", "tip": 25000}
          ]
        },
        {
          "slot": 312345679,
          "tips": [
            {"signature": "7Ym4...ghi", "tip": 15000}
          ]
        }
      ]
    }
  ]
}
```

- `processors`: optional filter (e.g., `["jito","nextblock"]`)
- `blocks`: array of slots with tips
- `signature`: transaction signature (base58)
- `tip`: tip amount in lamports

### Fee Percentiles (POST /fees)

Request priority fee percentiles across all transactions:
```bash
curl -X POST http://127.0.0.1:7000/fees \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"params":[{"levels":[5000,7500,9800]}]}'
```

Response:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": [
    {"level": 5000, "fee": 1000},
    {"level": 7500, "fee": 5000},
    {"level": 9800, "fee": 50000}
  ]
}
```

- `levels`: percentiles in range 0-10,000 (5000 = 50.00%, 9800 = 98.00%)
- `fee`: priority fee in micro-lamports per compute unit

### Fee Window (POST /fees/window)

Get raw rolling window of priority fees per slot:
```bash
curl -X POST http://127.0.0.1:7000/fees/window \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1}'
```

Response:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": [
    {
      "slot": 312345678,
      "fees": [
        {"signature": "5K8s...abc", "priority_fee": 1000},
        {"signature": "3Jx2...def", "priority_fee": 5000}
      ]
    }
  ]
}
```

### Fee WebSocket (GET /fees/ws)

Connect and receive priority fee updates:
```bash
websocat ws://127.0.0.1:7000/fees/ws
```

Send a subscribe message:
```json
{"levels":[5000,8500,9000]}
```

Response (on each update):
```json
{
  "slot_start": 312345678,
  "slot_end": 312345728,
  "count": 5000,
  "percentiles": [
    {"level": 5000, "fee": 1000},
    {"level": 8500, "fee": 10000},
    {"level": 9000, "fee": 25000}
  ]
}
```

### Tip WebSocket (GET /tips/ws)

Connect and receive updates when new data is available:
```bash
websocat ws://127.0.0.1:7000/tips/ws
```

Send a subscribe message to set your percentiles:
```json
{"levels":[5000,7500,9800],"processors":["jito"]}
```

Response (on each update):
```json
{
  "data": [
    {
      "processor": "jito",
      "slot_start": 312345678,
      "slot_end": 312345728,
      "tips": 150,
      "percentiles": [
        {"level": 5000, "tip": 10000},
        {"level": 7500, "tip": 50000},
        {"level": 9800, "tip": 100000}
      ]
    }
  ]
}
```

## Testing

Run endpoint tests against a running server using the Rust example binary:

```bash
cargo run --example test -- tip-percentiles
cargo run --example test -- tip-window
cargo run --example test -- tip-ws
cargo run --example test -- fee-percentiles
cargo run --example test -- fee-window
cargo run --example test -- fee-ws
```

## Supported Processors

- jito
- nextblock
- sender
- zeroslot
- bloxroute
- astralane
- blockrazor
