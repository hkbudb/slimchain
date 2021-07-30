# SlimChain

**WARNING**: This is an academic proof-of-concept prototype, and in particular has not received careful code review. This implementation is NOT ready for production use.

If you find the code here useful, please consider to cite our paper:

```bibtex
@article{PVLDB:slimchain,
  author = {Xu, Cheng and Zhang, Ce and Xu, Jianliang and Pei, Jian},
  title = {{SlimChain}: Scaling Blockchain Transactions through Off-Chain Storage and Parallel Processing},
  journal = {Proceedings of the VLDB Endowment},
  year = {2021},
  month = jul,
  volume = {14},
  number = {11},
  pages = {2314--2326},
  issn = {2150-8097},
  doi = {10.14778/3476249.3476283}
}
```

## Install Dependencies

* OS: Ubuntu 18.04 LTS or Ubuntu 20.04 LTS.
* Install [Rust](https://rustup.rs).
* Run `sudo ./scripts/install_deps.sh`.
* Install [SGX Driver](https://github.com/intel/linux-sgx-driver) by running `sudo ./scripts/install_sgx_driver.sh`.
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

* Create proper `config.toml` file based on examples from `config-example`.
* See help messages on how to run nodes and send txs:

```bash
./target/release/slimchain-node-tee --help # run slimchain nodes
./target/release/slimchain-send-tx --help # send tx
./target/release/baseline-classic-node --help # run baseline (classic) nodes
./target/release/baseline-stateful-node-tee --help # run baseline (stateful) nodes
./target/release/slimchain-inspect-db --help # check storage size
```

## Adjust Proof-of-Work Difficulty

You can change the initial Proof-of-Work difficulty in the `config.toml`.

To test the difficulty:

```bash
cargo test --release -p slimchain-chain consensus::pow::tests::test_pow  -- --nocapture --exact --ignored
```
