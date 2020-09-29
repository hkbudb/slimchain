SGX_SDK ?= /opt/intel/sgxsdk
SGX_MODE ?= HW
DEBUG ?= 0

RUST_BUILD_FLAGS :=
RUST_SDK_PATH := ../rust-sgx-sdk
SGX_COMMON_CFLAGS := -m64
SGX_LIBRARY_PATH := $(SGX_SDK)/lib64
SGX_ENCLAVE_SIGNER := $(SGX_SDK)/bin/x64/sgx_sign
SGX_EDGER8R := $(SGX_SDK)/bin/x64/sgx_edger8r

ifeq ($(DEBUG), 1)
	SGX_COMMON_CFLAGS += -O0 -g
	TARGET_DIR := target/debug
	OUTPUT_DIR := ../target/debug
else
	RUST_BUILD_FLAGS += --release
	SGX_COMMON_CFLAGS += -O2 -flto
	TARGET_DIR := target/release
	OUTPUT_DIR := ../target/release
endif

ifneq ($(SGX_MODE), HW)
	ENCLAVE_NAME := slimchain_tx_engine_tee_enclave_sim
	Trts_Library_Name := sgx_trts_sim
	Service_Library_Name := sgx_tservice_sim
else
	ENCLAVE_NAME := slimchain_tx_engine_tee_enclave
	Trts_Library_Name := sgx_trts
	Service_Library_Name := sgx_tservice
endif
Crypto_Library_Name := sgx_tcrypto

ENCLAVE_CONFIG := config.xml
ENCLAVE_EDL := enclave.edl
ENCLAVE_KEY := private.pem
ENCLAVE_LDS := enclave.lds

ENCLAVE_FILE := $(TARGET_DIR)/lib$(ENCLAVE_NAME).so
ENCLAVE_SIGNED_FILE := $(OUTPUT_DIR)/lib$(ENCLAVE_NAME).signed.so

ENCLAVE_TRUST_OBJ_NAME := enclave_t.o
ENCLAVE_TRUST_OBJ_FILE := $(TARGET_DIR)/$(ENCLAVE_TRUST_OBJ_NAME)

RUST_LIB_NAME := slimchain_tx_engine_tee_enclave_rust
RUST_LIB_FILE := $(TARGET_DIR)/lib$(RUST_LIB_NAME).a

SGX_ENCLAVE_CFLAGS := \
	$(SGX_COMMON_CFLAGS) -nostdinc -fvisibility=hidden -fpie -fstack-protector \
	-I$(RUST_SDK_PATH)/common/inc -I$(RUST_SDK_PATH)/edl -I$(SGX_SDK)/include \
	-I$(SGX_SDK)/include/tlibc -I$(SGX_SDK)/include/stlport -I$(SGX_SDK)/include/epid \
	-I$(TARGET_DIR)
SGX_ENCLAVE_LDFLAGS := \
	${SGX_COMMON_CFLAGS} -Wl,--no-undefined -nostdlib -nodefaultlibs -nostartfiles \
	-L$(SGX_LIBRARY_PATH) -Wl,--whole-archive -l$(Trts_Library_Name) -Wl,--no-whole-archive \
	-Wl,--start-group -lsgx_tstdc -l$(Service_Library_Name) -l$(Crypto_Library_Name) \
	-L$(TARGET_DIR) -l$(RUST_LIB_NAME) \
	-Wl,--end-group \
	-Wl,-Bstatic -Wl,-Bsymbolic -Wl,--no-undefined \
	-Wl,-pie,-eenclave_entry -Wl,--export-dynamic \
	-Wl,--defsym,__ImageBase=0 \
	-Wl,--gc-sections \
	-Wl,--version-script=$(ENCLAVE_LDS)

default: enclave_lib
.PHONY: default

## enclave_lib

enclave_lib: $(ENCLAVE_SIGNED_FILE)
.PHONY: enclave_lib

$(ENCLAVE_SIGNED_FILE): $(ENCLAVE_CONFIG) $(ENCLAVE_KEY) $(ENCLAVE_FILE)
	@mkdir -p $(OUTPUT_DIR)
	$(SGX_ENCLAVE_SIGNER) sign -key $(ENCLAVE_KEY) -enclave $(ENCLAVE_FILE) -out $@ -config $(ENCLAVE_CONFIG)

$(ENCLAVE_FILE): $(ENCLAVE_TRUST_OBJ_FILE) $(RUST_LIB_FILE)
	$(CXX) $< -o $@ $(SGX_ENCLAVE_LDFLAGS)

## enclave_t.o

ENCLAVE_TRUST_SRC := \
	$(TARGET_DIR)/enclave_t.c \
	$(TARGET_DIR)/enclave_t.h

enclave_trust_obj: $(ENCLAVE_TRUST_OBJ_FILE)
.PHONY: enclave_trust_obj

$(ENCLAVE_TRUST_OBJ_FILE): $(ENCLAVE_TRUST_SRC)
	$(CC) $(SGX_ENCLAVE_CFLAGS) -c $< -o $@

$(ENCLAVE_TRUST_SRC): $(ENCLAVE_EDL)
	@mkdir -p $(TARGET_DIR)
	$(SGX_EDGER8R) --trusted $(ENCLAVE_EDL) \
		--search-path $(SGX_SDK)/include --search-path $(RUST_SDK_PATH)/edl \
		--trusted-dir $(TARGET_DIR)

## rust_lib

RUST_SRC := \
	$(shell find \
		src \
		../slimchain-common/src \
		../slimchain-merkle-trie/src \
		../slimchain-tx-executor/src \
		../slimchain-tx-state/src \
		-type f -name "*.rs") \
	$(shell find \
		. \
		../slimchain-common \
		../slimchain-merkle-trie \
		../slimchain-tx-executor \
		../slimchain-tx-state \
		-type f -name "Cargo.toml")

rust_lib: $(RUST_LIB_FILE)
.PHONY: rust_lib

$(RUST_LIB_FILE): $(RUST_SRC)
	cargo build $(RUST_BUILD_FLAGS)

## clean

clean:
	-rm ./Cargo.lock
	-rm -rf target
	-rm -f ../target/*/lib$(ENCLAVE_NAME)*.signed.so
.PHONY: clean

## fmt

fmt:
	cargo fmt
.PHONY: fmt

## check-deps

check-deps:
	cargo upgrade --skip-compatible --dry-run
.PHONY: check-deps

## update-deps

update-deps:
	cargo update
.PHONY: update-deps
