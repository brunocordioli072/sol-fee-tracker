# Sol Fee Tracker

Monitors Solana tips to block builders and priority fees, providing real-time percentile estimates via RPC and WebSocket APIs.

**Features:**
- Tracks tips to 7 block builders (Jito, Nextblock, Sender, Zeroslot, Bloxroute, Astralane, Blockrazor)
- Monitors priority fees across all transactions
- Calculates per-block percentiles averaged across rolling windows
- Percentiles: 0-10,000 format (5000 = 50%, 9800 = 98%)

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

## API Endpoints

### POST /tips
Get tip percentiles by block builder:
```bash
curl -X POST http://127.0.0.1:7000/tips -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"params":[{"levels":[5000,9800],"processors":["jito"]}]}'
```
Returns: `{"level": 5000, "tip": 10000}` (tip in lamports)

### POST /tips/window
Get raw tip data per slot:
```bash
curl -X POST http://127.0.0.1:7000/tips/window -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"params":[{"processors":["jito"]}]}'
```

### POST /fees
Get priority fee percentiles:
```bash
curl -X POST http://127.0.0.1:7000/fees -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"params":[{"levels":[5000,9800]}]}'
```
Returns: `{"level": 5000, "fee": 1000}` (fee in micro-lamports per CU)

### POST /fees/window
Get raw fee data per slot

### WebSockets
- `GET /tips/ws` - Subscribe to tip updates: `{"levels":[5000,9800],"processors":["jito"]}`
- `GET /fees/ws` - Subscribe to fee updates: `{"levels":[5000,9800]}`

## Testing

```bash
cargo run --example test -- [tip-percentiles|tip-window|tip-ws|fee-percentiles|fee-window|fee-ws]
```

## Supported Block Builders

jito, nextblock, sender, zeroslot, bloxroute, astralane, blockrazor
