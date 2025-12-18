.PHONY: help build run stop logs clean test docker-build docker-up docker-down docker-logs docker-restart

help: ## Show this help message
	@echo 'Usage: make [target]'
	@echo ''
	@echo 'Available targets:'
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "  %-15s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

# ============================================================================
# Native Development
# ============================================================================

build: ## Build the Rust project
	cargo build --release

run: ## Run the bot (requires .env file)
	dotenvx run -- cargo run --release

test: ## Run tests
	cargo test

clean: ## Clean build artifacts
	cargo clean
	rm -rf target/

# ============================================================================
# Docker Commands
# ============================================================================

docker-build: ## Build Docker image
	docker-compose build

docker-up: ## Start Docker container
	docker-compose up -d

docker-down: ## Stop Docker container
	docker-compose down

docker-logs: ## View Docker logs
	docker-compose logs -f polymarket-bot

docker-restart: ## Restart Docker container
	docker-compose restart

docker-shell: ## Open shell in Docker container
	docker-compose exec polymarket-bot sh

docker-prod: ## Run in production mode
	docker-compose -f docker-compose.yml -f docker-compose.prod.yml up -d

# ============================================================================
# Setup
# ============================================================================

setup: ## Initial setup (create directories, copy .env.example)
	mkdir -p data logs
	@if [ ! -f .env ]; then \
		cp .env.example .env; \
		echo "Created .env file from .env.example - please edit it with your credentials"; \
	else \
		echo ".env file already exists"; \
	fi

# ============================================================================
# Development
# ============================================================================

dev: ## Run in development mode with hot reload (requires cargo-watch)
	@command -v cargo-watch >/dev/null 2>&1 || { echo "Install cargo-watch: cargo install cargo-watch"; exit 1; }
	cargo watch -x "run --release"

lint: ## Run clippy linter
	cargo clippy -- -D warnings

format: ## Format code
	cargo fmt

# ============================================================================
# Production
# ============================================================================

prod-build: ## Build optimized release
	cargo build --release --features production

prod-run: ## Run production build
	DRY_RUN=0 RUST_LOG=info ./target/release/arb-bot

