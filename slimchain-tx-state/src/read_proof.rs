use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{account_data_to_digest, Address, Nonce, StateValue, H256},
    collections::HashMap,
    error::{anyhow, ensure, Result},
    rw_set::TxReadData,
};
use slimchain_merkle_trie::prelude::*;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AccountReadProof {
    pub nonce: Nonce,
    pub code_hash: H256,
    pub state_read_proof: Proof,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TxReadProof {
    pub main_proof: Proof,
    pub acc_proofs: HashMap<Address, AccountReadProof>,
}

impl TxReadProof {
    pub fn verify(&self, read_data: &TxReadData, state_root: H256) -> Result<()> {
        for (acc_address, acc_reads) in read_data.iter() {
            let acc_proof = self.acc_proofs.get(acc_address).ok_or_else(|| {
                anyhow!(
                    "TxReadProof: Account proof unavailable (address: {}).",
                    acc_address
                )
            })?;

            if let Some(nonce) = acc_reads.nonce {
                ensure!(
                    nonce == acc_proof.nonce,
                    "TxReadProof: Invalid nonce (address: {}, expect: {}, actual: {}).",
                    acc_address,
                    acc_proof.nonce,
                    nonce
                );
            }

            if let Some(code) = &acc_reads.code {
                let code_hash = code.to_digest();
                ensure!(
                    code_hash == acc_proof.code_hash,
                    "TxReadProof: Invalid code (address: {}, expect: {}, actual: {}).",
                    acc_address,
                    acc_proof.code_hash,
                    code_hash
                );
            }

            for (k, v) in acc_reads.values.iter() {
                let value = acc_proof.state_read_proof.value_hash(k).map(StateValue);
                ensure!(
                    value == Some(*v),
                    "TxReadProof: Invalid value (address: {}, key:{}, expect: {:?}, actual: {:?}).",
                    acc_address,
                    k,
                    value,
                    Some(*v)
                );
            }

            let acc_state_root = acc_proof.state_read_proof.root_hash();
            let acc_hash = account_data_to_digest(
                acc_proof.nonce.to_digest(),
                acc_proof.code_hash,
                acc_state_root,
            );
            let main_proof_acc_hash = self.main_proof.value_hash(acc_address);

            ensure!(
                main_proof_acc_hash == Some(acc_hash),
                "TxReadProof: Invalid account hash (address: {}, expect: {:?}, actual: {:?}).",
                acc_address,
                main_proof_acc_hash,
                Some(acc_hash)
            );
        }

        let main_proof_root = self.main_proof.root_hash();
        ensure!(
            main_proof_root == state_root,
            "TxReadProof: Invalid state root (expect: {}, actual: {}).",
            state_root,
            main_proof_root
        );

        Ok(())
    }
}
