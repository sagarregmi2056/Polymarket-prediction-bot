//! Polymarket Arbitrage Bot v2.0
//!
//! Strategy: BUY YES + BUY NO on Polymarket
//! Arb exists when: YES_ask + NO_ask < $1.00

mod cache;
mod circuit_breaker;
mod config;
mod discovery;
mod execution;
mod polymarket;
mod polymarket_clob;
mod position_tracker;
mod types;

use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
use config::{ARB_THRESHOLD, ENABLED_LEAGUES, WS_RECONNECT_DELAY_SECS};
use discovery::DiscoveryClient;
use execution::{ExecutionEngine, create_execution_channel, run_execution_loop};
use polymarket_clob::{PolymarketAsyncClient, PreparedCreds, SharedAsyncClient};
use position_tracker::{PositionTracker, create_position_channel, position_writer_loop};
use types::{GlobalState, PriceCents};

/// Polymarket CLOB API host
const POLY_CLOB_HOST: &str = "https://clob.polymarket.com";
/// Polygon chain ID
const POLYGON_CHAIN_ID: u64 = 137;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("arb_bot=info".parse().unwrap()),
        )
        .init();

    info!("🎯 Arb Bot v2.0");
    info!("   Threshold: <{:.1}¢ for {:.1}% profit",
          ARB_THRESHOLD * 100.0, (1.0 - ARB_THRESHOLD) * 100.0);
    info!("   Leagues: {:?}", ENABLED_LEAGUES);

    // Check for dry run mode
    let dry_run = std::env::var("DRY_RUN").map(|v| v == "1" || v == "true").unwrap_or(true);
    if dry_run {
        info!("   Mode: DRY RUN (set DRY_RUN=0 to execute)");
    } else {
        warn!("   Mode: LIVE EXECUTION");
    }

    // Load Polymarket credentials
    dotenvy::dotenv().ok();
    let poly_private_key = std::env::var("POLY_PRIVATE_KEY")
        .context("POLY_PRIVATE_KEY not set")?;
    let poly_funder = std::env::var("POLY_FUNDER")
        .context("POLY_FUNDER not set (your wallet address)")?;

    // Create async Polymarket client and derive API credentials
    info!("[POLYMARKET] Creating async client and deriving API credentials...");
    let poly_async_client = PolymarketAsyncClient::new(
        POLY_CLOB_HOST,
        POLYGON_CHAIN_ID,
        &poly_private_key,
        &poly_funder,
    )?;
    let api_creds = poly_async_client.derive_api_key(0).await?;
    let prepared_creds = PreparedCreds::from_api_creds(&api_creds)?;
    let poly_async = Arc::new(SharedAsyncClient::new(poly_async_client, prepared_creds, POLYGON_CHAIN_ID));

    // Load neg_risk cache from Python script output
    match poly_async.load_cache(".clob_market_cache.json") {
        Ok(count) => info!("[POLYMARKET] Loaded {} neg_risk entries from cache", count),
        Err(e) => warn!("[POLYMARKET] Could not load neg_risk cache: {}", e),
    }

    info!("[POLYMARKET] Client ready for {}", &poly_funder[..10]);

    // Run discovery (with caching support)
    let force_discovery = std::env::var("FORCE_DISCOVERY")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false);

    info!("🔍 Discovering markets{}...",
          if force_discovery { " (forced refresh)" } else { "" });

    let discovery = DiscoveryClient::new();

    let result = if force_discovery {
        discovery.discover_all_force(ENABLED_LEAGUES).await
    } else {
        discovery.discover_all(ENABLED_LEAGUES).await
    };

    info!("📊 Discovery complete:");
    info!("   - Market pairs found: {}", result.pairs.len());

    if !result.errors.is_empty() {
        for err in &result.errors {
            warn!("   ⚠️ {}", err);
        }
    }

    if result.pairs.is_empty() {
        error!("No market pairs found!");
        return Ok(());
    }

    // Print discovered pairs
    info!("📋 Matched markets:");
    for pair in &result.pairs {
        info!("   ✅ {} | {}",
              pair.description,
              pair.market_type);
    }

    // Build global state
    let state = Arc::new({
        let mut s = GlobalState::new();
        for pair in result.pairs {
            s.add_pair(pair);
        }
        info!("📡 State: Tracking {} markets", s.market_count());
        s
    });

    // Create execution infrastructure
    let (exec_tx, exec_rx) = create_execution_channel();
    let circuit_breaker = Arc::new(CircuitBreaker::new(CircuitBreakerConfig::from_env()));

    let position_tracker = Arc::new(RwLock::new(PositionTracker::new()));
    let (position_channel, position_rx) = create_position_channel();

    tokio::spawn(position_writer_loop(position_rx, position_tracker));

    let threshold_cents: PriceCents = ((ARB_THRESHOLD * 100.0).round() as u16).max(1);
    info!("   Threshold: {} cents", threshold_cents);

    let engine = Arc::new(ExecutionEngine::new(
        poly_async,
        state.clone(),
        circuit_breaker.clone(),
        position_channel,
        dry_run,
    ));

    let exec_handle = tokio::spawn(run_execution_loop(exec_rx, engine));

    // === TEST MODE: Inject fake arb after delay ===
    // TEST_ARB=1 to enable, TEST_ARB_TYPE=poly_yes_kalshi_no|kalshi_yes_poly_no|poly_only|kalshi_only
    let test_arb = std::env::var("TEST_ARB").map(|v| v == "1" || v == "true").unwrap_or(false);
    if test_arb {
        let test_state = state.clone();
        let test_exec_tx = exec_tx.clone();
        let test_dry_run = dry_run;

        // Parse arb type from environment (default: poly_yes_kalshi_no)
        let arb_type_str = std::env::var("TEST_ARB_TYPE").unwrap_or_else(|_| "poly_yes_kalshi_no".to_string());

        tokio::spawn(async move {
            use types::{FastExecutionRequest, ArbType};

            // Wait for WebSockets to connect and populate some prices
            info!("[TEST] Will inject fake arb in 10 seconds...");
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

            // Parse arb type (only PolyOnly supported now)
            let arb_type = ArbType::PolyOnly;

            // Set prices for realistic test scenario
            let (yes_price, no_price, description) = (48, 50, "P_yes=48¢ + P_no=50¢ + fee=0¢ = 98¢ → 2¢ profit (NO FEES!)");

            // Find first market with valid state
            let market_count = test_state.market_count();
            for market_id in 0..market_count {
                if let Some(market) = test_state.get_by_id(market_id as u16) {
                    if let Some(pair) = &market.pair {
                        // SIZE: 1000 cents = 10 contracts (Poly $1 min requires ~3 contracts at 40¢)
                        let fake_req = FastExecutionRequest {
                            market_id: market_id as u16,
                            yes_price,
                            no_price,
                            yes_size: 1000,  // 1000¢ = 10 contracts
                            no_size: 1000,   // 1000¢ = 10 contracts
                            arb_type,
                            detected_ns: 0,
                        };

                        warn!("[TEST] 🧪 Injecting FAKE {:?} arb for: {}", arb_type, pair.description);
                        warn!("[TEST]    {}", description);
                        warn!("[TEST]    SIZE CAPPED TO 10 CONTRACTS for safety!");
                        warn!("[TEST]    Execution mode: DRY_RUN={}", test_dry_run);

                        if let Err(e) = test_exec_tx.send(fake_req).await {
                            error!("[TEST] Failed to send fake arb: {}", e);
                        }
                        break;
                    }
                }
            }
        });
    }

    // Start Polymarket WebSocket
    let poly_state = state.clone();
    let poly_exec_tx = exec_tx.clone();
    let poly_threshold = threshold_cents;
    let poly_handle = tokio::spawn(async move {
        loop {
            if let Err(e) = polymarket::run_ws(poly_state.clone(), poly_exec_tx.clone(), poly_threshold).await {
                error!("[POLYMARKET] Disconnected: {} - reconnecting...", e);
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(WS_RECONNECT_DELAY_SECS)).await;
        }
    });

    // Heartbeat task with arb diagnostics
    let heartbeat_state = state.clone();
    let heartbeat_threshold = threshold_cents;
    let heartbeat_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let market_count = heartbeat_state.market_count();
            let mut with_poly = 0;
            let mut best_arb: Option<(u16, u16, u16, u16)> = None;

            for market in heartbeat_state.markets.iter().take(market_count) {
                let (p_yes, p_no, _, _) = market.poly.load();
                let has_p = p_yes > 0 && p_no > 0;
                if p_yes > 0 || p_no > 0 { with_poly += 1; }
                if has_p {
                    let cost = p_yes + p_no;
                    if best_arb.is_none() || cost < best_arb.as_ref().unwrap().0 {
                        best_arb = Some((cost, market.market_id, p_yes, p_no));
                    }
                }
            }

            info!("💓 Heartbeat | Markets: {} total, {} w/Poly | threshold={}¢",
                  market_count, with_poly, heartbeat_threshold);

            if let Some((cost, market_id, p_yes, p_no)) = best_arb {
                let gap = cost as i16 - heartbeat_threshold as i16;
                let desc = heartbeat_state.get_by_id(market_id)
                    .and_then(|m| m.pair.as_ref())
                    .map(|p| &*p.description)
                    .unwrap_or("Unknown");
                if gap <= 10 {
                    info!("   📊 Best: {} | P_yes({}¢) + P_no({}¢) = {}¢ | gap={:+}¢",
                          desc, p_yes, p_no, cost, gap);
                } else {
                    info!("   📊 Best: {} | P_yes({}¢) + P_no({}¢) = {}¢ | gap={:+}¢ - efficient",
                          desc, p_yes, p_no, cost, gap);
                }
            } else if with_poly == 0 {
                warn!("   ⚠️  No markets with Poly prices - check WebSocket connection");
            }
        }
    });

    // Run forever
    let _ = tokio::join!(poly_handle, heartbeat_handle, exec_handle);

    Ok(())
}
