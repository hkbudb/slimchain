FROM ubuntu:20.04

COPY ./scripts/install_deps.sh /tmp/install_deps.sh
COPY ./rust-sgx-sdk/rust-toolchain /tmp/rust-toolchain

ENV SGX_SDK="/opt/intel/sgxsdk"
ENV PKG_CONFIG_PATH="${PKG_CONFIG_PATH}:${SGX_SDK}/pkgconfig"
ENV LD_LIBRARY_PATH="${SGX_SDK}/sdk_libs"
ENV PATH="/root/.cargo/bin:${PATH}:${SGX_SDK}/bin:${SGX_SDK}/bin/x64"

RUN chmod +x /tmp/install_deps.sh && \
    /tmp/install_deps.sh && \
    rm /tmp/install_deps.sh && \
    rm -rf /var/lib/apt/lists/* && \
    rm -rf /var/cache/apt/archives/*

RUN curl -sSf https://sh.rustup.rs | sh -s -- --profile minimal --default-toolchain none -y
RUN RUST_TOOLCHAIN=$(cat /tmp/rust-toolchain) && \
    rustup toolchain install $RUST_TOOLCHAIN && \
    rustup component add clippy rustfmt --toolchain $RUST_TOOLCHAIN && \
    rm /tmp/rust-toolchain
