// tests/integration_tests.rs
// Holistic integration tests for the Polymarket arbitrage bot
//
// These tests verify the full flow:
// 1. Arb detection (Polymarket-only, no fees)
// 2. Position tracking after fills
// 3. Circuit breaker behavior
// 4. End-to-end scenarios

// ============================================================================
// POSITION TRACKER TESTS - Verify fill recording and P&L calculation
// ============================================================================

mod position_tracker_tests {
    use arb_bot::position_tracker::*;
    
    /// Test: Recording fills updates position correctly
    #[test]
    fn test_record_fills_updates_position() {
        let mut tracker = PositionTracker::new();
        
        // Record a Polymarket YES fill
        tracker.record_fill(&FillRecord::new(
            "TEST-MARKET",
            "Test Market",
            "polymarket",
            "yes",
            10.0,   // 10 contracts
            0.45,   // at 45¢
            0.0,    // no fees
            "order123",
        ));
        
        // Record a Polymarket NO fill
        tracker.record_fill(&FillRecord::new(
            "TEST-MARKET",
            "Test Market",
            "polymarket",
            "no",
            10.0,   // 10 contracts
            0.50,   // at 50¢
            0.0,    // no fees
            "order456",
        ));
        
        let summary = tracker.summary();
        
        assert_eq!(summary.open_positions, 1, "Should have 1 open position");
        assert!(summary.total_contracts > 0.0, "Should have contracts");
        
        // Cost basis: 10 * 0.45 + 10 * 0.50 = $9.50 (no fees!)
        assert!(summary.total_cost_basis > 9.0, "Cost basis should be > $9");
    }
    
    /// Test: Matched arb calculates guaranteed profit (PolyOnly)
    #[test]
    fn test_matched_arb_guaranteed_profit() {
        let mut pos = ArbPosition::new("TEST-MARKET", "Test");
        
        // Buy 10 YES on Poly at 45¢
        pos.poly_yes.add(10.0, 0.45);
        
        // Buy 10 NO on Poly at 50¢
        pos.poly_no.add(10.0, 0.50);
        
        // No fees on Polymarket
        pos.total_fees = 0.0;
        
        // Total cost: $4.50 + $5.00 = $9.50
        // Guaranteed payout: $10.00
        // Guaranteed profit: $0.50
        
        assert!((pos.total_cost() - 9.50).abs() < 0.01, "Cost should be $9.50");
        assert!((pos.matched_contracts() - 10.0).abs() < 0.01, "Should have 10 matched");
        assert!(pos.guaranteed_profit() > 0.0, "Should have positive guaranteed profit");
        assert!((pos.guaranteed_profit() - 0.50).abs() < 0.01, "Profit should be ~$0.50");
    }
    
    /// Test: Partial fills create exposure
    #[test]
    fn test_partial_fill_creates_exposure() {
        let mut pos = ArbPosition::new("TEST-MARKET", "Test");
        
        // Full fill on Poly YES
        pos.poly_yes.add(10.0, 0.45);
        
        // Partial fill on Poly NO (only 7 contracts)
        pos.poly_no.add(7.0, 0.50);
        
        assert!((pos.matched_contracts() - 7.0).abs() < 0.01, "Should have 7 matched");
        assert!((pos.unmatched_exposure() - 3.0).abs() < 0.01, "Should have 3 unmatched");
    }
    
    /// Test: Position resolution calculates realized P&L
    #[test]
    fn test_position_resolution() {
        let mut pos = ArbPosition::new("TEST-MARKET", "Test");
        
        pos.poly_yes.add(10.0, 0.45);   // Cost: $4.50
        pos.poly_no.add(10.0, 0.50);   // Cost: $5.00
        pos.total_fees = 0.0;           // No fees
        
        // YES wins → Poly YES pays $10
        pos.resolve(true);
        
        assert_eq!(pos.status, "resolved");
        let pnl = pos.realized_pnl.expect("Should have realized P&L");
        
        // Payout: $10 - Cost: $9.50 = $0.50 profit
        assert!((pnl - 0.50).abs() < 0.01, "P&L should be ~$0.50, got {}", pnl);
    }
    
    /// Test: Daily P&L resets
    #[test]
    fn test_daily_pnl_persistence() {
        let mut tracker = PositionTracker::new();
        
        // Simulate some activity
        tracker.all_time_pnl = 100.0;
        tracker.daily_realized_pnl = 10.0;
        
        // Reset daily
        tracker.reset_daily();
        
        assert_eq!(tracker.daily_realized_pnl, 0.0, "Daily should reset");
        assert_eq!(tracker.all_time_pnl, 100.0, "All-time should persist");
    }
}

// ============================================================================
// CIRCUIT BREAKER TESTS - Verify safety limits
// ============================================================================

mod circuit_breaker_tests {
    use arb_bot::circuit_breaker::*;
    
    fn test_config() -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            max_position_per_market: 50,
            max_total_position: 200,
            max_daily_loss: 25.0,
            max_consecutive_errors: 3,
            cooldown_secs: 60,
            enabled: true,
        }
    }
    
    /// Test: Allows trades within limits
    #[tokio::test]
    async fn test_allows_trades_within_limits() {
        let cb = CircuitBreaker::new(test_config());
        
        // First trade should be allowed
        let result = cb.can_execute("market1", 10).await;
        assert!(result.is_ok(), "Should allow first trade");
        
        // Record success
        cb.record_success("market1", 10, 10, 0.50).await;
        
        // Second trade on same market should still be allowed
        let result = cb.can_execute("market1", 10).await;
        assert!(result.is_ok(), "Should allow second trade within limit");
    }
    
    /// Test: Blocks trade exceeding per-market limit
    #[tokio::test]
    async fn test_blocks_per_market_limit() {
        let cb = CircuitBreaker::new(test_config());
        
        // Fill up the market
        cb.record_success("market1", 45, 45, 1.0).await;
        
        // Try to add 10 more (would exceed 50 limit)
        let result = cb.can_execute("market1", 10).await;
        
        assert!(matches!(result, Err(TripReason::MaxPositionPerMarket { .. })),
            "Should block trade exceeding per-market limit");
    }
    
    /// Test: Blocks trade exceeding total position limit
    #[tokio::test]
    async fn test_blocks_total_position_limit() {
        let cb = CircuitBreaker::new(test_config());
        
        // Fill up multiple markets
        cb.record_success("market1", 50, 50, 1.0).await;
        cb.record_success("market2", 50, 50, 1.0).await;
        cb.record_success("market3", 50, 50, 1.0).await;
        cb.record_success("market4", 45, 45, 1.0).await;  // Total: 195
        
        // Try to add 10 more (would exceed 200 total limit)
        let result = cb.can_execute("market5", 10).await;
        
        assert!(matches!(result, Err(TripReason::MaxTotalPosition { .. })),
            "Should block trade exceeding total position limit");
    }
    
    /// Test: Consecutive errors trip the breaker
    #[tokio::test]
    async fn test_consecutive_errors_trip() {
        let cb = CircuitBreaker::new(test_config());
        
        // Record errors up to limit
        cb.record_error().await;
        assert!(cb.is_trading_allowed(), "Should still allow after 1 error");
        
        cb.record_error().await;
        assert!(cb.is_trading_allowed(), "Should still allow after 2 errors");
        
        cb.record_error().await;
        assert!(!cb.is_trading_allowed(), "Should halt after 3 errors");
        
        // Verify trip reason
        let status = cb.status().await;
        assert!(status.halted);
        assert!(matches!(status.trip_reason, Some(TripReason::ConsecutiveErrors { .. })));
    }
    
    /// Test: Success resets error count
    #[tokio::test]
    async fn test_success_resets_errors() {
        let cb = CircuitBreaker::new(test_config());
        
        // Record 2 errors
        cb.record_error().await;
        cb.record_error().await;
        
        // Record success
        cb.record_success("market1", 10, 10, 0.50).await;
        
        // Error count should be reset
        let status = cb.status().await;
        assert_eq!(status.consecutive_errors, 0, "Success should reset error count");
        
        // Should need 3 more errors to trip
        cb.record_error().await;
        cb.record_error().await;
        assert!(cb.is_trading_allowed());
    }
    
    /// Test: Manual reset clears halt
    #[tokio::test]
    async fn test_manual_reset() {
        let cb = CircuitBreaker::new(test_config());
        
        // Trip the breaker
        cb.record_error().await;
        cb.record_error().await;
        cb.record_error().await;
        assert!(!cb.is_trading_allowed());
        
        // Reset
        cb.reset().await;
        assert!(cb.is_trading_allowed(), "Should allow trading after reset");
        
        let status = cb.status().await;
        assert!(!status.halted);
        assert!(status.trip_reason.is_none());
    }
    
    /// Test: Disabled circuit breaker allows everything
    #[tokio::test]
    async fn test_disabled_allows_all() {
        let mut config = test_config();
        config.enabled = false;
        let cb = CircuitBreaker::new(config);
        
        // Should allow even excessive trades
        let result = cb.can_execute("market1", 1000).await;
        assert!(result.is_ok(), "Disabled CB should allow all trades");
        
        // Errors shouldn't trip it
        cb.record_error().await;
        cb.record_error().await;
        cb.record_error().await;
        cb.record_error().await;
        assert!(cb.is_trading_allowed(), "Disabled CB should never halt");
    }
}

// ============================================================================
// END-TO-END SCENARIO TESTS - Full flow simulation
// ============================================================================

mod e2e_tests {
    use arb_bot::position_tracker::*;
    use arb_bot::circuit_breaker::*;

    /// Scenario: Circuit breaker halts trading after losses
    #[tokio::test]
    async fn test_circuit_breaker_halts_on_losses() {
        let config = CircuitBreakerConfig {
            max_position_per_market: 100,
            max_total_position: 500,
            max_daily_loss: 10.0,  // Low threshold for test
            max_consecutive_errors: 5,
            cooldown_secs: 60,
            enabled: true,
        };
        
        let cb = CircuitBreaker::new(config);
        
        // Simulate a series of losing trades
        // (In reality this would come from actual fill data)
        cb.record_success("market1", 10, 10, -3.0).await;  // -$3
        cb.record_success("market2", 10, 10, -4.0).await;  // -$7 cumulative
        
        // Should still be allowed
        assert!(cb.can_execute("market3", 10).await.is_ok());
        
        // One more loss pushes over the limit
        cb.record_success("market3", 10, 10, -5.0).await;  // -$12 cumulative
        
        // Now should be blocked due to max daily loss
        let result = cb.can_execute("market4", 10).await;
        assert!(matches!(result, Err(TripReason::MaxDailyLoss { .. })),
            "Should halt due to max daily loss");
    }
    
    /// Scenario: Partial fill creates exposure warning (Poly YES vs Poly NO)
    #[tokio::test]
    async fn test_partial_fill_exposure_tracking() {
        let mut tracker = PositionTracker::new();
        
        // Full fill on YES side
        tracker.record_fill(&FillRecord::new(
            "TEST-MARKET",
            "Test",
            "polymarket",
            "yes",
            10.0,
            0.45,
            0.0,
            "order1",
        ));
        
        // Partial fill on NO side (slippage/liquidity issue)
        tracker.record_fill(&FillRecord::new(
            "TEST-MARKET",
            "Test",
            "polymarket",
            "no",
            7.0,  // Only got 7!
            0.50,
            0.0,
            "order2",
        ));
        
        let summary = tracker.summary();
        
        // Should show exposure
        assert!(
            summary.total_unmatched_exposure > 0.0,
            "Should show unmatched exposure: {}",
            summary.total_unmatched_exposure
        );
        
        // Matched should be limited to the smaller fill
        let position = tracker.get("TEST-MARKET").expect("Should have position");
        assert!((position.matched_contracts() - 7.0).abs() < 0.01);
        assert!((position.unmatched_exposure() - 3.0).abs() < 0.01);
    }
}

// ============================================================================
// FILL DATA ACCURACY TESTS - Verify actual vs expected prices
// ============================================================================

mod fill_accuracy_tests {
    use arb_bot::position_tracker::*;
    
    /// Test: Actual fill price different from expected
    #[test]
    fn test_fill_price_slippage() {
        let mut tracker = PositionTracker::new();
        
        // Expected: buy at 45¢, but actually filled at 47¢ (slippage)
        tracker.record_fill(&FillRecord::new(
            "TEST-MARKET",
            "Test",
            "polymarket",
            "yes",
            10.0,
            0.47,  // Actual fill price (worse than expected 0.45)
            0.0,
            "order1",
        ));
        
        let pos = tracker.get("TEST-MARKET").expect("Should have position");
        
        // Should use actual price
        assert!((pos.poly_yes.avg_price - 0.47).abs() < 0.001);
        assert!((pos.poly_yes.cost_basis - 4.70).abs() < 0.01);
    }
    
    /// Test: Multiple fills at different prices calculates weighted average
    #[test]
    fn test_multiple_fills_weighted_average() {
        let mut pos = ArbPosition::new("TEST", "Test");
        
        // First fill: 5 contracts at 45¢
        pos.poly_yes.add(5.0, 0.45);
        
        // Second fill: 5 contracts at 47¢ (price moved)
        pos.poly_yes.add(5.0, 0.47);
        
        // Weighted average: (5*0.45 + 5*0.47) / 10 = 0.46
        assert!((pos.poly_yes.avg_price - 0.46).abs() < 0.001);
        assert!((pos.poly_yes.cost_basis - 4.60).abs() < 0.01);
        assert!((pos.poly_yes.contracts - 10.0).abs() < 0.01);
    }
    
    /// Test: Polymarket has no fees
    #[test]
    fn test_polymarket_no_fees() {
        let mut tracker = PositionTracker::new();
        
        // Polymarket reports no fees
        tracker.record_fill(&FillRecord::new(
            "TEST-MARKET",
            "Test",
            "polymarket",
            "no",
            10.0,
            0.50,
            0.0,  // No fees on Polymarket
            "order1",
        ));
        
        let pos = tracker.get("TEST-MARKET").expect("Should have position");
        assert!((pos.total_fees - 0.0).abs() < 0.001, "Should have no fees");
    }
}

// ============================================================================
// INFRASTRUCTURE INTEGRATION TESTS
// ============================================================================

mod infra_integration_tests {
    use arb_bot::types::*;

    /// Helper to create market state with prices (Polymarket only)
    fn setup_market(
        poly_yes: PriceCents,
        poly_no: PriceCents,
    ) -> (GlobalState, u16) {
        let mut state = GlobalState::new();

        let pair = MarketPair {
            pair_id: "arb-test-market".into(),
            league: "epl".into(),
            market_type: MarketType::Moneyline,
            description: "Test Market".into(),
            poly_slug: "arb-test".into(),
            poly_yes_token: "arb_yes_token".into(),
            poly_no_token: "arb_no_token".into(),
            line_value: None,
            team_suffix: Some("CFC".into()),
        };

        let market_id = state.add_pair(pair).unwrap();

        // Set prices (Polymarket only)
        let market = state.get_by_id(market_id).unwrap();
        market.poly.store(poly_yes, poly_no, 1000, 1000);

        (state, market_id)
    }

    // =========================================================================
    // Arb Detection Tests (check_arbs)
    // =========================================================================

    /// Test: detects Polymarket-only arb (no fees)
    #[test]
    fn test_detects_poly_only_arb() {
        // Poly YES 48¢ + Poly NO 50¢ = 98¢ → 2% profit with ZERO fees!
        let (state, market_id) = setup_market(48, 50);

        let market = state.get_by_id(market_id).unwrap();
        let arb_mask = market.check_arbs(100);  // 100¢ = $1.00 threshold

        assert!(arb_mask & 4 != 0, "Should detect Poly-only arb (bit 2)");
    }

    /// Test: correctly rejects marginal arb when prices sum to >= $1.00
    #[test]
    fn test_rejects_marginal_arb() {
        // Poly YES 50¢ + Poly NO 50¢ = 100¢ → NOT AN ARB (costs exactly $1 payout!)
        let (state, market_id) = setup_market(50, 50);

        let market = state.get_by_id(market_id).unwrap();
        let arb_mask = market.check_arbs(100);

        assert!(arb_mask & 4 == 0, "Should reject marginal Poly-only arb");
    }

    /// Test: returns no arbs for efficient market
    #[test]
    fn test_no_arbs_in_efficient_market() {
        // All prices sum to > $1
        let (state, market_id) = setup_market(52, 52);

        let market = state.get_by_id(market_id).unwrap();
        let arb_mask = market.check_arbs(100);

        assert_eq!(arb_mask, 0, "Should detect no arbs in efficient market");
    }

    /// Test: handles missing prices correctly
    #[test]
    fn test_handles_missing_prices() {
        let (state, market_id) = setup_market(50, NO_PRICE);

        let market = state.get_by_id(market_id).unwrap();
        let arb_mask = market.check_arbs(100);

        assert_eq!(arb_mask, 0, "Should return 0 when any price is missing");
    }

    // =========================================================================
    // FastExecutionRequest Tests
    // =========================================================================

    /// Test: FastExecutionRequest calculates profit correctly (PolyOnly)
    #[test]
    fn test_execution_request_profit_calculation() {
        // Poly YES 45¢ + Poly NO 50¢ = 95¢
        // Profit = 100 - 95 = 5¢ (no fees!)
        let req = FastExecutionRequest {
            market_id: 0,
            yes_price: 45,
            no_price: 50,
            yes_size: 1000,
            no_size: 1000,
            arb_type: ArbType::PolyOnly,
            detected_ns: 0,
        };

        assert_eq!(req.profit_cents(), 5, "Profit should be 5¢");
    }

    /// Test: FastExecutionRequest handles negative profit
    #[test]
    fn test_execution_request_negative_profit() {
        // Prices too high - no profit
        let req = FastExecutionRequest {
            market_id: 0,
            yes_price: 52,
            no_price: 52,
            yes_size: 1000,
            no_size: 1000,
            arb_type: ArbType::PolyOnly,
            detected_ns: 0,
        };

        assert!(req.profit_cents() < 0, "Should calculate negative profit");
    }

    // =========================================================================
    // GlobalStateLookup Tests
    // =========================================================================

    /// Test: GlobalState lookup by Poly token hashes
    #[test]
    fn test_lookup_by_poly_hashes() {
        let (state, market_id) = setup_market(50, 50);

        let poly_yes_hash = fxhash_str("arb_yes_token");
        let poly_no_hash = fxhash_str("arb_no_token");

        assert_eq!(state.id_by_poly_yes_hash(poly_yes_hash), Some(market_id));
        assert_eq!(state.id_by_poly_no_hash(poly_no_hash), Some(market_id));
    }

    /// Test: GlobalState handles multiple markets
    #[test]
    fn test_multiple_markets() {
        let mut state = GlobalState::new();

        // Add 5 markets
        for i in 0..5 {
            let pair = MarketPair {
                pair_id: format!("market-{}", i).into(),
                league: "epl".into(),
                market_type: MarketType::Moneyline,
                description: format!("Market {}", i).into(),
                poly_slug: format!("test-{}", i).into(),
                poly_yes_token: format!("yes_{}", i).into(),
                poly_no_token: format!("no_{}", i).into(),
                line_value: None,
                team_suffix: None,
            };

            let id = state.add_pair(pair).unwrap();
            assert_eq!(id, i as u16);
        }

        assert_eq!(state.market_count(), 5);

        // All should be findable
        for i in 0..5 {
            assert!(state.get_by_id(i as u16).is_some());
        }
    }

    // =========================================================================
    // Price Conversion Tests
    // =========================================================================

    /// Test: Price conversion roundtrip
    #[test]
    fn test_price_conversion_roundtrip() {
        for cents in [1u16, 10, 25, 50, 75, 90, 99] {
            let price = cents_to_price(cents);
            let back = price_to_cents(price);
            assert_eq!(back, cents, "Roundtrip failed for {}¢", cents);
        }
    }

    /// Test: Fast price parsing
    #[test]
    fn test_parse_price_accuracy() {
        assert_eq!(parse_price("0.50"), 50);
        assert_eq!(parse_price("0.01"), 1);
        assert_eq!(parse_price("0.99"), 99);
        assert_eq!(parse_price("0.5"), 50);  // Short format
        assert_eq!(parse_price("invalid"), 0);  // Invalid
    }

    // =========================================================================
    // Full Flow Integration Test
    // =========================================================================

    /// Test: Complete arb detection → execution request flow (PolyOnly)
    #[test]
    fn test_complete_arb_flow() {
        // 1. Setup market with arb opportunity
        let (state, market_id) = setup_market(45, 50);

        // 2. Detect arb (threshold = 100 cents = $1.00)
        let market = state.get_by_id(market_id).unwrap();
        let arb_mask = market.check_arbs(100);

        assert!(arb_mask & 4 != 0, "Step 2: Should detect PolyOnly arb");

        // 3. Extract prices for execution
        let (p_yes, p_no, p_yes_sz, p_no_sz) = market.poly.load();

        // 4. Build execution request
        let req = FastExecutionRequest {
            market_id,
            yes_price: p_yes,
            no_price: p_no,
            yes_size: p_yes_sz,
            no_size: p_no_sz,
            arb_type: ArbType::PolyOnly,
            detected_ns: 0,
        };

        // 5. Verify request is valid
        assert_eq!(req.yes_price, 45, "YES price should be 45¢");
        assert_eq!(req.no_price, 50, "NO price should be 50¢");
        assert!(req.profit_cents() > 0, "Should have positive profit");

        // 6. Verify we can access market pair for execution
        let pair = market.pair.as_ref().expect("Should have pair");
        assert!(!pair.poly_yes_token.is_empty());
        assert!(!pair.poly_no_token.is_empty());
    }
}

// ============================================================================
// EXECUTION ENGINE TESTS - Test execution without real API calls
// ============================================================================

mod execution_tests {
    use arb_bot::types::*;
    use arb_bot::circuit_breaker::*;
    use arb_bot::position_tracker::*;

    /// Test: ExecutionEngine correctly filters low-profit opportunities
    #[tokio::test]
    async fn test_execution_profit_threshold() {
        // This tests the logic flow - actual execution would need mocked clients
        let req = FastExecutionRequest {
            market_id: 0,
            yes_price: 50,
            no_price: 50,
            yes_size: 1000,
            no_size: 1000,
            arb_type: ArbType::PolyOnly,
            detected_ns: 0,
        };

        // 50 + 50 = 100 → no profit
        assert!(req.profit_cents() <= 0, "Should have no profit");
    }

    /// Test: ExecutionEngine respects circuit breaker
    #[tokio::test]
    async fn test_circuit_breaker_integration() {
        let config = CircuitBreakerConfig {
            max_position_per_market: 50,
            max_total_position: 200,
            max_daily_loss: 25.0,
            max_consecutive_errors: 3,
            cooldown_secs: 60,
            enabled: true,
        };

        let cb = CircuitBreaker::new(config);

        // Fill up market position
        cb.record_success("market1", 45, 45, 1.0).await;

        // Should block when adding more
        let result = cb.can_execute("market1", 10).await;
        assert!(matches!(result, Err(TripReason::MaxPositionPerMarket { .. })));
    }

    /// Test: Position tracker records fills correctly (PolyOnly)
    #[tokio::test]
    async fn test_position_tracker_integration() {
        let mut tracker = PositionTracker::new();

        // Simulate fill recording (what ExecutionEngine does)
        tracker.record_fill(&FillRecord::new(
            "test-market-1",
            "Test Market",
            "polymarket",
            "yes",
            10.0,
            0.45,
            0.0,  // no fees
            "test_order_123",
        ));

        tracker.record_fill(&FillRecord::new(
            "test-market-1",
            "Test Market",
            "polymarket",
            "no",
            10.0,
            0.50,
            0.0,  // no fees
            "test_order_456",
        ));

        let summary = tracker.summary();
        assert_eq!(summary.open_positions, 1);
        assert!(summary.total_guaranteed_profit > 0.0);
    }

    /// Test: NanoClock provides monotonic timing
    #[test]
    fn test_nano_clock_monotonic() {
        use arb_bot::execution::NanoClock;

        let clock = NanoClock::new();

        let t1 = clock.now_ns();
        std::thread::sleep(std::time::Duration::from_micros(100));
        let t2 = clock.now_ns();

        assert!(t2 > t1, "Clock should be monotonic");
        assert!(t2 - t1 >= 100_000, "Should measure at least 100µs");
    }
}

// ============================================================================
// MISMATCHED FILL / AUTO-CLOSE EXPOSURE TESTS
// ============================================================================
// These tests verify that when Poly YES and Poly NO fill different quantities,
// the system correctly handles the unmatched exposure.

mod mismatched_fill_tests {
    use arb_bot::position_tracker::*;

    /// Test: When Poly YES fills more than Poly NO, we have excess YES exposure
    /// that needs to be sold to close the position.
    #[test]
    fn test_poly_yes_fills_more_than_no_creates_exposure() {
        let mut tracker = PositionTracker::new();

        // Scenario: Requested 10 contracts
        // Poly NO filled: 7 contracts at 50¢
        // Poly YES filled: 10 contracts at 45¢
        // Excess: 3 Poly YES contracts that aren't hedged

        // Record Poly NO fill (only 7)
        tracker.record_fill(&FillRecord::new(
            "TEST-MARKET",
            "Test Match",
            "polymarket",
            "no",
            7.0,      // Only 7 filled
            0.50,
            0.0,
            "poly_no_order_1",
        ));

        // Record Poly YES fill (full 10)
        tracker.record_fill(&FillRecord::new(
            "TEST-MARKET",
            "Test Match",
            "polymarket",
            "yes",
            10.0,     // Full 10 filled
            0.45,
            0.0,
            "poly_yes_order_1",
        ));

        let pos = tracker.get("TEST-MARKET").expect("Should have position");

        // Matched position: 7 contracts
        // Unmatched Poly YES: 3 contracts
        assert!((pos.poly_yes.contracts - 10.0).abs() < 0.01, "Poly YES should have 10 contracts");
        assert!((pos.poly_no.contracts - 7.0).abs() < 0.01, "Poly NO should have 7 contracts");

        // This position has EXPOSURE because poly_yes (10) != poly_no (7)
        // The 7 matched contracts are hedged (guaranteed profit)
        // The 3 excess poly_yes contracts are unhedged exposure
    }

    /// Test: When Poly NO fills more than Poly YES, we have excess NO exposure
    /// that needs to be sold to close the position.
    #[test]
    fn test_poly_no_fills_more_than_yes_creates_exposure() {
        let mut tracker = PositionTracker::new();

        // Scenario: Requested 10 contracts
        // Poly NO filled: 10 contracts at 50¢
        // Poly YES filled: 6 contracts at 45¢
        // Excess: 4 Poly NO contracts that aren't hedged

        // Record Poly NO fill (full 10)
        tracker.record_fill(&FillRecord::new(
            "TEST-MARKET",
            "Test Match",
            "polymarket",
            "no",
            10.0,     // Full 10 filled
            0.50,
            0.0,
            "poly_no_order_1",
        ));

        // Record Poly YES fill (only 6)
        tracker.record_fill(&FillRecord::new(
            "TEST-MARKET",
            "Test Match",
            "polymarket",
            "yes",
            6.0,      // Only 6 filled
            0.45,
            0.0,
            "poly_yes_order_1",
        ));

        let pos = tracker.get("TEST-MARKET").expect("Should have position");

        assert!((pos.poly_no.contracts - 10.0).abs() < 0.01, "Poly NO should have 10 contracts");
        assert!((pos.poly_yes.contracts - 6.0).abs() < 0.01, "Poly YES should have 6 contracts");

        // The 6 matched contracts are hedged
        // The 4 excess poly_no contracts are unhedged exposure
    }

    /// Test: After auto-closing excess Poly YES, position should be balanced
    #[test]
    fn test_auto_close_poly_yes_excess_balances_position() {
        let mut tracker = PositionTracker::new();

        // Initial mismatched fill
        tracker.record_fill(&FillRecord::new(
            "TEST-MARKET",
            "Test Match",
            "polymarket",
            "no",
            7.0,
            0.50,
            0.0,
            "poly_no_order_1",
        ));

        tracker.record_fill(&FillRecord::new(
            "TEST-MARKET",
            "Test Match",
            "polymarket",
            "yes",
            10.0,
            0.45,
            0.0,
            "poly_yes_order_1",
        ));

        // Simulate auto-close: SELL 3 Poly YES to close exposure
        tracker.record_fill(&FillRecord::new(
            "TEST-MARKET",
            "Test Match",
            "polymarket",
            "yes",
            -3.0,     // Negative = selling/reducing position
            0.43,     // Might get worse price on the close
            0.0,
            "poly_close_order",
        ));

        let pos = tracker.get("TEST-MARKET").expect("Should have position");

        // After auto-close, both sides should have 7 contracts
        assert!(
            (pos.poly_yes.contracts - 7.0).abs() < 0.01,
            "Poly YES should be reduced to 7 contracts, got {}",
            pos.poly_yes.contracts
        );
        assert!(
            (pos.poly_no.contracts - 7.0).abs() < 0.01,
            "Poly NO should still have 7 contracts, got {}",
            pos.poly_no.contracts
        );
    }

    /// Test: After auto-closing excess Poly NO, position should be balanced
    #[test]
    fn test_auto_close_poly_no_excess_balances_position() {
        let mut tracker = PositionTracker::new();

        // Initial mismatched fill
        tracker.record_fill(&FillRecord::new(
            "TEST-MARKET",
            "Test Match",
            "polymarket",
            "no",
            10.0,
            0.50,
            0.0,
            "poly_no_order_1",
        ));

        tracker.record_fill(&FillRecord::new(
            "TEST-MARKET",
            "Test Match",
            "polymarket",
            "yes",
            6.0,
            0.45,
            0.0,
            "poly_yes_order_1",
        ));

        // Simulate auto-close: SELL 4 Poly NO to close exposure
        tracker.record_fill(&FillRecord::new(
            "TEST-MARKET",
            "Test Match",
            "polymarket",
            "no",
            -4.0,     // Negative = selling/reducing position
            0.48,     // Might get worse price on the close
            0.0,
            "poly_close_order",
        ));

        let pos = tracker.get("TEST-MARKET").expect("Should have position");

        // After auto-close, both sides should have 6 contracts
        assert!(
            (pos.poly_no.contracts - 6.0).abs() < 0.01,
            "Poly NO should be reduced to 6 contracts, got {}",
            pos.poly_no.contracts
        );
        assert!(
            (pos.poly_yes.contracts - 6.0).abs() < 0.01,
            "Poly YES should still have 6 contracts, got {}",
            pos.poly_yes.contracts
        );
    }

    /// Test: Complete failure on one side creates full exposure
    /// (e.g., Poly YES fills 10, Poly NO fills 0)
    #[test]
    fn test_complete_one_side_failure_full_exposure() {
        let mut tracker = PositionTracker::new();

        // Poly YES succeeds
        tracker.record_fill(&FillRecord::new(
            "TEST-MARKET",
            "Test Match",
            "polymarket",
            "yes",
            10.0,
            0.45,
            0.0,
            "poly_yes_order_1",
        ));

        // Poly NO completely fails - no fill recorded

        let pos = tracker.get("TEST-MARKET").expect("Should have position");

        // Full Poly YES exposure - must be closed immediately
        assert!((pos.poly_yes.contracts - 10.0).abs() < 0.01);
        assert!((pos.poly_no.contracts - 0.0).abs() < 0.01);

        // This is a dangerous situation - 10 unhedged Poly YES contracts
        // Auto-close should sell all 10 Poly YES to eliminate exposure
    }

    /// Test: Auto-close after complete one-side failure
    #[test]
    fn test_auto_close_after_complete_failure() {
        let mut tracker = PositionTracker::new();

        // Poly YES fills, Poly NO fails completely
        tracker.record_fill(&FillRecord::new(
            "TEST-MARKET",
            "Test Match",
            "polymarket",
            "yes",
            10.0,
            0.45,
            0.0,
            "poly_yes_order_1",
        ));

        // Auto-close: Sell ALL 10 Poly YES contracts
        tracker.record_fill(&FillRecord::new(
            "TEST-MARKET",
            "Test Match",
            "polymarket",
            "yes",
            -10.0,    // Sell everything
            0.40,     // Might get a bad price in emergency close
            0.0,
            "poly_close_order",
        ));

        let pos = tracker.get("TEST-MARKET").expect("Should have position");

        // Position should be flat (0 contracts on both sides)
        assert!(
            (pos.poly_yes.contracts - 0.0).abs() < 0.01,
            "Poly YES should be 0 after emergency close, got {}",
            pos.poly_yes.contracts
        );
    }

    /// Test: Profit calculation with partial fill and auto-close
    #[test]
    fn test_profit_with_partial_fill_and_auto_close() {
        let mut tracker = PositionTracker::new();

        // Requested 10 contracts
        // Poly NO fills 8 @ 50¢ (cost: $4.00)
        // Poly YES fills 10 @ 45¢ (cost: $4.50)
        // Need to close 2 excess Poly YES @ 43¢ (receive: $0.86)

        // Initial fills
        tracker.record_fill(&FillRecord::new(
            "TEST-MARKET",
            "Test Match",
            "polymarket",
            "no",
            8.0,
            0.50,
            0.0,
            "poly_no_order_1",
        ));

        tracker.record_fill(&FillRecord::new(
            "TEST-MARKET",
            "Test Match",
            "polymarket",
            "yes",
            10.0,
            0.45,
            0.0,
            "poly_yes_order_1",
        ));

        // Auto-close 2 excess Poly YES
        tracker.record_fill(&FillRecord::new(
            "TEST-MARKET",
            "Test Match",
            "polymarket",
            "yes",
            -2.0,
            0.43,     // Sold at 43¢ (worse than 45¢ buy price)
            0.0,
            "poly_close_order",
        ));

        let pos = tracker.get("TEST-MARKET").expect("Should have position");

        // Net position: 8 matched contracts
        // Poly NO: 8 @ 50¢ = $4.00 cost
        // Poly YES: 8 @ ~45¢ = ~$3.60 cost (10*0.45 - 2*0.43 = 4.50 - 0.86 = 3.64 effective for 8)

        assert!(
            (pos.poly_no.contracts - 8.0).abs() < 0.01,
            "Should have 8 matched Poly NO"
        );
        assert!(
            (pos.poly_yes.contracts - 8.0).abs() < 0.01,
            "Should have 8 matched Poly YES"
        );

        // The matched 8 contracts have guaranteed profit:
        // $1.00 payout - $0.50 poly_no - ~$0.455 poly_yes = ~$0.045 per contract
        // But we also lost $0.02 per contract on the 2 we had to close (45¢ - 43¢)
    }
}

// ============================================================================
// PROCESS MOCK TESTS - Simulate execution flow without real APIs
// ============================================================================
// These tests verify that process correctly:
// 1. Records fills to the position tracker
// 2. Handles mismatched fills
// 3. Updates circuit breaker state
// 4. Captures order IDs from Polymarket

mod process_mock_tests {
    use arb_bot::types::*;
    use arb_bot::circuit_breaker::*;
    use arb_bot::position_tracker::*;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    /// Simulates what process does after execute_poly_only_leg_async returns
    /// This allows testing the position tracking logic without real API clients
    struct MockExecutionResult {
        yes_filled: i64,
        no_filled: i64,
        yes_cost: i64,  // cents
        no_cost: i64,    // cents
        yes_order_id: String,
        no_order_id: String,
    }

    /// Simulates the position tracking logic from process (PolyOnly)
    async fn simulate_process_position_tracking(
        tracker: &Arc<RwLock<PositionTracker>>,
        circuit_breaker: &CircuitBreaker,
        pair: &MarketPair,
        req: &FastExecutionRequest,
        result: MockExecutionResult,
    ) -> (i64, i16) {  // Returns (matched, profit_cents)
        let matched = result.yes_filled.min(result.no_filled);
        let actual_profit = matched as i16 * 100 - (result.yes_cost + result.no_cost) as i16;

        // Record success to circuit breaker
        if matched > 0 {
            circuit_breaker.record_success(&pair.pair_id, matched, matched, actual_profit as f64 / 100.0).await;
        }

        // === UPDATE POSITION TRACKER (mirrors process logic) ===
        if matched > 0 || result.yes_filled > 0 || result.no_filled > 0 {
            let mut tracker_guard = tracker.write().await;

            // Record Poly YES fill
            if result.yes_filled > 0 {
                tracker_guard.record_fill(&FillRecord::new(
                    &pair.pair_id,
                    &pair.description,
                    "polymarket",
                    "yes",
                    matched as f64,
                    result.yes_cost as f64 / 100.0 / result.yes_filled.max(1) as f64,
                    0.0,
                    &result.yes_order_id,
                ));
            }

            // Record Poly NO fill
            if result.no_filled > 0 {
                tracker_guard.record_fill(&FillRecord::new(
                    &pair.pair_id,
                    &pair.description,
                    "polymarket",
                    "no",
                    matched as f64,
                    result.no_cost as f64 / 100.0 / result.no_filled.max(1) as f64,
                    0.0,
                    &result.no_order_id,
                ));
            }

            tracker_guard.save_async();
        }

        (matched, actual_profit)
    }

    fn test_market_pair() -> MarketPair {
        MarketPair {
            pair_id: "process-fast-test".into(),
            league: "epl".into(),
            market_type: MarketType::Moneyline,
            description: "Process Fast Test Market".into(),
            poly_slug: "process-fast-test".into(),
            poly_yes_token: "pf_yes_token".into(),
            poly_no_token: "pf_no_token".into(),
            line_value: None,
            team_suffix: None,
        }
    }

    fn test_circuit_breaker_config() -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            max_position_per_market: 100,
            max_total_position: 500,
            max_daily_loss: 50.0,
            max_consecutive_errors: 5,
            cooldown_secs: 60,
            enabled: true,
        }
    }

    /// Test: process records both fills to position tracker with correct order IDs
    #[tokio::test]
    async fn test_process_records_fills_with_order_ids() {
        let tracker = Arc::new(RwLock::new(PositionTracker::new()));
        let cb = CircuitBreaker::new(test_circuit_breaker_config());
        let pair = test_market_pair();

        let req = FastExecutionRequest {
            market_id: 0,
            yes_price: 45,
            no_price: 50,
            yes_size: 1000,
            no_size: 1000,
            arb_type: ArbType::PolyOnly,
            detected_ns: 0,
        };

        let result = MockExecutionResult {
            yes_filled: 10,
            no_filled: 10,
            yes_cost: 450,  // 10 contracts × 45¢
            no_cost: 500,  // 10 contracts × 50¢
            yes_order_id: "poly_yes_order_abc123".to_string(),
            no_order_id: "poly_no_order_xyz789".to_string(),
        };

        let (matched, profit) = simulate_process_position_tracking(
            &tracker, &cb, &pair, &req, result
        ).await;

        // Verify matched contracts
        assert_eq!(matched, 10, "Should have 10 matched contracts");

        // Verify profit: 10 contracts × $1 payout - $4.50 YES - $5.00 NO = $0.50 = 50¢
        assert_eq!(profit, 50, "Should have 50¢ profit");

        // Verify position tracker was updated
        let tracker_guard = tracker.read().await;
        let summary = tracker_guard.summary();

        assert_eq!(summary.open_positions, 1, "Should have 1 open position");
        assert!(summary.total_contracts > 0.0, "Should have contracts recorded");

        // Verify the position has both legs recorded
        let pos = tracker_guard.get(&pair.pair_id).expect("Should have position");
        assert!(pos.poly_no.contracts > 0.0, "Should have Poly NO contracts");
        assert!(pos.poly_yes.contracts > 0.0, "Should have Poly YES contracts");
    }

    /// Test: process handles PolyOnly arb correctly (both sides on Polymarket)
    #[tokio::test]
    async fn test_process_poly_only_sides() {
        let tracker = Arc::new(RwLock::new(PositionTracker::new()));
        let cb = CircuitBreaker::new(test_circuit_breaker_config());
        let pair = test_market_pair();

        // PolyOnly: Buy YES and NO both on Polymarket
        let req = FastExecutionRequest {
            market_id: 0,
            yes_price: 45,
            no_price: 50,
            yes_size: 1000,
            no_size: 1000,
            arb_type: ArbType::PolyOnly,
            detected_ns: 0,
        };

        let result = MockExecutionResult {
            yes_filled: 10,
            no_filled: 10,
            yes_cost: 450,
            no_cost: 500,
            yes_order_id: "p_yes_order_1".to_string(),
            no_order_id: "p_no_order_1".to_string(),
        };

        simulate_process_position_tracking(&tracker, &cb, &pair, &req, result).await;

        let tracker_guard = tracker.read().await;
        let pos = tracker_guard.get(&pair.pair_id).expect("Should have position");

        // With arb_type = PolyOnly:
        // - Both sides are on Polymarket
        assert!((pos.poly_yes.contracts - 10.0).abs() < 0.01, "Poly YES should have 10 contracts");
        assert!((pos.poly_no.contracts - 10.0).abs() < 0.01, "Poly NO should have 10 contracts");
    }

    /// Test: process updates circuit breaker on success
    #[tokio::test]
    async fn test_process_updates_circuit_breaker() {
        let tracker = Arc::new(RwLock::new(PositionTracker::new()));
        let cb = CircuitBreaker::new(test_circuit_breaker_config());
        let pair = test_market_pair();

        let req = FastExecutionRequest {
            market_id: 0,
            yes_price: 45,
            no_price: 50,
            yes_size: 1000,
            no_size: 1000,
            arb_type: ArbType::PolyOnly,
            detected_ns: 0,
        };

        let result = MockExecutionResult {
            yes_filled: 10,
            no_filled: 10,
            yes_cost: 450,
            no_cost: 500,
            yes_order_id: "p_order_3".to_string(),
            no_order_id: "p_order_3".to_string(),
        };

        simulate_process_position_tracking(&tracker, &cb, &pair, &req, result).await;

        // Verify circuit breaker was updated
        let status = cb.status().await;
        assert_eq!(status.consecutive_errors, 0, "Errors should be reset after success");
        assert!(status.total_position > 0, "Total position should be tracked");
    }

    /// Test: process handles partial YES fill correctly
    #[tokio::test]
    async fn test_process_partial_yes_fill() {
        let tracker = Arc::new(RwLock::new(PositionTracker::new()));
        let cb = CircuitBreaker::new(test_circuit_breaker_config());
        let pair = test_market_pair();

        let req = FastExecutionRequest {
            market_id: 0,
            yes_price: 45,
            no_price: 50,
            yes_size: 1000,
            no_size: 1000,
            arb_type: ArbType::PolyOnly,
            detected_ns: 0,
        };

        // YES only fills 7 out of 10
        let result = MockExecutionResult {
            yes_filled: 7,
            no_filled: 10,
            yes_cost: 315,  // 7 × 45¢
            no_cost: 500,   // 10 × 50¢
            yes_order_id: "p_partial_yes".to_string(),
            no_order_id: "p_full_no".to_string(),
        };

        let (matched, _profit) = simulate_process_position_tracking(
            &tracker, &cb, &pair, &req, result
        ).await;

        // Matched should be min(7, 10) = 7
        assert_eq!(matched, 7, "Matched should be min of both fills");

        let tracker_guard = tracker.read().await;
        let pos = tracker_guard.get(&pair.pair_id).expect("Should have position");

        // Position tracker records matched amounts (7), not total fills
        assert!((pos.poly_yes.contracts - 7.0).abs() < 0.01, "Should record matched Poly YES contracts");
        assert!((pos.poly_no.contracts - 7.0).abs() < 0.01, "Should record matched Poly NO contracts");
    }

    /// Test: process handles partial NO fill correctly
    #[tokio::test]
    async fn test_process_partial_no_fill() {
        let tracker = Arc::new(RwLock::new(PositionTracker::new()));
        let cb = CircuitBreaker::new(test_circuit_breaker_config());
        let pair = test_market_pair();

        let req = FastExecutionRequest {
            market_id: 0,
            yes_price: 45,
            no_price: 50,
            yes_size: 1000,
            no_size: 1000,
            arb_type: ArbType::PolyOnly,
            detected_ns: 0,
        };

        // NO only fills 6 out of 10
        let result = MockExecutionResult {
            yes_filled: 10,
            no_filled: 6,
            yes_cost: 450,  // 10 × 45¢
            no_cost: 300,   // 6 × 50¢
            yes_order_id: "p_full_yes".to_string(),
            no_order_id: "p_partial_no".to_string(),
        };

        let (matched, _profit) = simulate_process_position_tracking(
            &tracker, &cb, &pair, &req, result
        ).await;

        // Matched should be min(10, 6) = 6
        assert_eq!(matched, 6, "Matched should be min of both fills");
    }

    /// Test: process handles zero YES fill
    #[tokio::test]
    async fn test_process_zero_yes_fill() {
        let tracker = Arc::new(RwLock::new(PositionTracker::new()));
        let cb = CircuitBreaker::new(test_circuit_breaker_config());
        let pair = test_market_pair();

        let req = FastExecutionRequest {
            market_id: 0,
            yes_price: 45,
            no_price: 50,
            yes_size: 1000,
            no_size: 1000,
            arb_type: ArbType::PolyOnly,
            detected_ns: 0,
        };

        // YES fills 0, NO fills 10 (complete failure on one side)
        let result = MockExecutionResult {
            yes_filled: 0,
            no_filled: 10,
            yes_cost: 0,
            no_cost: 500,
            yes_order_id: "".to_string(),
            no_order_id: "p_only_no".to_string(),
        };

        let (matched, _profit) = simulate_process_position_tracking(
            &tracker, &cb, &pair, &req, result
        ).await;

        // No matched contracts since one side is 0
        assert_eq!(matched, 0, "No matched contracts when one side is 0");
    }

    /// Test: process handles zero NO fill
    #[tokio::test]
    async fn test_process_zero_no_fill() {
        let tracker = Arc::new(RwLock::new(PositionTracker::new()));
        let cb = CircuitBreaker::new(test_circuit_breaker_config());
        let pair = test_market_pair();

        let req = FastExecutionRequest {
            market_id: 0,
            yes_price: 45,
            no_price: 50,
            yes_size: 1000,
            no_size: 1000,
            arb_type: ArbType::PolyOnly,
            detected_ns: 0,
        };

        // YES fills 10, NO fills 0
        let result = MockExecutionResult {
            yes_filled: 10,
            no_filled: 0,
            yes_cost: 450,
            no_cost: 0,
            yes_order_id: "p_only_yes".to_string(),
            no_order_id: "".to_string(),
        };

        let (matched, _profit) = simulate_process_position_tracking(
            &tracker, &cb, &pair, &req, result
        ).await;

        assert_eq!(matched, 0, "No matched contracts when NO is 0");
    }

    /// Test: process correctly calculates profit with full fills
    #[tokio::test]
    async fn test_process_profit_calculation_full_fill() {
        let tracker = Arc::new(RwLock::new(PositionTracker::new()));
        let cb = CircuitBreaker::new(test_circuit_breaker_config());
        let pair = test_market_pair();

        let req = FastExecutionRequest {
            market_id: 0,
            yes_price: 45,  // Poly YES at 45¢
            no_price: 50,   // Poly NO at 50¢
            yes_size: 1000,
            no_size: 1000,
            arb_type: ArbType::PolyOnly,
            detected_ns: 0,
        };

        let result = MockExecutionResult {
            yes_filled: 10,
            no_filled: 10,
            yes_cost: 450,  // 10 × 45¢ = $4.50 = 450¢
            no_cost: 500,   // 10 × 50¢ = $5.00 = 500¢
            yes_order_id: "p_profit_yes".to_string(),
            no_order_id: "p_profit_no".to_string(),
        };

        let (matched, profit) = simulate_process_position_tracking(
            &tracker, &cb, &pair, &req, result
        ).await;

        // Profit = matched × $1 payout - costs
        // = 10 × 100¢ - 450¢ - 500¢
        // = 1000¢ - 950¢
        // = 50¢
        assert_eq!(matched, 10);
        assert_eq!(profit, 50, "Profit should be 50¢ ($0.50)");
    }

    /// Test: process correctly calculates profit with partial fill
    #[tokio::test]
    async fn test_process_profit_calculation_partial_fill() {
        let tracker = Arc::new(RwLock::new(PositionTracker::new()));
        let cb = CircuitBreaker::new(test_circuit_breaker_config());
        let pair = test_market_pair();

        let req = FastExecutionRequest {
            market_id: 0,
            yes_price: 45,
            no_price: 50,
            yes_size: 1000,
            no_size: 1000,
            arb_type: ArbType::PolyOnly,
            detected_ns: 0,
        };

        // Partial fill: YES 7, NO 10
        let result = MockExecutionResult {
            yes_filled: 7,
            no_filled: 10,
            yes_cost: 315,  // 7 × 45¢
            no_cost: 500,   // 10 × 50¢ (but only 7 are matched)
            yes_order_id: "p_partial_profit_yes".to_string(),
            no_order_id: "p_partial_profit_no".to_string(),
        };

        let (matched, profit) = simulate_process_position_tracking(
            &tracker, &cb, &pair, &req, result
        ).await;

        // Profit = matched × $1 payout - ALL costs (including unmatched)
        // = 7 × 100¢ - 315¢ - 500¢
        // = 700¢ - 815¢
        // = -115¢ (LOSS because we paid for 10 NO but only matched 7!)
        assert_eq!(matched, 7);
        assert_eq!(profit, -115, "Should have -115¢ loss due to unmatched NO contracts");
    }

    /// Test: Multiple executions accumulate in position tracker
    #[tokio::test]
    async fn test_process_multiple_executions_accumulate() {
        let tracker = Arc::new(RwLock::new(PositionTracker::new()));
        let cb = CircuitBreaker::new(test_circuit_breaker_config());
        let pair = test_market_pair();

        let req = FastExecutionRequest {
            market_id: 0,
            yes_price: 45,
            no_price: 50,
            yes_size: 1000,
            no_size: 1000,
            arb_type: ArbType::PolyOnly,
            detected_ns: 0,
        };

        // First execution: 10 contracts
        let result1 = MockExecutionResult {
            yes_filled: 10,
            no_filled: 10,
            yes_cost: 450,
            no_cost: 500,
            yes_order_id: "p_exec_1_yes".to_string(),
            no_order_id: "p_exec_1_no".to_string(),
        };

        simulate_process_position_tracking(&tracker, &cb, &pair, &req, result1).await;

        // Second execution: 5 more contracts
        let result2 = MockExecutionResult {
            yes_filled: 5,
            no_filled: 5,
            yes_cost: 225,
            no_cost: 250,
            yes_order_id: "p_exec_2_yes".to_string(),
            no_order_id: "p_exec_2_no".to_string(),
        };

        simulate_process_position_tracking(&tracker, &cb, &pair, &req, result2).await;

        // Verify accumulated position
        let tracker_guard = tracker.read().await;
        let pos = tracker_guard.get(&pair.pair_id).expect("Should have position");

        // Should have 15 total contracts (10 + 5)
        assert!(
            (pos.poly_no.contracts - 15.0).abs() < 0.01,
            "Should have accumulated 15 Poly NO contracts, got {}",
            pos.poly_no.contracts
        );
        assert!(
            (pos.poly_yes.contracts - 15.0).abs() < 0.01,
            "Should have accumulated 15 Poly YES contracts, got {}",
            pos.poly_yes.contracts
        );
    }

    /// Test: Circuit breaker tracks accumulated position per market
    #[tokio::test]
    async fn test_circuit_breaker_accumulates_position() {
        let tracker = Arc::new(RwLock::new(PositionTracker::new()));
        let cb = CircuitBreaker::new(test_circuit_breaker_config());
        let pair = test_market_pair();

        let req = FastExecutionRequest {
            market_id: 0,
            yes_price: 45,
            no_price: 50,
            yes_size: 1000,
            no_size: 1000,
            arb_type: ArbType::PolyOnly,
            detected_ns: 0,
        };

        // Execute multiple times
        for i in 0..5 {
            let result = MockExecutionResult {
                yes_filled: 10,
                no_filled: 10,
                yes_cost: 450,
                no_cost: 500,
                yes_order_id: format!("p_cb_{}_yes", i),
                no_order_id: format!("p_cb_{}_no", i),
            };

            simulate_process_position_tracking(&tracker, &cb, &pair, &req, result).await;
        }

        // Circuit breaker tracks contracts on BOTH sides (yes + no)
        // 5 executions × 10 matched × 2 sides = 100 total
        let status = cb.status().await;
        assert_eq!(status.total_position, 100, "Circuit breaker should track 100 contracts total (both sides)");
    }

    // =========================================================================
    // POLYONLY ARB TESTS
    // =========================================================================

    /// Test: PolyOnly arb (Poly YES + Poly NO on same platform - zero fees)
    #[tokio::test]
    async fn test_process_poly_only_arb() {
        let tracker = Arc::new(RwLock::new(PositionTracker::new()));
        let cb = CircuitBreaker::new(test_circuit_breaker_config());
        let pair = test_market_pair();

        // PolyOnly: Buy YES and NO both on Polymarket
        // This is profitable when Poly YES + Poly NO < $1
        let req = FastExecutionRequest {
            market_id: 0,
            yes_price: 48,  // Poly YES at 48¢
            no_price: 50,   // Poly NO at 50¢ (total = 98¢, 2¢ profit with NO fees!)
            yes_size: 1000,
            no_size: 1000,
            arb_type: ArbType::PolyOnly,
            detected_ns: 0,
        };

        // Verify fee calculation
        assert_eq!(req.estimated_fee_cents(), 0, "PolyOnly should have ZERO fees");
        assert_eq!(req.profit_cents(), 2, "PolyOnly profit = 100 - 48 - 50 - 0 = 2¢");
    }

    /// Test: PolyOnly fee calculation is always zero
    #[test]
    fn test_poly_only_zero_fees() {
        for yes_price in [10u16, 25, 50, 75, 90] {
            for no_price in [10u16, 25, 50, 75, 90] {
                let req = FastExecutionRequest {
                    market_id: 0,
                    yes_price,
                    no_price,
                    yes_size: 1000,
                    no_size: 1000,
                    arb_type: ArbType::PolyOnly,
                    detected_ns: 0,
                };
                assert_eq!(req.estimated_fee_cents(), 0,
                    "PolyOnly should always have 0 fees, got {} for prices ({}, {})",
                    req.estimated_fee_cents(), yes_price, no_price);
            }
        }
    }
}
