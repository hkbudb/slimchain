use crate::basic::{Code, Nonce, H256};
use crate::digest::{blake2b_hash_to_h256, default_blake2, Digestible};

#[derive(Debug, Default, Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AccountData {
    pub nonce: Nonce,
    pub code: Code,
    pub acc_state_root: H256,
}

pub fn account_data_to_digest(nonce_hash: H256, code_hash: H256, acc_state_root: H256) -> H256 {
    if nonce_hash.is_zero() && code_hash.is_zero() && acc_state_root.is_zero() {
        H256::zero()
    } else {
        let mut hash_state = default_blake2().to_state();
        hash_state.update(nonce_hash.as_bytes());
        hash_state.update(code_hash.as_bytes());
        hash_state.update(acc_state_root.as_bytes());
        blake2b_hash_to_h256(hash_state.finalize())
    }
}

impl Digestible for AccountData {
    fn to_digest(&self) -> H256 {
        account_data_to_digest(
            self.nonce.to_digest(),
            self.code.to_digest(),
            self.acc_state_root,
        )
    }
}
