use serde_json::Value as JsonValue;
use sha3::{Digest, Keccak256};
use slimchain_common::{
    basic::{Address, Code, Nonce, H160, H256, U256},
    collections::HashMap,
    error::{Context as _, Result},
};
use std::{fs::File, io::BufReader, path::Path};

pub use ethabi::{self, Function, Token};

// Ref: https://github.com/rust-blockchain/evm/blob/60f4020ab38dc8f21311e44f0f4174192bb1769d/src/executor/stack.rs#L328-L334
pub fn contract_address(creator: Address, nonce: Nonce) -> Address {
    let creator: H160 = creator.into();
    let nonce: U256 = nonce.into();
    let mut stream = rlp::RlpStream::new_list(2);
    stream.append(&creator);
    stream.append(&nonce);
    let address: H160 = H256::from_slice(Keccak256::digest(&stream.out()).as_slice()).into();
    address.into()
}

#[derive(Debug)]
pub struct Contract {
    code: Code,
    funcs: HashMap<String, Function>,
}

impl Contract {
    pub fn from_json_file(file: &Path) -> Result<Self> {
        let reader = BufReader::new(File::open(file)?);
        Self::from_json_value(serde_json::from_reader(reader)?)
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        Self::from_json_value(serde_json::from_slice(data)?)
    }

    pub fn from_json_value(json_data: JsonValue) -> Result<Self> {
        let bytecode = json_data["bytecode"]
            .as_str()
            .context("Failed to read `bytecode`.")?;
        let code = hex::decode(&bytecode[2..])?.into();

        let mut funcs = HashMap::new();
        let abi_data = json_data["abi"]
            .as_array()
            .context("Failed to read `abi`.")?;

        for abi in abi_data {
            if abi["type"] == "function" {
                let func: Function =
                    serde_json::from_value(abi.clone()).context("Failed to decode abi.")?;
                funcs.insert(func.name.clone(), func);
            }
        }

        Ok(Self { code, funcs })
    }

    pub fn code(&self) -> &Code {
        &self.code
    }

    pub fn encode_tx_input(&self, name: &str, args: &[Token]) -> Result<Vec<u8>> {
        self.funcs
            .get(name)
            .context("Failed to find function.")?
            .encode_input(args)
            .context("Failed to encode inputs.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slimchain_common::create_address;
    use std::path::PathBuf;

    #[test]
    fn test_contract_address() {
        let creator = create_address!("29ed001a09cd53e21e50a027f47b66f8e034534a");
        let expect = create_address!("334174c99836bcc7c983b4fa13d702407354f003");
        let actual = contract_address(creator, U256::from(0).into());
        assert_eq!(expect, actual);
    }

    #[test]
    fn test_encode_tx_input() {
        let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("contracts/build/contracts/SimpleStorage.json");
        let contract = Contract::from_json_file(&file).unwrap();
        let args = [Token::Uint(U256::from(1)), Token::Uint(U256::from(43))];
        let encoded_input = contract.encode_tx_input("set", &args).unwrap();
        let expect = hex::decode("1ab06ee50000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002b").unwrap();
        assert_eq!(encoded_input, expect);
    }
}
