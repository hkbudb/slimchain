#!/usr/bin/env bash

SGX_DRIVER_VERSION="2.11.0_4505f07"

set -e

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

curl -L "https://download.01.org/intel-sgx/latest/linux-latest/distro/ubuntu18.04-server/sgx_linux_x64_driver_$SGX_DRIVER_VERSION.bin" \
    -o /tmp/sgx_linux_x64_driver.bin
chmod +x /tmp/sgx_linux_x64_driver.bin
mkdir -p /opt/intel
/tmp/sgx_linux_x64_driver.bin
rm /tmp/sgx_linux_x64_driver.bin
