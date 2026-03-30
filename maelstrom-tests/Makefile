MAELSTROM_VERSION := 0.2.3
MAELSTROM_DIR := .maelstrom
MAELSTROM_URL := https://github.com/jepsen-io/maelstrom/releases/download/v$(MAELSTROM_VERSION)/maelstrom.tar.bz2
MAELSTROM_SRC_URL := https://github.com/jepsen-io/maelstrom/archive/refs/tags/v$(MAELSTROM_VERSION).tar.gz
MAELSTROM := $(MAELSTROM_DIR)/maelstrom/maelstrom
CARGO_TARGET := $(or $(CARGO_TARGET_DIR),$(CURDIR)/target)
BIN := $(CARGO_TARGET)/release/groupcache-maelstrom

.PHONY: build install-maelstrom test-maelstrom test-echo test-baseline test-partitions test-packet-loss test-combined serve-results clean

build:
	cargo build --release

install-maelstrom: $(MAELSTROM)

$(MAELSTROM):
	@echo "Installing Maelstrom $(MAELSTROM_VERSION)..."
	mkdir -p $(MAELSTROM_DIR)
	curl -sL $(MAELSTROM_URL) | tar xj -C $(MAELSTROM_DIR)
	@echo "Downloading Maelstrom source..."
	curl -sL $(MAELSTROM_SRC_URL) | tar xz -C $(MAELSTROM_DIR)
	@echo "Installing custom workload and checker..."
	cp maelstrom/src/maelstrom/workload/cache_convergence.clj \
		$(MAELSTROM_DIR)/maelstrom-$(MAELSTROM_VERSION)/src/maelstrom/workload/
	mkdir -p $(MAELSTROM_DIR)/maelstrom-$(MAELSTROM_VERSION)/src/maelstrom/checker
	cp maelstrom/src/maelstrom/checker/convergence.clj \
		$(MAELSTROM_DIR)/maelstrom-$(MAELSTROM_VERSION)/src/maelstrom/checker/
	@echo "Registering workload..."
	bash scripts/patch-maelstrom.sh \
		$(MAELSTROM_DIR)/maelstrom-$(MAELSTROM_VERSION)/src/maelstrom/core.clj
	@echo "Maelstrom installed and patched."

test-echo: build install-maelstrom
	$(MAELSTROM) test -w echo --bin $(BIN) --node-count 1 --time-limit 5

# Delta-t thresholds per scenario (milliseconds):
#   baseline:    100ms  — near-realtime under normal conditions
#   partitions:  35000ms — bounded by hot cache TTL (30s) during partition
#   packet-loss: 1000ms  — brief delays from dropped messages
#   combined:    35000ms — partition-dominated worst case

test-baseline: build install-maelstrom
	cd $(MAELSTROM_DIR)/maelstrom-$(MAELSTROM_VERSION) && \
	DELTA_T_MS=100 lein run test -w cache-convergence \
		--bin $(BIN) \
		--node-count 5 \
		--time-limit 30 \
		--rate 50

test-partitions: build install-maelstrom
	cd $(MAELSTROM_DIR)/maelstrom-$(MAELSTROM_VERSION) && \
	DELTA_T_MS=35000 lein run test -w cache-convergence \
		--bin $(BIN) \
		--node-count 5 \
		--time-limit 60 \
		--rate 50 \
		--nemesis partition \
		--nemesis-interval 10

test-partitions-long: build install-maelstrom
	cd $(MAELSTROM_DIR)/maelstrom-$(MAELSTROM_VERSION) && \
	DELTA_T_MS=35000 lein run test -w cache-convergence \
		--bin $(BIN) \
		--node-count 5 \
		--time-limit 120 \
		--rate 50 \
		--nemesis partition \
		--nemesis-interval 5

test-maelstrom: test-baseline test-partitions test-partitions-long
	@echo "All Maelstrom tests complete."

serve-results: install-maelstrom
	cd $(MAELSTROM_DIR)/maelstrom-$(MAELSTROM_VERSION) && lein run serve

clean:
	cargo clean
	rm -rf $(MAELSTROM_DIR)
