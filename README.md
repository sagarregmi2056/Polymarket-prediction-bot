# Polymarket Arbitrage Bot

An arbitrage system for prediction market trading on Polymarket.

**📖 [Complete Workflow Guide](./WORKFLOW.md)** - Detailed explanation of how the bot works end-to-end

**📊 [Data Sources Guide](./DATA_SOURCES.md)** - All external APIs, services, and data requirements

## Quick Start

### Option 1: Docker (Recommended)

The easiest way to run the bot is using Docker. No need to install Rust or dependencies!

#### Prerequisites

- Docker and Docker Compose installed
- Polymarket credentials (see [Obtaining Credentials](#obtaining-credentials))

#### Steps

1. **Clone the repository**
   ```bash
   git clone <repository-url>
   cd poly-kalshi-arb
   ```

2. **Create environment file**
   ```bash
   cp .env.example .env
   # Edit .env and add your credentials
   ```

3. **Build and run with Docker Compose**
   ```bash
   # Build the image
   docker-compose build
   
   # Run in dry-run mode (paper trading)
   docker-compose up
   
   # Or run in detached mode
   docker-compose up -d
   
   # View logs
   docker-compose logs -f
   ```

4. **Run in production mode**
   ```bash
   # Use production compose file
   docker-compose -f docker-compose.yml -f docker-compose.prod.yml up -d
   ```

#### Docker Commands

```bash
# Stop the bot
docker-compose down

# Restart the bot
docker-compose restart

# View logs
docker-compose logs -f polymarket-bot

# Execute commands inside container
docker-compose exec polymarket-bot sh

# Rebuild after code changes
docker-compose build --no-cache
docker-compose up -d
```

#### Docker Run (without Compose)

```bash
# Build the image
docker build -t polymarket-bot .

# Run with environment variables
docker run -d \
  --name polymarket-bot \
  -e POLY_PRIVATE_KEY=0xYOUR_KEY \
  -e POLY_FUNDER=0xYOUR_ADDRESS \
  -e DRY_RUN=1 \
  -e RUST_LOG=info \
  -v $(pwd)/data:/app/data \
  polymarket-bot

# View logs
docker logs -f polymarket-bot
```

### Option 2: Native Installation

#### 1. Install Dependencies

```bash
# Rust 1.75+
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build
cd poly-kalshi-arb
cargo build --release
```

#### 2. Set Up Credentials

Create a `.env` file:

```bash
# === POLYMARKET CREDENTIALS ===
POLY_PRIVATE_KEY=0xYOUR_WALLET_PRIVATE_KEY
POLY_FUNDER=0xYOUR_WALLET_ADDRESS

# === BOT CONFIGURATION ===
DRY_RUN=1
RUST_LOG=info
```

#### 3. Run

```bash
# Dry run (paper trading)
dotenvx run -- cargo run --release

# Live execution
DRY_RUN=0 dotenvx run -- cargo run --release
```

---

## Docker Configuration

### Volumes

The Docker setup uses volumes to persist data:

- `./data/positions.json` - Position tracking data
- `./data/.discovery_cache.json` - Market discovery cache
- `./data/.clob_market_cache.json` - CLOB market cache
- `./logs` - Application logs (optional)

Create the data directory before running:
```bash
mkdir -p data logs
```

### Environment Variables in Docker

You can set environment variables in several ways:

1. **Using `.env` file** (recommended for development)
   ```bash
   # Create .env file
   POLY_PRIVATE_KEY=0x...
   POLY_FUNDER=0x...
   DRY_RUN=1
   ```

2. **Using docker-compose.yml** (already configured)
   ```yaml
   environment:
     - POLY_PRIVATE_KEY=${POLY_PRIVATE_KEY}
     - DRY_RUN=1
   ```

3. **Using command line**
   ```bash
   docker run -e POLY_PRIVATE_KEY=0x... -e DRY_RUN=1 polymarket-bot
   ```

### Production Deployment

For production, use Docker secrets:

1. Create secrets directory:
   ```bash
   mkdir -p secrets
   echo "0xYOUR_PRIVATE_KEY" > secrets/poly_private_key.txt
   echo "0xYOUR_ADDRESS" > secrets/poly_funder.txt
   chmod 600 secrets/*.txt
   ```

2. Run with production compose:
   ```bash
   docker-compose -f docker-compose.yml -f docker-compose.prod.yml up -d
   ```

### Health Checks

The Docker container includes a health check that verifies the bot process is running. Check status:

```bash
docker-compose ps
```

---

## Environment Variables

### Required

| Variable                  | Description                                                 |
| ------------------------- | ----------------------------------------------------------- |
| `POLY_PRIVATE_KEY`        | Ethereum private key (with 0x prefix) for Polymarket wallet |
| `POLY_FUNDER`             | Your Polymarket wallet address (with 0x prefix)             |

### Bot Configuration

| Variable          | Default | Description                                           |
| ----------------- | ------- | ----------------------------------------------------- |
| `DRY_RUN`         | `1`     | `1` = paper trading (no orders), `0` = live execution |
| `RUST_LOG`        | `info`  | Log level: `error`, `warn`, `info`, `debug`, `trace`  |
| `FORCE_DISCOVERY` | `0`     | `1` = re-fetch market mappings (ignore cache)         |
| `PRICE_LOGGING`   | `0`     | `1` = verbose price update logging                    |
| `POLY_MARKET_SLUGS` | (none) | Comma-separated Polymarket market slugs to discover (e.g., `epl-che-avl-2025-12-08,epl-mci-liv-2025-12-09`) |

### Test Mode

| Variable        | Default     | Description                                                                                    |
| --------------- | ----------- | ---------------------------------------------------------------------------------------------- |
| `TEST_ARB`      | `0`         | `1` = inject synthetic arb opportunity for testing                                             |
| `TEST_ARB_TYPE` | `poly_only` | Arb type: `poly_only` (Polymarket YES + NO)                                                    |

### Circuit Breaker

| Variable                     | Default | Description                                 |
| ---------------------------- | ------- | ------------------------------------------- |
| `CB_ENABLED`                 | `true`  | Enable/disable circuit breaker              |
| `CB_MAX_POSITION_PER_MARKET` | `100`   | Max contracts per market                    |
| `CB_MAX_TOTAL_POSITION`      | `500`   | Max total contracts across all markets      |
| `CB_MAX_DAILY_LOSS`          | `5000`  | Max daily loss in cents before halt         |
| `CB_MAX_CONSECUTIVE_ERRORS`  | `5`     | Consecutive errors before halt              |
| `CB_COOLDOWN_SECS`           | `60`    | Cooldown period after circuit breaker trips |

---

## Obtaining Credentials

### Polymarket

1. Create or import an Ethereum wallet (MetaMask, etc.)
2. Export the private key (include `0x` prefix)
3. Fund your wallet on Polygon network with USDC
4. The wallet address is your `POLY_FUNDER`

---

## Usage Examples

### Paper Trading (Development)

```bash
# Full logging, dry run
RUST_LOG=debug DRY_RUN=1 dotenvx run -- cargo run --release
```

### Test Arbitrage Execution

```bash
# Inject synthetic arb to test execution path
TEST_ARB=1 DRY_RUN=0 dotenvx run -- cargo run --release
```

### Production

```bash
# Live trading with circuit breaker
DRY_RUN=0 CB_MAX_DAILY_LOSS=10000 dotenvx run -- cargo run --release
```

### Force Market Re-Discovery

```bash
# Clear cache and re-fetch all market mappings
FORCE_DISCOVERY=1 dotenvx run -- cargo run --release
```

### Specify Markets to Discover

```bash
# Discover specific markets by providing slugs
POLY_MARKET_SLUGS='epl-che-avl-2025-12-08,epl-mci-liv-2025-12-09' dotenvx run -- cargo run --release
```

**Note**: Market slugs can be found on Polymarket's website. The format is typically: `{league}-{team1}-{team2}-{date}` (e.g., `epl-che-avl-2025-12-08`).

---

## How It Works

### Arbitrage Mechanics

In prediction markets, YES + NO = $1.00 guaranteed.

**Arbitrage exists when:**

```
Best YES ask (platform A) + Best NO ask (platform B) < $1.00
```

**Example:**

```
Kalshi YES ask:  42¢
Poly NO ask:     56¢
Total cost:      98¢
Guaranteed:     100¢
Profit:           2¢ per contract
```

### Arbitrage Type

| Type        | Buy                 | Description                    |
| ----------- | ------------------- | ------------------------------ |
| `poly_only` | Polymarket YES + NO | Same-platform arbitrage (no fees) |

### Fee Handling

- **Polymarket**: Zero trading fees

---

## Architecture

```
src/
├── main.rs              # Entry point, WebSocket orchestration
├── types.rs             # MarketArbState
├── execution.rs         # Concurrent leg execution, in-flight deduplication
├── position_tracker.rs  # Channel-based fill recording, P&L tracking
├── circuit_breaker.rs   # Risk limits, error tracking, auto-halt
├── discovery.rs         # Polymarket market discovery
├── cache.rs             # Team code mappings (EPL, NBA, etc.)
├── polymarket.rs        # Polymarket WS client
├── polymarket_clob.rs   # Polymarket CLOB order execution
└── config.rs            # League configs, thresholds
```

---

## Development

### Run Tests

```bash
cargo test
```

### Enable Profiling

```bash
cargo build --release --features profiling
```

### Benchmarks

```bash
cargo bench
```

---

## Project Status

- [x] Polymarket REST/WebSocket client
- [x] Lock-free orderbook cache
- [x] SIMD arb detection
- [x] Concurrent order execution
- [x] Position & P&L tracking
- [x] Circuit breaker
- [x] Market discovery & caching
- [ ] Risk limit configuration UI
- [ ] Multi-account support

# poly-kalshi-arb
