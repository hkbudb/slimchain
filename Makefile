default: build-release
.PHONY: default

build:
	cargo build
.PHONY: build

build-release:
	cargo build --release
.PHONY: build-release

test:
	RUST_LOG=info cargo test -- --nocapture
.PHONY: test

test-release:
	RUST_LOG=info cargo test --release -- --nocapture
.PHONY: test-release

clippy:
	cargo clippy --tests
.PHONY: clippy

cov:
	cargo tarpaulin --all-features --workspace --exclude-files 'rust-sgx-sdk/*' -o Html --out-dir target
.PHONY: cov

check-deps:
	cargo upgrade --workspace --skip-compatible --dry-run
.PHONY: check-deps
