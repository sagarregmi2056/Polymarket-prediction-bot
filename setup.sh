#!/bin/bash
# Setup script for Polymarket Arbitrage Bot

set -e

echo "🚀 Setting up Polymarket Arbitrage Bot..."

# Create necessary directories
echo "📁 Creating directories..."
mkdir -p data logs

# Create .env.example if it doesn't exist
if [ ! -f .env.example ]; then
    cat > .env.example << 'EOF'
# Polymarket Arbitrage Bot - Environment Variables Example
# Copy this file to .env and fill in your values

# ============================================================================
# REQUIRED: Polymarket Credentials
# ============================================================================
POLY_PRIVATE_KEY=0xYOUR_WALLET_PRIVATE_KEY_HERE
POLY_FUNDER=0xYOUR_WALLET_ADDRESS_HERE

# ============================================================================
# Bot Configuration
# ============================================================================
DRY_RUN=1
RUST_LOG=info

# Market discovery (comma-separated Polymarket slugs)
# Example: POLY_MARKET_SLUGS=epl-che-avl-2025-12-08,epl-mci-liv-2025-12-09
POLY_MARKET_SLUGS=

# ============================================================================
# Advanced Configuration
# ============================================================================

# Force market re-discovery (ignore cache)
FORCE_DISCOVERY=0

# Verbose price logging
PRICE_LOGGING=0

# Test mode (inject synthetic arb for testing)
TEST_ARB=0
TEST_ARB_TYPE=poly_only

# ============================================================================
# Circuit Breaker Settings
# ============================================================================
CB_ENABLED=true
CB_MAX_POSITION_PER_MARKET=100
CB_MAX_TOTAL_POSITION=500
CB_MAX_DAILY_LOSS=5000
CB_MAX_CONSECUTIVE_ERRORS=5
CB_COOLDOWN_SECS=60
EOF
    echo "✅ Created .env.example"
fi

# Create .env from example if it doesn't exist
if [ ! -f .env ]; then
    cp .env.example .env
    echo "✅ Created .env file from .env.example"
    echo "⚠️  Please edit .env and add your Polymarket credentials!"
else
    echo "ℹ️  .env file already exists"
fi

# Set permissions
chmod 600 .env 2>/dev/null || true

echo ""
echo "✅ Setup complete!"
echo ""
echo "Next steps:"
echo "1. Edit .env file and add your POLY_PRIVATE_KEY and POLY_FUNDER"
echo "2. For Docker: docker-compose up"
echo "3. For native: cargo run --release"
echo ""

