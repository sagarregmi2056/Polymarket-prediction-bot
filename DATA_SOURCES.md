# 📊 Data Sources & Dependencies

Complete guide to all external data sources, APIs, and dependencies required by the Polymarket Arbitrage Bot.

---

## 🌐 External APIs & Services

### 1. **Polymarket CLOB API** (Required)
**Purpose**: Order execution, authentication, market data

- **Base URL**: `https://clob.polymarket.com`
- **Endpoints Used**:
  - `GET /auth/derive-api-key` - Derive API credentials from wallet signature
  - `POST /order` - Place orders (FAK - Fill or Kill)
  - `GET /order/{order_id}` - Check order status
  - `GET /neg-risk?token_id={token_id}` - Get negative risk cache for tokens

**Authentication**:
- Requires Ethereum wallet private key (`POLY_PRIVATE_KEY`)
- Derives API key via signature-based authentication
- API key used for all subsequent requests

**Rate Limits**: Not publicly documented, but should be respected

**Required Credentials**:
```bash
POLY_PRIVATE_KEY=0x...  # Ethereum private key
POLY_FUNDER=0x...       # Wallet address
```

---

### 2. **Polymarket WebSocket** (Required)
**Purpose**: Real-time price updates for YES/NO tokens

- **WebSocket URL**: `wss://ws-subscriptions-clob.polymarket.com/ws/market`
- **Protocol**: WebSocket (WSS)
- **Message Types**:
  - `BookSnapshot` - Full orderbook snapshot
  - `PriceChangeEvent` - Incremental price updates

**Subscription Format**:
```json
{
  "assets_ids": ["0x...", "0x..."],
  "sub_type": "market"
}
```

**Keepalive**: Sends ping every 30 seconds (`POLY_PING_INTERVAL_SECS`)

**Reconnection**: Auto-reconnects after 5 seconds on disconnect

**No Authentication Required**: Public WebSocket endpoint

---

### 3. **Polymarket Gamma API** (Required)
**Purpose**: Market discovery - find active markets and their token addresses

- **Base URL**: `https://gamma-api.polymarket.com`
- **Endpoints Used**:
  - `GET /markets?slug={slug}` - Get market data by slug

**Example Request**:
```bash
GET https://gamma-api.polymarket.com/markets?slug=epl-che-avl-2025-12-08
```

**Response Format**:
```json
{
  "clobTokenIds": ["0x...", "0x..."],
  "question": "Will Chelsea beat Aston Villa?",
  ...
}
```

**Rate Limits**: 
- Max concurrent requests: 20 (`GAMMA_CONCURRENCY`)
- No authentication required (public API)

**Market Discovery Methods**:

1. **Via Environment Variable** (Recommended):
   ```bash
   POLY_MARKET_SLUGS=epl-che-avl-2025-12-08,epl-mci-liv-2025-12-09
   ```
   - Comma-separated list of Polymarket market slugs
   - Bot queries Gamma API for each slug
   - Extracts YES/NO token addresses

2. **Via League Discovery** (Future):
   - Currently placeholder implementation
   - Would search by league prefix (e.g., "epl", "nba")
   - Requires Polymarket GraphQL or frontend API access

---

## 💾 Local Data Storage

### Cache Files (Created Automatically)

1. **`.discovery_cache.json`**
   - **Purpose**: Cache discovered market pairs
   - **TTL**: 2 hours
   - **Format**: JSON with timestamp and market pairs
   - **Location**: Project root or `./data/` in Docker

2. **`positions.json`**
   - **Purpose**: Track open positions and P&L
   - **Updated**: After each fill
   - **Format**: JSON with position data
   - **Location**: Project root or `./data/` in Docker

3. **`.clob_market_cache.json`**
   - **Purpose**: Cache negative risk data from CLOB API
   - **Updated**: On startup and periodically
   - **Format**: JSON with token → neg_risk mappings
   - **Location**: Project root or `./data/` in Docker

---

## 🔐 Required Credentials

### Polymarket Wallet

**What You Need**:
1. **Ethereum Wallet** (MetaMask, Ledger, etc.)
   - Must be on Polygon network
   - Funded with USDC for trading

2. **Private Key** (`POLY_PRIVATE_KEY`)
   - Format: `0x` + 64 hex characters
   - Example: `0x1234567890abcdef...`
   - ⚠️ **Keep this secret!**

3. **Wallet Address** (`POLY_FUNDER`)
   - Format: `0x` + 40 hex characters
   - Example: `0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb`
   - Public address (safe to share)

**How to Get**:
1. Create/import wallet in MetaMask
2. Switch to Polygon network
3. Export private key (MetaMask → Account → Account Details → Export Private Key)
4. Copy wallet address from MetaMask

---

## 📡 Network Requirements

### Outbound Connections Required

1. **HTTPS** (Port 443)
   - `clob.polymarket.com` - Order execution
   - `gamma-api.polymarket.com` - Market discovery

2. **WSS** (Port 443)
   - `ws-subscriptions-clob.polymarket.com` - Price updates

3. **Ethereum RPC** (Port 443)
   - Polygon network RPC endpoint (for wallet operations)
   - Default: Uses ethers-rs default providers

### Firewall Rules

If running behind a firewall, allow:
- Outbound HTTPS (443) to `*.polymarket.com`
- Outbound WSS (443) to `ws-subscriptions-clob.polymarket.com`
- Outbound HTTPS (443) to Polygon RPC endpoints

---

## 🗄️ Database / Storage

**None Required**: The bot uses file-based storage:
- JSON files for caching and position tracking
- No database server needed
- Files persist in `./data/` directory (Docker) or project root (native)

---

## 🔄 Data Flow

### Startup Sequence

```
1. Load Credentials
   └─ Read POLY_PRIVATE_KEY and POLY_FUNDER from .env

2. Authenticate with CLOB API
   └─ GET /auth/derive-api-key
   └─ Returns API credentials

3. Discover Markets
   └─ Read POLY_MARKET_SLUGS from .env
   └─ For each slug: GET /markets?slug={slug}
   └─ Extract YES/NO token addresses
   └─ Cache to .discovery_cache.json

4. Connect to WebSocket
   └─ Connect to wss://ws-subscriptions-clob.polymarket.com
   └─ Subscribe to all discovered tokens
   └─ Start receiving price updates

5. Start Monitoring
   └─ Process price updates
   └─ Detect arbitrage opportunities
   └─ Execute orders via CLOB API
```

### Runtime Data Flow

```
Price Updates (WebSocket)
  └─> Update GlobalState
      └─> Check for arbitrage
          └─> If arb detected:
              └─> POST /order (CLOB API)
                  └─> Record fill
                      └─> Update positions.json
```

---

## 📋 Environment Variables Summary

### Required (Must Set)

| Variable | Source | Purpose |
|----------|--------|---------|
| `POLY_PRIVATE_KEY` | Your wallet | Authenticate with CLOB API |
| `POLY_FUNDER` | Your wallet | Wallet address for trading |
| `POLY_MARKET_SLUGS` | Polymarket website | Markets to monitor (comma-separated) |

### Optional (Have Defaults)

| Variable | Default | Purpose |
|----------|---------|---------|
| `DRY_RUN` | `1` | Paper trading mode |
| `RUST_LOG` | `info` | Logging level |
| `ARB_THRESHOLD` | `0.995` | Profit threshold |
| `FORCE_DISCOVERY` | `0` | Ignore cache |

---

## 🔍 Finding Market Slugs

### Method 1: From Polymarket Website

1. Go to [polymarket.com](https://polymarket.com)
2. Navigate to a market
3. Look at the URL: `polymarket.com/event/{slug}`
4. Example: `polymarket.com/event/epl-che-avl-2025-12-08`
   - Slug: `epl-che-avl-2025-12-08`

### Method 2: From Market Page

1. Open browser DevTools (F12)
2. Go to Network tab
3. Filter by "gamma-api"
4. Look for requests to `/markets?slug=...`
5. Copy the slug value

### Method 3: API Search (Future)

Currently not implemented, but could query:
- Polymarket GraphQL API
- Polymarket frontend API
- Third-party market aggregators

---

## 🚨 API Limitations & Considerations

### Polymarket CLOB API

- **Rate Limits**: Not publicly documented
- **Order Types**: Only FAK (Fill or Kill) supported
- **Minimum Order Size**: ~$1 worth of contracts
- **Slippage**: Prices may change between detection and execution

### Polymarket WebSocket

- **Connection Limits**: Unknown (likely per IP)
- **Reconnection**: Automatic, but may miss updates during disconnect
- **Message Rate**: High frequency (multiple updates per second per market)

### Gamma API

- **Rate Limits**: Unknown (20 concurrent requests max)
- **Search Limitations**: No direct search endpoint
- **Market Coverage**: May not have all markets indexed immediately

---

## 🔧 Troubleshooting Data Sources

### WebSocket Connection Issues

**Symptoms**: No price updates, connection errors

**Solutions**:
1. Check firewall allows WSS (port 443)
2. Verify `POLYMARKET_WS_URL` is correct
3. Check network connectivity: `curl -I https://polymarket.com`
4. Review logs for WebSocket errors

### CLOB API Authentication Failures

**Symptoms**: `derive-api-key` fails, 401 errors

**Solutions**:
1. Verify `POLY_PRIVATE_KEY` format (must start with `0x`)
2. Ensure wallet has funds on Polygon network
3. Check `POLY_FUNDER` matches private key
4. Verify network is Polygon (not Ethereum mainnet)

### Market Discovery Failures

**Symptoms**: No markets found, empty discovery cache

**Solutions**:
1. Verify `POLY_MARKET_SLUGS` is set correctly
2. Check slugs exist on Polymarket website
3. Try `FORCE_DISCOVERY=1` to bypass cache
4. Verify Gamma API is accessible: `curl https://gamma-api.polymarket.com/markets?slug=test`

### Missing Price Updates

**Symptoms**: Markets discovered but no prices

**Solutions**:
1. Verify WebSocket is connected (check logs)
2. Ensure tokens are subscribed (check subscription message)
3. Check if markets are active (may have closed)
4. Verify token addresses are correct (check discovery cache)

---

## 📚 Additional Resources

- **Polymarket Docs**: [docs.polymarket.com](https://docs.polymarket.com)
- **CLOB API Docs**: [clob.polymarket.com/docs](https://clob.polymarket.com/docs)
- **Polygon Network**: [polygon.technology](https://polygon.technology)

---

## ✅ Pre-Flight Checklist

Before running the bot, verify:

- [ ] `POLY_PRIVATE_KEY` is set and valid
- [ ] `POLY_FUNDER` matches private key
- [ ] Wallet has USDC on Polygon network
- [ ] `POLY_MARKET_SLUGS` contains valid market slugs
- [ ] Network allows outbound HTTPS/WSS connections
- [ ] Firewall rules allow connections to `*.polymarket.com`
- [ ] Docker (if using) has network access
- [ ] Data directory exists and is writable (`./data/`)

---

**Ready to run!** Once all data sources are accessible and credentials are set, the bot will automatically connect and start monitoring markets.

