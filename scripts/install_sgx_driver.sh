#!/usr/bin/env bash

DCAP_SGX_DRIVER_VERSION="1.41"
OOT_SGX_DRIVER_VERSION="2.11.0_0373e2e"

set -eo pipefail

error() {
    echo "Error: $1" >&2
    exit 1
}

if [[ "$EUID" -ne 0 ]]; then
    error "Please run as root."
fi

RELEASE_INFO="$(cat /etc/issue)"
if [[ "$RELEASE_INFO" = *"Ubuntu 18.04"* ]]; then
    OS="Ubuntu-18.04"
    DCAP_SGX_DERIVER_URL="https://download.01.org/intel-sgx/latest/linux-latest/distro/ubuntu18.04-server/sgx_linux_x64_driver_$DCAP_SGX_DRIVER_VERSION.bin"
    OOT_SGX_DERIVER_URL="https://download.01.org/intel-sgx/latest/linux-latest/distro/ubuntu18.04-server/sgx_linux_x64_driver_$OOT_SGX_DRIVER_VERSION.bin"
elif [[ "$RELEASE_INFO" = *"Ubuntu 20.04"* ]]; then
    OS="Ubuntu-20.04"
    DCAP_SGX_DERIVER_URL="https://download.01.org/intel-sgx/latest/linux-latest/distro/ubuntu20.04-server/sgx_linux_x64_driver_$DCAP_SGX_DRIVER_VERSION.bin"
    OOT_SGX_DERIVER_URL="https://download.01.org/intel-sgx/latest/linux-latest/distro/ubuntu20.04-server/sgx_linux_x64_driver_$OOT_SGX_DRIVER_VERSION.bin"
else
    error "Ubuntu 18.04 or Ubuntu 20.04 is required."
fi

apt-get update -y
apt-get install -y dkms

if [[ -x /opt/intel/sgx-aesm-service/cleanup.sh ]]; then
    /opt/intel/sgx-aesm-service/cleanup.sh
fi

echo "install $DCAP_SGX_DERIVER_URL..."
rm -f /tmp/sgx_linux_x64_driver.bin
curl -fsSL "$DCAP_SGX_DERIVER_URL" -o /tmp/sgx_linux_x64_driver.bin
chmod +x /tmp/sgx_linux_x64_driver.bin
mkdir -p /opt/intel
/tmp/sgx_linux_x64_driver.bin
rm -f /tmp/sgx_linux_x64_driver.bin

echo "install $OOT_SGX_DERIVER_URL..."
rm -f /tmp/sgx_linux_x64_driver.bin
curl -fsSL "$OOT_SGX_DERIVER_URL" -o /tmp/sgx_linux_x64_driver.bin
chmod +x /tmp/sgx_linux_x64_driver.bin
mkdir -p /opt/intel
/tmp/sgx_linux_x64_driver.bin
rm -f /tmp/sgx_linux_x64_driver.bin

if [[ -x /opt/intel/sgx-aesm-service/startup.sh ]]; then
    /opt/intel/sgx-aesm-service/startup.sh
fi
