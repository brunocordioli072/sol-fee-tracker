# Sol Fee Tracker

Monitors Solana tips to block builders and priority fees, providing real-time percentile estimates via RPC and WebSocket APIs.

**Features:**
- Tracks tips to 8 block builders (Jito, Nextblock, Sender, Zeroslot, Bloxroute, Astralane, Blockrazor, Nozomi)
- Monitors priority fees across all transactions
- Two percentile strategies per signal:
  - **Per-block-averaged** (`/tips`, `/fees`) — average of per-block percentiles; closest to "what's the typical top tip in a block"
  - **Pooled + winsorized** (`/tips/pooled`, `/tips/pooled/aggregate`, `/fees/pooled`) — dust-filtered pooled sample with rank-trim outlier cap; stable signal for dynamic-tip logic
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

## Choosing a Tip Endpoint

Tipping markets differ by builder. Pick the endpoint that matches the market you're operating in:

| Endpoint | Signal | When to use |
|----------|--------|-------------|
| `/tips` | Per-block-percentile, averaged across the window | **Jito** and other bundle-auction markets — "what does the winning tip look like?" is the relevant question, and averaging per-block maxes answers it. |
| `/tips/pooled` | Dust-filtered + rank-trim-winsorized pool, per processor | Single-builder dynamic tipping on any processor. Stable under outlier churn; returns `0` when the pool is <20 non-dust samples (use `MinTip` fallback). |
| `/tips/pooled/aggregate` | Same treatment applied to the **union** of multiple builders' pools | Fast-path routing where you send the same transaction to several builders and pay one tip to whichever lands. Dramatically denser sample (~4,000+ tips/window across 6 fast-path builders) — the most stable dynamic-tip signal. |

Typical production setup for a bot that routes across Jito + fast-path builders:
- Jito consumes `/tips/pooled/ws` with `processors: ["jito"]`.
- Fast-path builders (Astralane, Sender, Zeroslot, Bloxroute, Blockrazor, Nozomi) all consume **one** `/tips/pooled/aggregate/ws` subscription and share the resulting percentile as their dynamic tip.
- Always pair a dynamic percentile with `MinTip` (floor) and `FeeCap` (ceiling). The dynamic value can be 0 during startup or low-activity periods.

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

### POST /tips/pooled
Get tip percentiles from a pooled, winsorized sample across the rolling window, per processor. Designed for use as a "what's the competitive tip right now" signal across all builders, including bimodal ones like Astralane where `/tips` mixes MEV-bot whale tips with normal trader tips.

The pool excludes:
- Tips below `10_000` lamports (dust/spam), so low percentiles aren't dragged toward zero.
- The top `max(5, N/50)` samples by rank (whale/outlier trim), so sparse-pool percentiles aren't dominated by a handful of panic tips. On dense pools (Jito-sized, ~1000+ samples) this matches classic p98 winsorization; on sparse pools (Astralane-sized, ~150 samples) it's stricter.

Any requested level at or above `9800` returns the cap value. Pools with fewer than 20 non-dust samples return `tip: 0` — treat as "insufficient data" and fall back to a minimum tip.

```bash
curl -X POST http://127.0.0.1:7000/tips/pooled -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"params":[{"levels":[5000,9000],"processors":["jito"]}]}'
```
Returns: `{"level": 5000, "tip": 10000}` (tip in lamports)

### POST /tips/pooled/aggregate
Get **one** set of tip percentiles computed on the union of the requested processors' pools. Designed for consumers that route the same transaction to multiple fast-path builders and pay a single tip value to whichever lands first — typical for bot-detection-resistant send paths that hit every builder except Jito.

Same dust filter (≥10,000 lamports) and rank-trim winsorization as `/tips/pooled`. With 6 fast-path builders pooled you typically get 300–600 samples per window instead of the 60–150 you'd see per-builder, which makes percentiles substantially more stable under whale churn.

Response contains `slot_start`, `slot_end` (union of per-processor windows), `count` (total tips across included processors pre-dust), `processors` (what was included), and `percentiles`.

```bash
curl -X POST http://127.0.0.1:7000/tips/pooled/aggregate -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"params":[{"levels":[5000,9000],"processors":["astralane","sender","zeroslot","bloxroute","blockrazor","nozomi"]}]}'
```

Omit `processors` (or pass an empty list) to pool across all builders.

### POST /fees
Get priority fee percentiles:
```bash
curl -X POST http://127.0.0.1:7000/fees -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"params":[{"levels":[5000,9800]}]}'
```
Returns: `{"level": 5000, "fee": 1000}` (fee in micro-lamports per CU)

### POST /fees/window
Get raw fee data per slot

### POST /fees/pooled
Get priority fee percentiles from a pooled, winsorized sample across the rolling window. More stable than `/fees` during quiet periods — pools all priority fees across complete blocks and clamps anything above p98 before computing percentiles. Any requested level at or above 9800 returns the same value (the cap); for unmodified high percentiles use `/fees`.
```bash
curl -X POST http://127.0.0.1:7000/fees/pooled -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"params":[{"levels":[5000,9000]}]}'
```
Returns: `{"level": 5000, "fee": 1000}` (fee in micro-lamports per CU)

### WebSockets
- `GET /tips/ws` - Subscribe to tip updates: `{"levels":[5000,9800],"processors":["jito"]}`
- `GET /tips/pooled/ws` - Subscribe to pooled (winsorized) tip updates: `{"levels":[5000,9000],"processors":["jito"]}`
- `GET /tips/pooled/aggregate/ws` - Subscribe to aggregate pooled tip updates across multiple builders: `{"levels":[5000,9000],"processors":["astralane","sender","zeroslot","bloxroute","blockrazor","nozomi"]}`
- `GET /fees/ws` - Subscribe to fee updates: `{"levels":[5000,9800]}`
- `GET /fees/pooled/ws` - Subscribe to pooled (winsorized) fee updates: `{"levels":[5000,9000]}`

## Testing

```bash
cargo run --example test -- [tip-percentiles|tip-window|tip-ws|tip-pooled-percentiles|tip-pooled-ws|tip-compare-ws|tip-pooled-aggregate-percentiles|tip-pooled-aggregate-ws|fee-percentiles|fee-window|fee-ws|fee-pooled-percentiles|fee-pooled-ws|fee-compare-ws]
```

## Supported Block Builders

jito, nextblock, sender, zeroslot, bloxroute, astralane, blockrazor, nozomi
