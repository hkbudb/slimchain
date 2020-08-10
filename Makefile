default: build-release
.PHONY: default

build:
	$(MAKE) -C contracts
	cargo build
.PHONY: build

build-release:
	$(MAKE) -C contracts
	cargo build --release
.PHONY: build-release

build-contracts:
	$(MAKE) -C contracts
.PHONY: build-contracts

test:
	RUST_LOG=info cargo test -- --nocapture
.PHONY: test

test-release:
	RUST_LOG=info cargo test --release -- --nocapture
.PHONY: test-release

clean:
	-rm -rf target
	-$(MAKE) -C contracts clean
.PHONY: clean

clippy:
	cargo clippy --tests
.PHONY: clippy

cov:
	cargo tarpaulin --all-features --workspace --exclude-files 'rust-sgx-sdk/*' -o Html --output-dir target
.PHONY: cov

check-deps:
	cargo upgrade --workspace --skip-compatible --dry-run
.PHONY: check-deps
