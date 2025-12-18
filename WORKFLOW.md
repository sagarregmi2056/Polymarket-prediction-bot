# 🔄 Complete Workflow: How the Polymarket Arbitrage Bot Works

Complete end-to-end explanation of how the Polymarket arbitrage bot operates, from startup to order execution.

---

## 📋 Table of Contents

1. [Overview](#overview)
2. [Startup Phase](#startup-phase)
3. [Market Discovery Phase](#market-discovery-phase)
4. [Price Tracking Phase](#price-tracking-phase)
5. [Arbitrage Detection Phase](#arbitrage-detection-phase)
6. [Order Execution Phase](#order-execution-phase)
7. [Complete Flow Diagram](#complete-flow-diagram)
8. [Data Structures](#data-structures)
9. [Example Scenario](#example-scenario)

---

## 🎯 Overview

The bot performs **same-platform arbitrage** on Polymarket by:
1. **Discovering** active markets
2. **Tracking** YES and NO prices via WebSocket
3. **Detecting** when YES + NO < $1.00 (arbitrage opportunity)
4. **Executing** simultaneous YES and NO orders
5. **Profiting** from the guaranteed $1.00 payout

**Key Insight**: In prediction markets, YES + NO always equals $1.00. If you can buy both for less than $1.00, you profit!

---

## 🚀 Startup Phase

### Step 1: Initialization (`main.rs`)

```
┌─────────────────────────────────────────┐
│         STARTUP SEQUENCE               │
└─────────────────────────────────────────┘

1. Load Configuration
   ├─ Read environment variables
   ├─ Set up logging
   └─ Check DRY_RUN mode

2. Load Credentials
   ├─ POLY_PRIVATE_KEY (Ethereum wallet)
   └─ POLY_FUNDER (wallet address)

3. Create Polymarket Client
   ├─ Initialize CLOB client
   ├─ Derive API credentials
   └─ Load neg_risk cache

4. Initialize Components
   ├─ GlobalState (market tracking)
   ├─ ExecutionEngine
   ├─ CircuitBreaker
   └─ PositionTracker
```

**Code Flow**:
```rust
// main.rs
async fn main() -> Result<()> {
    // 1. Setup logging
    tracing_subscriber::fmt().init();
    
    // 2. Load credentials
    let poly_private_key = std::env::var("POLY_PRIVATE_KEY")?;
    let poly_funder = std::env::var("POLY_FUNDER")?;
    
    // 3. Create Polymarket client
    let poly_async_client = PolymarketAsyncClient::new(...)?;
    let api_creds = poly_async_client.derive_api_key(0).await?;
    let poly_async = Arc::new(SharedAsyncClient::new(...));
    
    // 4. Initialize state
    let state = Arc::new(GlobalState::new());
    let engine = Arc::new(ExecutionEngine::new(...));
    // ...
}
```

---

## 🔍 Market Discovery Phase

### Step 2: Discover Markets (`discovery.rs`)

```
┌─────────────────────────────────────────┐
│      MARKET DISCOVERY FLOW              │
└─────────────────────────────────────────┘

1. Check Cache
   ├─ Load .discovery_cache.json
   ├─ Check if cache is fresh (<2 hours)
   └─ If fresh → use cached markets

2. Discover Markets
   ├─ Read POLY_MARKET_SLUGS env var
   ├─ Split by comma: ["slug1", "slug2", ...]
   └─ For each slug:
       ├─ Query Gamma API: GET /markets?slug={slug}
       ├─ Extract YES/NO tokens
       ├─ Get market description
       └─ Create MarketPair object

3. Build GlobalState
   ├─ Add each MarketPair to state
   ├─ Create hash maps for fast lookup:
       ├─ poly_yes_to_id (YES token → market_id)
       └─ poly_no_to_id (NO token → market_id)
   └─ Store in markets array

4. Save Cache
   └─ Write discovered pairs to .discovery_cache.json
```

**Code Flow**:
```rust
// discovery.rs
async fn discover_league(&self, config: &LeagueConfig, cache: Option<&DiscoveryCache>) -> DiscoveryResult {
    // 1. Read POLY_MARKET_SLUGS
    let market_slugs_env = std::env::var("POLY_MARKET_SLUGS")?;
    let slugs: Vec<&str> = market_slugs_env.split(',').collect();
    
    // 2. For each slug, lookup market
    for slug in slugs {
        match self.gamma.lookup_market(slug).await {
            Ok(Some((yes_token, no_token, description))) => {
                // 3. Create MarketPair
                let pair = MarketPair {
                    poly_slug: slug.into(),
                    poly_yes_token: yes_token.into(),
                    poly_no_token: no_token.into(),
                    description: description.into(),
                    // ...
                };
                result.pairs.push(pair);
            }
            // ...
        }
    }
}

// main.rs
let discovery = DiscoveryClient::new();
let result = discovery.discover_all(ENABLED_LEAGUES).await;

// Build GlobalState
let mut state = GlobalState::new();
for pair in result.pairs {
    state.add_pair(pair);  // Creates market_id, stores in markets array
}
```

**Example**:
```
POLY_MARKET_SLUGS='epl-che-avl-2025-12-08'

Discovery Process:
1. Query: GET https://gamma-api.polymarket.com/markets?slug=epl-che-avl-2025-12-08
2. Response: {
     "clobTokenIds": "[\"0x123...\", \"0x456...\"]",
     "question": "Will Chelsea beat Aston Villa?"
   }
3. Create MarketPair:
   - poly_yes_token: "0x123..."
   - poly_no_token: "0x456..."
   - description: "Will Chelsea beat Aston Villa?"
   - market_id: 0 (first market)
```

---

## 📊 Price Tracking Phase

### Step 3: WebSocket Connection (`polymarket.rs`)

```
┌─────────────────────────────────────────┐
│      PRICE TRACKING FLOW                │
└─────────────────────────────────────────┘

1. Connect to WebSocket
   └─ wss://ws-subscriptions-clob.polymarket.com/ws/market

2. Subscribe to Tokens
   ├─ Collect all YES tokens from state
   ├─ Collect all NO tokens from state
   └─ Send subscription message:
       {
         "assets_ids": ["0x123...", "0x456...", ...],
         "sub_type": "market"
       }

3. Receive Price Updates
   ├─ Book Snapshot (full orderbook)
   │   └─ Extract best ASK price
   └─ Price Change Event (incremental)
       └─ Update price if ASK side

4. Update GlobalState
   ├─ Find market by token hash
   ├─ Update AtomicOrderbook:
       ├─ YES side: update_yes(price, size)
       └─ NO side: update_no(price, size)
   └─ Check for arbitrage immediately
```

**Code Flow**:
```rust
// polymarket.rs
pub async fn run_ws(
    state: Arc<GlobalState>,
    exec_tx: mpsc::Sender<FastExecutionRequest>,
    threshold_cents: PriceCents,
) -> Result<()> {
    // 1. Collect all tokens
    let tokens: Vec<String> = state.markets.iter()
        .flat_map(|m| m.pair.as_ref())
        .flat_map(|p| [p.poly_yes_token.to_string(), p.poly_no_token.to_string()])
        .collect();
    
    // 2. Connect and subscribe
    let (mut write, mut read) = connect_async(POLYMARKET_WS_URL).await?.split();
    write.send(Message::Text(subscribe_msg)).await?;
    
    // 3. Process messages
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                // Parse book snapshot or price change
                if let Ok(books) = serde_json::from_str::<Vec<BookSnapshot>>(&text) {
                    for book in &books {
                        process_book(&state, book, &exec_tx, threshold_cents, &clock).await;
                    }
                }
            }
            // ...
        }
    }
}

async fn process_book(
    state: &GlobalState,
    book: &BookSnapshot,
    exec_tx: &mpsc::Sender<FastExecutionRequest>,
    threshold_cents: PriceCents,
    clock: &NanoClock,
) {
    // Find best ASK (lowest price to buy)
    let (best_ask, ask_size) = book.asks.iter()
        .map(|l| (parse_price(&l.price), parse_size(&l.size)))
        .min_by_key(|(p, _)| *p)
        .unwrap_or((0, 0));
    
    // Find market by token
    let token_hash = fxhash_str(&book.asset_id);
    
    if let Some(&market_id) = state.poly_yes_to_id.get(&token_hash) {
        // Update YES price
        state.markets[market_id as usize].poly.update_yes(best_ask, ask_size);
        
        // Check for arbitrage
        let arb_mask = state.markets[market_id as usize].check_arbs(threshold_cents);
        if arb_mask != 0 {
            send_arb_request(market_id, market, arb_mask, exec_tx, clock).await;
        }
    }
    // Similar for NO token...
}
```

**Example Price Update**:
```
WebSocket Message Received:
{
  "asset_id": "0x123...",
  "asks": [
    {"price": "0.45", "size": "100.0"},
    {"price": "0.46", "size": "50.0"}
  ]
}

Processing:
1. Extract best ASK: price=0.45 (45¢), size=100.0
2. Find market_id by token hash: market_id=0
3. Update: market[0].poly.update_yes(45, 10000)  // 45¢, $100
4. Check arbs: YES=45¢, NO=50¢ → Total=95¢ < 100¢ threshold → ARB DETECTED!
```

---

## 🎯 Arbitrage Detection Phase

### Step 4: Detect Arbitrage (`types.rs`)

```
┌─────────────────────────────────────────┐
│      ARBITRAGE DETECTION                │
└─────────────────────────────────────────┘

1. Price Update Triggers Check
   └─ After updating YES or NO price

2. Calculate Total Cost
   ├─ YES price: 45¢
   ├─ NO price: 50¢
   ├─ Total: 45 + 50 = 95¢
   └─ Fees: 0¢ (Polymarket has no fees!)

3. Compare to Threshold
   ├─ Threshold: 99.5¢ (0.5% profit minimum)
   ├─ Cost: 95¢
   └─ Profit: 100¢ - 95¢ = 5¢ per contract

4. Check Liquidity
   ├─ YES size: $100 available
   ├─ NO size: $100 available
   └─ Can execute: min(100, 100) = $100 worth

5. Create Execution Request
   └─ FastExecutionRequest {
        market_id: 0,
        yes_price: 45,
        no_price: 50,
        yes_size: 10000,  // $100 in cents
        no_size: 10000,
        arb_type: PolyOnly,
        detected_ns: timestamp
      }
```

**Code Flow**:
```rust
// types.rs
impl AtomicMarketState {
    pub fn check_arbs(&self, threshold_cents: PriceCents) -> u8 {
        let (p_yes, p_no, _, _) = self.poly.load();
        
        if p_yes == NO_PRICE || p_no == NO_PRICE {
            return 0;  // Missing prices
        }
        
        // Calculate total cost (no fees on Polymarket)
        let cost = (p_yes + p_no) as i16;
        
        // Check if below threshold (profitable)
        if cost < threshold_cents as i16 {
            return 4;  // Bit 2 = PolyOnly arb detected
        }
        
        0  // No arb
    }
}

// polymarket.rs
async fn send_arb_request(
    market_id: u16,
    market: &AtomicMarketState,
    arb_mask: u8,
    exec_tx: &mpsc::Sender<FastExecutionRequest>,
    clock: &NanoClock,
) {
    if arb_mask & 4 != 0 {  // PolyOnly arb
        let (p_yes, p_no, yes_size, no_size) = market.poly.load();
        
        let req = FastExecutionRequest {
            market_id,
            yes_price: p_yes,
            no_price: p_no,
            yes_size,
            no_size,
            arb_type: ArbType::PolyOnly,
            detected_ns: clock.now_ns(),
        };
        
        exec_tx.send(req).await.ok();
    }
}
```

**Example Detection**:
```
Current Prices:
- YES ask: 45¢
- NO ask: 50¢
- Total: 95¢

Threshold: 99.5¢ (0.5% profit minimum)

Calculation:
- Cost: 95¢
- Guaranteed payout: 100¢
- Profit: 5¢ per contract (5% return!)
- Status: ✅ ARB DETECTED (95 < 99.5)

Execution Request Created:
{
  market_id: 0,
  yes_price: 45,
  no_price: 50,
  yes_size: 10000,  // $100
  no_size: 10000,   // $100
  arb_type: PolyOnly
}
```

---

## ⚡ Order Execution Phase

### Step 5: Execute Orders (`execution.rs`)

```
┌─────────────────────────────────────────┐
│      ORDER EXECUTION FLOW                │
└─────────────────────────────────────────┘

1. Receive Execution Request
   └─ From execution channel

2. Deduplication Check
   ├─ Check in_flight bitmask
   ├─ If already in-flight → skip
   └─ Mark as in-flight

3. Validation
   ├─ Check profit still exists
   ├─ Check liquidity available
   ├─ Check circuit breaker
   └─ Calculate max contracts

4. Execute Both Legs Concurrently
   ├─ YES order: buy_fak(poly_yes_token, 0.45, contracts)
   └─ NO order: buy_fak(poly_no_token, 0.50, contracts)
   
   Both execute simultaneously (tokio::join!)

5. Process Results
   ├─ YES filled: 10 contracts @ 0.45 = $4.50
   ├─ NO filled: 10 contracts @ 0.50 = $5.00
   ├─ Total cost: $9.50
   ├─ Guaranteed payout: $10.00 (10 contracts × $1)
   └─ Profit: $0.50

6. Handle Mismatches
   └─ If YES ≠ NO filled:
       ├─ Auto-close excess position
       └─ Log P&L

7. Record Position
   └─ Update PositionTracker with fills
```

**Code Flow**:
```rust
// execution.rs
impl ExecutionEngine {
    pub async fn process(&self, req: FastExecutionRequest) -> Result<ExecutionResult> {
        // 1. Deduplication
        if self.is_in_flight(req.market_id) {
            return Ok(ExecutionResult { success: false, error: Some("Already in-flight") });
        }
        self.mark_in_flight(req.market_id);
        
        // 2. Validation
        let profit_cents = req.profit_cents();
        if profit_cents < 1 {
            return Ok(ExecutionResult { success: false, error: Some("Profit below threshold") });
        }
        
        let max_contracts = (req.yes_size.min(req.no_size) / 100) as i64;
        
        // 3. Circuit breaker check
        if let Err(_) = self.circuit_breaker.can_execute(&pair.pair_id, max_contracts).await {
            return Ok(ExecutionResult { success: false, error: Some("Circuit breaker") });
        }
        
        // 4. Execute both legs concurrently
        let result = self.execute_both_legs_async(&req, pair, max_contracts).await;
        
        match result {
            Ok((yes_filled, no_filled, yes_cost, no_cost, yes_order_id, no_order_id)) => {
                let matched = yes_filled.min(no_filled);
                let actual_profit = matched as i16 * 100 - (yes_cost + no_cost) as i16;
                
                // 5. Handle mismatches
                if yes_filled != no_filled {
                    self.auto_close_background(...).await;
                }
                
                // 6. Record position
                self.position_channel.record_fill(...);
                
                Ok(ExecutionResult {
                    success: matched > 0,
                    profit_cents: actual_profit,
                    // ...
                })
            }
            // ...
        }
    }
    
    async fn execute_both_legs_async(
        &self,
        req: &FastExecutionRequest,
        pair: &MarketPair,
        contracts: i64,
    ) -> Result<(i64, i64, i64, i64, String, String)> {
        // Execute YES and NO orders simultaneously
        let yes_fut = self.poly_async.buy_fak(
            &pair.poly_yes_token,
            cents_to_price(req.yes_price),  // 0.45
            contracts as f64,                // 10.0
        );
        let no_fut = self.poly_async.buy_fak(
            &pair.poly_no_token,
            cents_to_price(req.no_price),    // 0.50
            contracts as f64,                // 10.0
        );
        
        // Wait for both to complete
        let (yes_res, no_res) = tokio::join!(yes_fut, no_fut);
        
        // Extract results
        self.extract_poly_only_results(yes_res, no_res)
    }
}
```

**Example Execution**:
```
Execution Request:
- YES: Buy 10 contracts @ 45¢ = $4.50
- NO: Buy 10 contracts @ 50¢ = $5.00
- Total: $9.50

Execution:
1. Send YES order: buy_fak("0x123...", 0.45, 10.0)
2. Send NO order: buy_fak("0x456...", 0.50, 10.0)
3. Both execute concurrently

Results:
- YES filled: 10 contracts @ 0.45 = $4.50 ✅
- NO filled: 10 contracts @ 0.50 = $5.00 ✅
- Matched: 10 contracts

Profit Calculation:
- Cost: $4.50 + $5.00 = $9.50
- Payout: 10 × $1.00 = $10.00
- Profit: $0.50 (5.26% return)

Position Recorded:
- YES: 10 contracts @ $0.45
- NO: 10 contracts @ $0.50
```

---

## 🔄 Complete Flow Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                    COMPLETE BOT WORKFLOW                        │
└─────────────────────────────────────────────────────────────────┘

STARTUP
   │
   ├─► Load Config & Credentials
   ├─► Create Polymarket Client
   ├─► Initialize GlobalState
   └─► Initialize ExecutionEngine
        │
        ▼
DISCOVERY
   │
   ├─► Check Cache (.discovery_cache.json)
   │   ├─► If fresh → use cached markets
   │   └─► If stale/empty → discover new
   │
   ├─► Read POLY_MARKET_SLUGS
   │   └─► Split: ["slug1", "slug2", ...]
   │
   ├─► For each slug:
   │   ├─► Query Gamma API: GET /markets?slug={slug}
   │   ├─► Extract YES/NO tokens
   │   └─► Create MarketPair
   │
   └─► Build GlobalState
       ├─► Add pairs to markets array
       ├─► Create hash maps (token → market_id)
       └─► Save cache
        │
        ▼
WEBSOCKET SETUP
   │
   ├─► Collect all tokens (YES + NO)
   ├─► Connect: wss://ws-subscriptions-clob.polymarket.com
   ├─► Subscribe to all tokens
   └─► Start listening loop
        │
        ▼
PRICE TRACKING (Continuous Loop)
   │
   ├─► Receive WebSocket Message
   │   ├─► Book Snapshot → full orderbook
   │   └─► Price Change → incremental update
   │
   ├─► Extract Best ASK Price
   │   └─► Lowest price in asks array
   │
   ├─► Find Market by Token Hash
   │   ├─► poly_yes_to_id.get(hash) → market_id
   │   └─► poly_no_to_id.get(hash) → market_id
   │
   ├─► Update AtomicOrderbook
   │   ├─► market.poly.update_yes(price, size)
   │   └─► market.poly.update_no(price, size)
   │
   └─► Check for Arbitrage
       ├─► market.check_arbs(threshold)
       ├─► Calculate: YES + NO < threshold?
       └─► If YES → send to execution channel
            │
            ▼
ARBITRAGE DETECTION
   │
   ├─► Calculate Total Cost
   │   ├─► YES price: 45¢
   │   ├─► NO price: 50¢
   │   └─► Total: 95¢ (no fees!)
   │
   ├─► Compare to Threshold
   │   ├─► Threshold: 99.5¢
   │   └─► 95¢ < 99.5¢ → ARB!
   │
   ├─► Check Liquidity
   │   └─► min(yes_size, no_size) = available contracts
   │
   └─► Create ExecutionRequest
       └─► Send to execution channel
            │
            ▼
ORDER EXECUTION
   │
   ├─► Receive Request from Channel
   │
   ├─► Deduplication Check
   │   └─► Skip if already in-flight
   │
   ├─► Validation
   │   ├─► Profit still exists?
   │   ├─► Liquidity available?
   │   └─► Circuit breaker OK?
   │
   ├─► Execute Both Legs Concurrently
   │   ├─► YES: buy_fak(token, 0.45, contracts)
   │   └─► NO: buy_fak(token, 0.50, contracts)
   │   └─► tokio::join!(yes_fut, no_fut)
   │
   ├─► Process Results
   │   ├─► Extract fills: (yes_filled, no_filled)
   │   ├─► Calculate matched contracts
   │   └─► Calculate actual profit
   │
   ├─► Handle Mismatches
   │   └─► If yes_filled ≠ no_filled:
   │       └─► Auto-close excess position
   │
   └─► Record Position
       └─► Update PositionTracker
            │
            ▼
CONTINUOUS MONITORING
   │
   └─► Loop Forever
       ├─► Price updates → Check arbs
       ├─► Execute profitable opportunities
       └─► Heartbeat every 60s (status logging)
```

---

## 📊 Data Structures

### GlobalState

```rust
pub struct GlobalState {
    pub markets: Vec<AtomicMarketState>,      // All tracked markets
    next_market_id: u16,                      // Next available ID
    pub poly_yes_to_id: FxHashMap<u64, u16>,  // YES token hash → market_id
    pub poly_no_to_id: FxHashMap<u64, u16>,   // NO token hash → market_id
}
```

**Purpose**: Central state tracking all markets and fast lookups.

### AtomicMarketState

```rust
pub struct AtomicMarketState {
    pub poly: AtomicOrderbook,                // YES/NO prices and sizes
    pub pair: Option<Arc<MarketPair>>,         // Market metadata
    pub market_id: u16,                        // Unique ID
}
```

**Purpose**: Per-market state with lock-free price updates.

### AtomicOrderbook

```rust
pub struct AtomicOrderbook {
    packed: AtomicU64,  // [yes_ask:16][no_ask:16][yes_size:16][no_size:16]
}
```

**Purpose**: Lock-free orderbook storage (64-bit packed).

### MarketPair

```rust
pub struct MarketPair {
    pub pair_id: Arc<str>,                    // Unique identifier
    pub league: Arc<str>,                     // League code
    pub market_type: MarketType,               // Moneyline/Spread/Total/Btts
    pub description: Arc<str>,                // Market question
    pub poly_slug: Arc<str>,                  // Polymarket slug
    pub poly_yes_token: Arc<str>,              // YES token address
    pub poly_no_token: Arc<str>,               // NO token address
    pub line_value: Option<f64>,              // Spread/total line
    pub team_suffix: Option<Arc<str>>,        // Team suffix (for moneyline)
}
```

**Purpose**: Immutable market metadata.

---

## 💡 Example Scenario

### Complete Example: EPL Match

**Setup**:
```bash
POLY_MARKET_SLUGS='epl-che-avl-2025-12-08'
DRY_RUN=0
```

**Step-by-Step**:

1. **Discovery**:
   ```
   Query: GET /markets?slug=epl-che-avl-2025-12-08
   Response: {
     "clobTokenIds": ["0xABC...", "0xDEF..."],
     "question": "Will Chelsea beat Aston Villa?"
   }
   
   MarketPair Created:
   - market_id: 0
   - poly_yes_token: "0xABC..."
   - poly_no_token: "0xDEF..."
   - description: "Will Chelsea beat Aston Villa?"
   ```

2. **WebSocket Subscription**:
   ```
   Subscribe to: ["0xABC...", "0xDEF..."]
   ```

3. **Price Update (YES)**:
   ```
   Message: {
     "asset_id": "0xABC...",
     "asks": [{"price": "0.45", "size": "100.0"}]
   }
   
   Update:
   - market[0].poly.update_yes(45, 10000)
   - Current state: YES=45¢, NO=0¢ (waiting for NO price)
   ```

4. **Price Update (NO)**:
   ```
   Message: {
     "asset_id": "0xDEF...",
     "asks": [{"price": "0.50", "size": "100.0"}]
   }
   
   Update:
   - market[0].poly.update_no(50, 10000)
   - Current state: YES=45¢, NO=50¢
   - Check arbs: 45 + 50 = 95¢ < 99.5¢ → ARB DETECTED!
   ```

5. **Execution Request**:
   ```
   FastExecutionRequest {
     market_id: 0,
     yes_price: 45,
     no_price: 50,
     yes_size: 10000,  // $100
     no_size: 10000,   // $100
     arb_type: PolyOnly,
     detected_ns: 1234567890
   }
   ```

6. **Order Execution**:
   ```
   Concurrent Execution:
   ├─ YES Order: buy_fak("0xABC...", 0.45, 10.0)
   └─ NO Order: buy_fak("0xDEF...", 0.50, 10.0)
   
   Results:
   ├─ YES: 10 contracts filled @ $0.45 = $4.50
   └─ NO: 10 contracts filled @ $0.50 = $5.00
   
   Profit:
   ├─ Total Cost: $9.50
   ├─ Guaranteed Payout: $10.00
   └─ Profit: $0.50 (5.26% return)
   ```

7. **Position Recording**:
   ```
   PositionTracker Updated:
   ├─ YES: 10 contracts @ $0.45
   └─ NO: 10 contracts @ $0.50
   
   Status: Matched position, ready to settle
   ```

---

## 🔧 Key Components

### 1. GlobalState
- **Purpose**: Central repository for all market data
- **Key Feature**: O(1) lookup by token hash
- **Thread Safety**: Lock-free via atomics

### 2. ExecutionEngine
- **Purpose**: Handles order execution
- **Key Feature**: Concurrent leg execution
- **Safety**: Deduplication, circuit breaker, position limits

### 3. CircuitBreaker
- **Purpose**: Risk management
- **Features**: 
  - Max position per market
  - Max total position
  - Max daily loss
  - Consecutive error tracking

### 4. PositionTracker
- **Purpose**: Track fills and P&L
- **Features**: Channel-based recording, async processing

### 5. WebSocket Handler
- **Purpose**: Real-time price updates
- **Features**: Auto-reconnect, ping/pong, incremental updates

---

## ⚙️ Configuration

### Environment Variables

```bash
# Required
POLY_PRIVATE_KEY=0x...          # Ethereum private key
POLY_FUNDER=0x...               # Wallet address
POLY_MARKET_SLUGS='slug1,slug2' # Markets to track

# Optional
DRY_RUN=1                        # Paper trading mode
RUST_LOG=info                    # Logging level
ARB_THRESHOLD=0.995              # Profit threshold (99.5¢)
FORCE_DISCOVERY=0                # Ignore cache
```

### Threshold Calculation

```
Threshold: 99.5¢ means:
- Minimum profit: 0.5¢ per contract
- Minimum return: 0.5%

Example:
- YES: 49.5¢
- NO: 50.0¢
- Total: 99.5¢
- Profit: 0.5¢ (0.5% return)
- Status: ✅ Executable
```

---

## 🎯 Performance Characteristics

### Latency Breakdown

```
Price Update → Arb Detection: <1ms (atomic operations)
Arb Detection → Execution: <5ms (channel send)
Execution → Order Sent: <10ms (API call)
Total Latency: ~15-20ms
```

### Throughput

```
- Markets Tracked: Up to 1024
- Price Updates: Real-time (WebSocket)
- Execution Rate: Limited by API rate limits
- Concurrent Orders: 2 per arb (YES + NO)
```

---

## 🔍 Monitoring

### Heartbeat (Every 60s)

```
💓 Heartbeat | Markets: 10 total, 10 w/Poly | threshold=99¢
   📊 Best: Will Chelsea beat Aston Villa? | P_yes(45¢) + P_no(50¢) = 95¢ | gap=-4¢
```

### Execution Logs

```
[EXEC] 🎯 Will Chelsea beat Aston Villa? | PolyOnly y=45¢ n=50¢ | profit=5¢ | 10x | 15µs
[EXEC] ✅ market_id=0 profit=5¢ latency=15µs
```

---

## 🚨 Error Handling

### Common Scenarios

1. **WebSocket Disconnect**
   - Auto-reconnect after 5s delay
   - Resubscribe to all tokens

2. **Order Failure**
   - Log error
   - Update circuit breaker
   - Continue monitoring

3. **Mismatched Fills**
   - Auto-close excess position
   - Log P&L
   - Continue monitoring

4. **Circuit Breaker Trip**
   - Halt execution
   - Log reason
   - Wait for cooldown

---

## 📈 Profit Calculation

### Formula

```
Profit per Contract = $1.00 - (YES_price + NO_price + fees)

For Polymarket:
- Fees = $0.00
- Profit = $1.00 - (YES_price + NO_price)

Example:
- YES: 45¢
- NO: 50¢
- Profit: 100¢ - (45¢ + 50¢) = 5¢ per contract
- Return: 5¢ / 95¢ = 5.26%
```

### Position Sizing

```
Max Contracts = min(
    YES_size_available / YES_price,
    NO_size_available / NO_price
)

Example:
- YES: 45¢, $100 available → 222 contracts max
- NO: 50¢, $100 available → 200 contracts max
- Execute: min(222, 200) = 200 contracts
```

---

## 🎓 Summary

The bot operates in **4 main phases**:

1. **Discovery**: Find markets via `POLY_MARKET_SLUGS` or cache
2. **Tracking**: Monitor prices via WebSocket in real-time
3. **Detection**: Calculate YES + NO < threshold → arbitrage!
4. **Execution**: Buy both sides simultaneously → profit!

**Key Insight**: The bot exploits the mathematical guarantee that YES + NO = $1.00. If you can buy both for less, you profit!

---

**Ready to run!** Set `POLY_MARKET_SLUGS` and start the bot! 🚀
