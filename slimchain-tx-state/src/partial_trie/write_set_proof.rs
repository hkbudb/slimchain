use crate::view::{
    trie_view_sync::{AccountTrieView, StateTrieView},
    TxStateView,
};
use alloc::{format, sync::Arc};
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{account_data_to_digest, Address, Nonce, H256},
    collections::HashMap,
    digest::Digestible,
    error::{ensure, Result},
    rw_set::TxWriteData,
};
use slimchain_merkle_trie::prelude::*;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AccountWriteSetProof {
    pub nonce: Nonce,
    pub code_hash: H256,
    pub state_proof: PartialTrie,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TxWriteSetProof {
    pub main_proof: PartialTrie,
    pub acc_proofs: HashMap<Address, AccountWriteSetProof>,
}

impl TxWriteSetProof {
    pub fn new(
        state_view: Arc<dyn TxStateView + Sync + Send>,
        root_address: H256,
        writes: &TxWriteData,
    ) -> Result<Self> {
        let mut acc_proofs: HashMap<Address, AccountWriteSetProof> = HashMap::new();
        let mut main_read_ctx =
            ReadTrieContext::new(AccountTrieView::new(Arc::clone(&state_view)), root_address);

        for (acc_address, acc_write) in writes.iter() {
            let acc_data = main_read_ctx.read(acc_address)?;
            let acc_proof = if acc_write.reset_values {
                AccountWriteSetProof {
                    nonce: acc_data.map(|acc| acc.nonce).unwrap_or_default(),
                    code_hash: acc_data.map(|acc| acc.code.to_digest()).unwrap_or_default(),
                    state_proof: PartialTrie::from_root_hash(
                        acc_data.map(|acc| acc.acc_state_root).unwrap_or_default(),
                    ),
                }
            } else {
                let mut value_read_ctx = ReadTrieContext::new(
                    StateTrieView::new(Arc::clone(&state_view), *acc_address),
                    acc_data.map(|acc| acc.acc_state_root).unwrap_or_default(),
                );

                for key in acc_write.values.keys() {
                    value_read_ctx.read(key)?;
                }

                AccountWriteSetProof {
                    nonce: acc_data.map(|acc| acc.nonce).unwrap_or_default(),
                    code_hash: acc_data.map(|acc| acc.code.to_digest()).unwrap_or_default(),
                    state_proof: value_read_ctx.into_proof().into(),
                }
            };

            acc_proofs.insert(*acc_address, acc_proof);
        }

        Ok(Self {
            main_proof: main_read_ctx.into_proof().into(),
            acc_proofs,
        })
    }

    pub fn verify(&self, state_root: H256) -> Result<()> {
        for (acc_address, acc_proof) in self.acc_proofs.iter() {
            let acc_hash = account_data_to_digest(
                acc_proof.nonce.to_digest(),
                acc_proof.code_hash,
                acc_proof.state_proof.root_hash(),
            );

            let main_proof_acc_hash = self.main_proof.value_hash(acc_address);

            ensure!(
                main_proof_acc_hash == Some(acc_hash),
                "TxWriteSetProof: Invalid account hash (address: {}, expect: {:?}, actual: {:?}).",
                acc_address,
                main_proof_acc_hash,
                Some(acc_hash)
            );
        }

        let main_proof_root = self.main_proof.root_hash();
        ensure!(
            main_proof_root == state_root,
            "TxWriteSetProof: Invalid state root (expect: {}, actual: {}).",
            state_root,
            main_proof_root
        );

        Ok(())
    }
}
