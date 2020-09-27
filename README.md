# SlimChain

## Install Dependencies

* OS: Ubuntu 18.04 LTS.
* Install [Rust](https://rustup.rs).
* Run `sudo ./scripts/install_deps.sh`.
* Install [SGX Driver](https://github.com/intel/linux-sgx-driver).
* Enable SGX Aesm service: `sudo /opt/intel/sgx-aesm-service/startup.sh`.

## Build and Test

* Create a minimal `config.toml` file in the root directory with the following content:

```toml
# Obtain keys from https://api.portal.trustedservices.intel.com/EPID-attestation
[tee]
# Subscription Key that provides access to the Intel API
api_key = "YOUR_API_KEY"
# Service Provider ID (SPID)
spid = "YOUR_SPID"
# Whether to sign linkable quote
linkable = false
```

* Run the following commands.

```bash
make
make test-release
```

## Run Nodes

* Create proper `config.toml` file based on `config.toml.example`.
* See help messages on how to run nodes and send txs:

```bash
./target/release/slimchain-node-tee --help
./target/release/slimchain-send-tx --help
```

## Adjust Proof-of-Work Difficulty

You can change the initial Proof-of-Work difficulty in the `config.toml`/`baseline_config.toml`.

To test the difficulty:

```bash
cargo test --release -p slimchain-chain consensus::pow::tests::test_pow  -- --nocapture --exact --ignored
```
