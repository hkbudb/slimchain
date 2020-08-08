use crate::{
    read_proof::{AccountReadProof, TxReadProof},
    view::{
        trie_view_sync::{AccountTrieView, StateTrieView},
        TxStateView,
    },
};
use alloc::sync::Arc;
use slimchain_common::{
    basic::{AccountData, Address, Code, Nonce, StateKey, StateValue, H256},
    collections::{hash_map::Entry, HashMap},
    error::Result,
};
use slimchain_merkle_trie::prelude::*;

pub struct TxStateReadContext {
    state_view: Arc<dyn TxStateView + Sync + Send>,
    main_read_ctx: ReadTrieContext<Address, AccountData, AccountTrieView>,
    values_read_ctx: HashMap<Address, ReadTrieContext<StateKey, StateValue, StateTrieView>>,
}

impl TxStateReadContext {
    pub fn new(state_view: Arc<dyn TxStateView + Sync + Send>, root_address: H256) -> Self {
        Self {
            state_view: Arc::clone(&state_view),
            main_read_ctx: ReadTrieContext::new(AccountTrieView::new(state_view), root_address),
            values_read_ctx: HashMap::new(),
        }
    }

    pub fn get_account(&mut self, acc_address: Address) -> Result<Option<&'_ AccountData>> {
        self.main_read_ctx.read(&acc_address)
    }

    pub fn get_nonce(&mut self, acc_address: Address) -> Result<Nonce> {
        let acc_data = self.get_account(acc_address)?;
        Ok(acc_data.map(|d| d.nonce).unwrap_or_default())
    }

    pub fn get_code(&mut self, acc_address: Address) -> Result<Code> {
        let acc_data = self.get_account(acc_address)?;
        Ok(acc_data.map(|d| d.code.clone()).unwrap_or_default())
    }

    pub fn get_code_len(&mut self, acc_address: Address) -> Result<usize> {
        let acc_data = self.get_account(acc_address)?;
        Ok(acc_data.map(|d| d.code.len()).unwrap_or_default())
    }

    pub fn get_value(&mut self, acc_address: Address, key: StateKey) -> Result<StateValue> {
        Ok(match self.values_read_ctx.entry(acc_address) {
            Entry::Occupied(mut entry) => entry.get_mut().read(&key)?.copied().unwrap_or_default(),
            Entry::Vacant(entry) => {
                let acc_data = self.main_read_ctx.read(&acc_address)?;
                let acc_state_root = acc_data.map(|d| d.acc_state_root).unwrap_or_default();

                let ctx = ReadTrieContext::<StateKey, _, _>::new(
                    StateTrieView::new(Arc::clone(&self.state_view), acc_address),
                    acc_state_root,
                );
                entry.insert(ctx).read(&key)?.copied().unwrap_or_default()
            }
        })
    }

    pub fn generate_proof(&self) -> Result<TxReadProof> {
        let mut acc_proofs: HashMap<Address, AccountReadProof> = HashMap::new();
        for (acc_address, acc_data) in self.main_read_ctx.get_cache().iter() {
            match acc_data.as_ref() {
                Some(acc_data) => {
                    let acc_proof = AccountReadProof {
                        nonce: acc_data.nonce,
                        code_hash: acc_data.code.to_digest(),
                        state_read_proof: self.values_read_ctx.get(acc_address).map_or_else(
                            || Proof::from_root_hash(acc_data.acc_state_root),
                            |ctx| ctx.get_proof().clone(),
                        ),
                    };
                    acc_proofs.insert(*acc_address, acc_proof);
                }
                None => {
                    acc_proofs.insert(*acc_address, AccountReadProof::default());
                }
            }
        }

        Ok(TxReadProof {
            main_proof: self.main_read_ctx.get_proof().clone(),
            acc_proofs,
        })
    }
}
