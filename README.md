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
