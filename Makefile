default: build-release
.PHONY: default

build:
	$(MAKE) -C contracts
	$(MAKE) -C slimchain-tx-engine-tee-enclave DEBUG=1
	cargo build
.PHONY: build

build-release:
	$(MAKE) -C contracts
	$(MAKE) -C slimchain-tx-engine-tee-enclave
	cargo build --release
.PHONY: build-release

build-contracts:
	$(MAKE) -C contracts
.PHONY: build-contracts

test:
	cargo test -- --nocapture
.PHONY: test

test-release:
	cargo test --release -- --nocapture
.PHONY: test-release

clean:
	-rm ./Cargo.lock
	-rm -rf target
	-$(MAKE) -C contracts clean
	-$(MAKE) -C slimchain-tx-engine-tee-enclave clean
.PHONY: clean

clippy:
	cargo clippy --all-targets
.PHONY: clippy

cov:
	cargo tarpaulin --all-features --workspace --exclude-files 'rust-sgx-sdk/*' -o Html --output-dir target
.PHONY: cov

check-deps:
	cargo upgrade --workspace --skip-compatible --dry-run
	$(MAKE) -C slimchain-tx-engine-tee-enclave check-deps
.PHONY: check-deps

update-deps:
	cargo update
	$(MAKE) -C slimchain-tx-engine-tee-enclave update-deps
.PHONY: update-deps

fmt:
	cargo fmt
	$(MAKE) -C slimchain-tx-engine-tee-enclave fmt
.PHONY: fmt

loc:
	tokei -e rust-sgx-sdk
.PHONY: loc
