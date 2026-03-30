# groupcache — Makefile
# Run `make help` to see all available targets.

.PHONY: help build check test test-all clippy fmt lint bench doc clean \
        coverage coverage-html coverage-lcov coverage-summary \
        maelstrom-test maelstrom-baseline maelstrom-partitions

# ── Build & Check ────────────────────────────────────────────────────

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}'

build: ## Build all workspace crates
	cargo build --workspace

build-release: ## Build optimized release
	cargo build --workspace --release

check: ## Type-check without building
	cargo check --workspace --all-targets

# ── Testing ──────────────────────────────────────────────────────────

test: ## Run all tests
	cargo test --workspace

test-lib: ## Run only library unit + integration tests
	cargo test -p groupcache

test-integration: ## Run only integration tests
	cargo test -p groupcache --test integration_test

test-metrics: ## Run only metrics tests
	cargo test -p groupcache --test metrics_test

test-bincode: ## Run tests with bincode feature enabled
	cargo test -p groupcache --features bincode --lib --tests

test-all: test test-bincode ## Run all tests including feature variants

# ── Code Quality ─────────────────────────────────────────────────────

clippy: ## Run clippy with warnings as errors
	cargo clippy --workspace --all-targets -- -D warnings

fmt: ## Format code
	cargo fmt --all

fmt-check: ## Check formatting without modifying
	cargo fmt --all -- --check

lint: clippy fmt-check ## Run all lints (clippy + format check)

# ── Benchmarks ───────────────────────────────────────────────────────

bench: ## Run all benchmarks
	cargo bench -p groupcache

bench-codec: ## Run serialization benchmarks (msgpack vs bincode)
	cargo bench -p groupcache --bench codec_bench

bench-routing: ## Run routing contention benchmarks (RwLock vs ArcSwap)
	cargo bench -p groupcache --bench routing_contention

# ── Coverage ─────────────────────────────────────────────────────────

coverage: coverage-summary ## Show coverage summary (alias)

coverage-summary: ## Show per-file coverage summary (merged default + bincode runs)
	cargo llvm-cov clean --workspace
	cargo llvm-cov --no-report --workspace
	cargo llvm-cov --no-report -p groupcache --features bincode --lib --tests
	cargo llvm-cov report --summary-only

coverage-text: ## Show line-by-line coverage in terminal
	cargo llvm-cov clean --workspace
	cargo llvm-cov --no-report --workspace
	cargo llvm-cov --no-report -p groupcache --features bincode --lib --tests
	cargo llvm-cov report --text

coverage-html: ## Generate HTML coverage report and open in browser
	cargo llvm-cov clean --workspace
	cargo llvm-cov --no-report --workspace
	cargo llvm-cov --no-report -p groupcache --features bincode --lib --tests
	cargo llvm-cov report --html --open

coverage-lcov: ## Generate LCOV report (for CI integration)
	cargo llvm-cov clean --workspace
	cargo llvm-cov --no-report --workspace
	cargo llvm-cov --no-report -p groupcache --features bincode --lib --tests
	cargo llvm-cov report --lcov --output-path lcov.info
	@echo "Written to lcov.info"

# ── Documentation ────────────────────────────────────────────────────

doc: ## Build and open documentation
	cargo doc --workspace --no-deps --open

# ── Maelstrom Convergence Tests ──────────────────────────────────────

maelstrom-test: ## Run all Maelstrom convergence test scenarios
	$(MAKE) -C maelstrom-tests test-maelstrom

maelstrom-baseline: ## Run Maelstrom baseline (no faults)
	$(MAKE) -C maelstrom-tests test-baseline

maelstrom-partitions: ## Run Maelstrom partition test
	$(MAKE) -C maelstrom-tests test-partitions

maelstrom-results: ## View Maelstrom results in browser
	$(MAKE) -C maelstrom-tests serve-results

# ── Cleanup ──────────────────────────────────────────────────────────

clean: ## Remove build artifacts
	cargo clean
	rm -f lcov.info
