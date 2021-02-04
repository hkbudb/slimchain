#!/usr/bin/env bash

SGX_SDK_VERSION="2.13.100.4"

set -eo pipefail

error() {
    echo "Error: $1" >&2
    exit 1
}

if [[ "$EUID" -ne 0 ]]; then
    error "Please run as root."
fi

if [[ "$(cat /etc/issue)" != *"Ubuntu 18.04"* ]]; then
    error "Ubuntu 18.04 is required."
fi

apt-get update -y
apt-get upgrade -y
apt-get install -y build-essential autoconf libtool libssl-dev pkg-config
apt-get install -y llvm-dev libclang-dev clang # required by bindgen
apt-get install -y curl gnupg

echo "deb [arch=amd64] https://download.01.org/intel-sgx/sgx_repo/ubuntu bionic main" > /etc/apt/sources.list.d/intel-sgx.list
curl -fsSL https://download.01.org/intel-sgx/sgx_repo/ubuntu/intel-sgx-deb.key | apt-key add -
curl -sL https://deb.nodesource.com/setup_15.x | bash -

apt-get update -y
apt-get install -y libsgx-uae-service libsgx-urts sgx-aesm-service
apt-get install -y nodejs

curl -fsSL "https://download.01.org/intel-sgx/latest/linux-latest/distro/ubuntu18.04-server/sgx_linux_x64_sdk_$SGX_SDK_VERSION.bin" \
    -o /tmp/sgx_linux_x64_sdk.bin
chmod +x /tmp/sgx_linux_x64_sdk.bin
mkdir -p /opt/intel
cd /opt/intel
echo 'yes'| /tmp/sgx_linux_x64_sdk.bin
rm /tmp/sgx_linux_x64_sdk.bin

npm install -g truffle
