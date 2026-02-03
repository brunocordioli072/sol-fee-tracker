# Dynamic Tip

A Solana tip tracking service that monitors tips sent to various block builders and provides real-time estimates.

## What It Does

- Connects to Solana via Yellowstone gRPC to stream transaction data
- Tracks tip amounts sent to 7 block builders: Jito, Nextblock, Sender, Zeroslot, Bloxroute, Astralane, and Blockrazor
- Maintains a rolling window of the last 50 blocks per processor
- Calculates percentile-based tip estimates (e.g., p50, p75, p98)
- Exposes data via RPC and WebSocket APIs

## Setup

1. Create `config.toml`:
```toml
[network]
grpc_url = "https://grpc.ny.shyft.to"
grpc_token = "your-token"
```

2. Run:
```bash
cargo run --release
```

Server starts on `http://127.0.0.1:7000`

## Usage

### RPC (POST /)

Request fee percentiles:
```bash
curl -X POST http://127.0.0.1:7000 \
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

- `levels`: percentiles in basis points (5000 = p50, 9800 = p98)
- `processors`: optional filter (e.g., `["jito","nextblock"]`)
- `tip`: tip amount in lamports

### WebSocket (GET /ws)

Connect and receive updates when new data is available:
```bash
websocat ws://127.0.0.1:7000/ws
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

## Supported Processors

- jito
- nextblock
- sender
- zeroslot
- bloxroute
- astralane
- blockrazor
