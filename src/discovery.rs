// src/discovery.rs
// Market discovery - discovers Polymarket markets

use anyhow::Result;
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

use crate::config::{LeagueConfig, get_league_configs, get_league_config};
use crate::polymarket::GammaClient;
use crate::types::{MarketPair, MarketType, DiscoveryResult};

/// Max concurrent Gamma API requests
const GAMMA_CONCURRENCY: usize = 20;

/// Cache file path
const DISCOVERY_CACHE_PATH: &str = ".discovery_cache.json";

/// Cache TTL in seconds (2 hours - new markets appear every ~2 hours)
const CACHE_TTL_SECS: u64 = 2 * 60 * 60;


/// Persistent cache for discovered market pairs
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiscoveryCache {
    /// Unix timestamp when cache was created
    timestamp_secs: u64,
    /// Cached market pairs
    pairs: Vec<MarketPair>,
    /// Set of known Polymarket slugs (for incremental updates)
    known_poly_slugs: Vec<String>,
}

impl DiscoveryCache {
    fn new(pairs: Vec<MarketPair>) -> Self {
        let known_poly_slugs: Vec<String> = pairs.iter()
            .map(|p| p.poly_slug.to_string())
            .collect();
        Self {
            timestamp_secs: current_unix_secs(),
            pairs,
            known_poly_slugs,
        }
    }

    fn is_expired(&self) -> bool {
        let now = current_unix_secs();
        now.saturating_sub(self.timestamp_secs) > CACHE_TTL_SECS
    }

    fn age_secs(&self) -> u64 {
        current_unix_secs().saturating_sub(self.timestamp_secs)
    }

    fn has_slug(&self, slug: &str) -> bool {
        self.known_poly_slugs.iter().any(|s| s == slug)
    }
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Market discovery client
pub struct DiscoveryClient {
    gamma: Arc<GammaClient>,
}

impl DiscoveryClient {
    pub fn new() -> Self {
        Self {
            gamma: Arc::new(GammaClient::new()),
        }
    }

    /// Load cache from disk (async)
    async fn load_cache() -> Option<DiscoveryCache> {
        let data = tokio::fs::read_to_string(DISCOVERY_CACHE_PATH).await.ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Save cache to disk (async)
    async fn save_cache(cache: &DiscoveryCache) -> Result<()> {
        let data = serde_json::to_string_pretty(cache)?;
        tokio::fs::write(DISCOVERY_CACHE_PATH, data).await?;
        Ok(())
    }
    
    /// Discover all market pairs with caching support
    ///
    /// Strategy:
    /// 1. Try to load cache from disk
    /// 2. If cache exists and is fresh (<2 hours), use it directly
    /// 3. If cache exists but is stale, load it + fetch incremental updates
    /// 4. If no cache, do full discovery
    pub async fn discover_all(&self, leagues: &[&str]) -> DiscoveryResult {
        // Try to load existing cache
        let cached = Self::load_cache().await;

        match cached {
            Some(cache) if !cache.is_expired() => {
                // Cache is fresh - use it directly
                let pair_count = cache.pairs.len();
                info!("📂 Loaded {} pairs from cache (age: {}s)",
                      pair_count, cache.age_secs());
                return DiscoveryResult {
                    pairs: cache.pairs,
                    kalshi_events_found: 0,  // From cache (kept for compatibility)
                    poly_matches: pair_count,
                    poly_misses: 0,
                    errors: vec![],
                };
            }
            Some(cache) => {
                // Cache is stale - do incremental discovery
                info!("📂 Cache expired (age: {}s), doing incremental refresh...", cache.age_secs());
                return self.discover_incremental(leagues, cache).await;
            }
            None => {
                // No cache - do full discovery
                info!("📂 No cache found, doing full discovery...");
            }
        }

        // Full discovery (no cache)
        let result = self.discover_full(leagues).await;

        // Save to cache
        if !result.pairs.is_empty() {
            let cache = DiscoveryCache::new(result.pairs.clone());
            if let Err(e) = Self::save_cache(&cache).await {
                warn!("Failed to save discovery cache: {}", e);
            } else {
                info!("💾 Saved {} pairs to cache", result.pairs.len());
            }
        }

        result
    }

    /// Force full discovery (ignores cache)
    pub async fn discover_all_force(&self, leagues: &[&str]) -> DiscoveryResult {
        info!("🔄 Forced full discovery (ignoring cache)...");
        let result = self.discover_full(leagues).await;

        // Save to cache
        if !result.pairs.is_empty() {
            let cache = DiscoveryCache::new(result.pairs.clone());
            if let Err(e) = Self::save_cache(&cache).await {
                warn!("Failed to save discovery cache: {}", e);
            } else {
                info!("💾 Saved {} pairs to cache", result.pairs.len());
            }
        }

        result
    }

    /// Full discovery without cache
    async fn discover_full(&self, leagues: &[&str]) -> DiscoveryResult {
        let configs: Vec<_> = if leagues.is_empty() {
            get_league_configs()
        } else {
            leagues.iter()
                .filter_map(|l| get_league_config(l))
                .collect()
        };

        // Parallel discovery across all leagues
        let league_futures: Vec<_> = configs.iter()
            .map(|config| self.discover_league(config, None))
            .collect();

        let league_results = futures_util::future::join_all(league_futures).await;

        // Merge results
        let mut result = DiscoveryResult::default();
        for league_result in league_results {
            result.pairs.extend(league_result.pairs);
            result.poly_matches += league_result.poly_matches;
            result.errors.extend(league_result.errors);
        }
        result.kalshi_events_found = result.pairs.len();

        result
    }

    /// Incremental discovery - merge cached pairs with newly discovered ones
    async fn discover_incremental(&self, leagues: &[&str], cache: DiscoveryCache) -> DiscoveryResult {
        let configs: Vec<_> = if leagues.is_empty() {
            get_league_configs()
        } else {
            leagues.iter()
                .filter_map(|l| get_league_config(l))
                .collect()
        };

        // Discover with filter for known tickers
        let league_futures: Vec<_> = configs.iter()
            .map(|config| self.discover_league(config, Some(&cache)))
            .collect();

        let league_results = futures_util::future::join_all(league_futures).await;

        // Merge cached pairs with newly discovered ones
        let mut all_pairs = cache.pairs;
        let mut new_count = 0;

        for league_result in league_results {
            for pair in league_result.pairs {
                if !all_pairs.iter().any(|p| *p.poly_slug == *pair.poly_slug) {
                    all_pairs.push(pair);
                    new_count += 1;
                }
            }
        }

        if new_count > 0 {
            info!("🆕 Found {} new market pairs", new_count);

            // Update cache
            let new_cache = DiscoveryCache::new(all_pairs.clone());
            if let Err(e) = Self::save_cache(&new_cache).await {
                warn!("Failed to update discovery cache: {}", e);
            } else {
                info!("💾 Updated cache with {} total pairs", all_pairs.len());
            }
        } else {
            info!("✅ No new markets found, using {} cached pairs", all_pairs.len());

            // Just update timestamp to extend TTL
            let refreshed_cache = DiscoveryCache::new(all_pairs.clone());
            let _ = Self::save_cache(&refreshed_cache).await;
        }

        DiscoveryResult {
            pairs: all_pairs,
            kalshi_events_found: new_count,
            poly_matches: new_count,
            poly_misses: 0,
            errors: vec![],
        }
    }
    
    /// Discover all market types for a single league
    /// If cache is provided, only discovers markets not already in cache
    async fn discover_league(&self, config: &LeagueConfig, cache: Option<&DiscoveryCache>) -> DiscoveryResult {
        info!("🔍 Discovering {} markets from Polymarket...", config.league_code);
        
        let mut result = DiscoveryResult::default();
        
        // Try to discover markets by searching for slugs with the league prefix
        // For now, we'll search for markets manually via POLY_MARKET_SLUGS env var
        // Format: comma-separated slugs like "epl-che-avl-2025-12-08,epl-mci-liv-2025-12-09"
        let market_slugs_env = std::env::var("POLY_MARKET_SLUGS").unwrap_or_default();
        
        if !market_slugs_env.is_empty() {
            let slugs: Vec<&str> = market_slugs_env.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
            info!("  📋 Found {} market slugs from POLY_MARKET_SLUGS", slugs.len());
            
            for slug in slugs {
                // Skip if already in cache
                if let Some(c) = &cache {
                    if c.has_slug(slug) {
                        continue;
                    }
                }
                
                match self.gamma.lookup_market(slug).await {
                    Ok(Some((yes_token, no_token, description))) => {
                        // Extract market info from slug
                        let parts: Vec<&str> = slug.split('-').collect();
                        let league = if parts.len() > 0 { parts[0] } else { &config.league_code };
                        
                        let pair = MarketPair {
                            pair_id: format!("poly-{}", slug).into(),
                            league: league.into(),
                            market_type: MarketType::Moneyline, // Default to moneyline
                            description: description.into(),
                            poly_slug: slug.into(),
                            poly_yes_token: yes_token.into(),
                            poly_no_token: no_token.into(),
                            line_value: None,
                            team_suffix: None,
                        };
                        
                        result.pairs.push(pair);
                        result.poly_matches += 1;
                    }
                    Ok(None) => {
                        warn!("  ⚠️ Market not found: {}", slug);
                        result.poly_misses += 1;
                    }
                    Err(e) => {
                        warn!("  ⚠️ Error looking up {}: {}", slug, e);
                        result.errors.push(format!("Failed to lookup {}: {}", slug, e));
                    }
                }
            }
        } else {
            // Try to search for markets using Gamma API search
            // Note: Gamma API doesn't have a direct search endpoint, so we'll use a workaround
            // For now, we'll try to fetch markets by querying common patterns
            warn!("  ⚠️ POLY_MARKET_SLUGS not set. Set it to comma-separated market slugs to discover markets.");
            warn!("  ⚠️ Example: POLY_MARKET_SLUGS='epl-che-avl-2025-12-08,epl-mci-liv-2025-12-09'");
            
            // Try to discover markets by searching for recent markets with the league prefix
            // This is a simplified approach - in production you'd want a more sophisticated search
            let search_result = self.search_markets_by_prefix(&config.poly_prefix).await;
            match search_result {
                Ok(markets) => {
                    info!("  ✅ Found {} markets via search", markets.len());
                    for (slug, yes_token, no_token, description) in markets {
                        // Skip if already in cache
                        if let Some(c) = &cache {
                            if c.has_slug(&slug) {
                                continue;
                            }
                        }
                        
                        let pair = MarketPair {
                            pair_id: format!("poly-{}", slug).into(),
                            league: config.league_code.into(),
                            market_type: MarketType::Moneyline,
                            description: description.into(),
                            poly_slug: slug.into(),
                            poly_yes_token: yes_token.into(),
                            poly_no_token: no_token.into(),
                            line_value: None,
                            team_suffix: None,
                        };
                        
                        result.pairs.push(pair);
                        result.poly_matches += 1;
                    }
                }
                Err(e) => {
                    warn!("  ⚠️ Search failed: {}", e);
                    result.errors.push(format!("Search failed: {}", e));
                }
            }
        }
        
        if result.pairs.is_empty() {
            warn!("  ⚠️ No markets discovered for {}", config.league_code);
        } else {
            info!("  ✅ {} {}: {} pairs", config.league_code, MarketType::Moneyline, result.pairs.len());
        }
        
        result
    }
    
    /// Search for markets by prefix using Gamma API
    /// Note: This is a simplified implementation - Gamma API doesn't have direct search
    /// For now, this returns empty results. You'll need to implement actual search logic
    /// or use POLY_MARKET_SLUGS environment variable to specify markets manually
    async fn search_markets_by_prefix(&self, prefix: &str) -> Result<Vec<(String, String, String, String)>> {
        // Gamma API doesn't have a direct search endpoint
        // You would need to:
        // 1. Use Polymarket's frontend API or GraphQL endpoint
        // 2. Or maintain a list of known market slugs
        // 3. Or use a third-party API that indexes Polymarket markets
        
        // For now, return empty - users should use POLY_MARKET_SLUGS env var
        warn!("  ⚠️ Market search by prefix not implemented. Use POLY_MARKET_SLUGS env var instead.");
        Ok(vec![])
    }
    
}
