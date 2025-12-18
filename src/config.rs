// src/config.rs
// Configuration constants and league mappings

/// Polymarket WebSocket URL
pub const POLYMARKET_WS_URL: &str = "wss://ws-subscriptions-clob.polymarket.com/ws/market";

/// Gamma API base URL (Polymarket market data)
pub const GAMMA_API_BASE: &str = "https://gamma-api.polymarket.com";

/// Arb threshold: alert when total cost < this (e.g., 0.995 = 0.5% profit)
pub const ARB_THRESHOLD: f64 = 0.995;

/// Polymarket ping interval (seconds) - keep connection alive
pub const POLY_PING_INTERVAL_SECS: u64 = 30;

/// WebSocket reconnect delay (seconds)
pub const WS_RECONNECT_DELAY_SECS: u64 = 5;

/// Which leagues to monitor (empty slice = all)
pub const ENABLED_LEAGUES: &[&str] = &[];

/// Price logging enabled (set PRICE_LOGGING=1 to enable)
#[allow(dead_code)]
pub fn price_logging_enabled() -> bool {
    static CACHED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CACHED.get_or_init(|| {
        std::env::var("PRICE_LOGGING")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false)
    })
}

/// League configuration for market discovery
#[derive(Debug, Clone)]
pub struct LeagueConfig {
    pub league_code: &'static str,
    pub poly_prefix: &'static str,
}

/// Get all supported leagues with their configurations
pub fn get_league_configs() -> Vec<LeagueConfig> {
    vec![
        LeagueConfig {
            league_code: "epl",
            poly_prefix: "epl",
        },
        LeagueConfig {
            league_code: "bundesliga",
            poly_prefix: "bun",
        },
        LeagueConfig {
            league_code: "laliga",
            poly_prefix: "lal",
        },
        LeagueConfig {
            league_code: "seriea",
            poly_prefix: "sea",
        },
        LeagueConfig {
            league_code: "ligue1",
            poly_prefix: "fl1",
        },
        LeagueConfig {
            league_code: "ucl",
            poly_prefix: "ucl",
        },
        LeagueConfig {
            league_code: "uel",
            poly_prefix: "uel",
        },
        LeagueConfig {
            league_code: "eflc",
            poly_prefix: "elc",
        },
        LeagueConfig {
            league_code: "nba",
            poly_prefix: "nba",
        },
        LeagueConfig {
            league_code: "nfl",
            poly_prefix: "nfl",
        },
        LeagueConfig {
            league_code: "nhl",
            poly_prefix: "nhl",
        },
        LeagueConfig {
            league_code: "mlb",
            poly_prefix: "mlb",
        },
        LeagueConfig {
            league_code: "mls",
            poly_prefix: "mls",
        },
        LeagueConfig {
            league_code: "ncaaf",
            poly_prefix: "cfb",
        },
    ]
}

/// Get config for a specific league
pub fn get_league_config(league: &str) -> Option<LeagueConfig> {
    get_league_configs()
        .into_iter()
        .find(|c| c.league_code == league || c.poly_prefix == league)
}