pub mod propose;
pub use propose::*;

pub mod verify;
pub use verify::*;

pub mod commit;
pub use commit::*;

use crate::db::DBPtr;
use slimchain_common::{
    basic::{AccountData, Address, Code, Nonce, StateKey, StateValue, H256},
    error::Result,
    tx_req::SignedTxRequest,
};
use slimchain_merkle_trie::prelude::*;
use slimchain_tx_executor::execute_tx;
use slimchain_tx_state::{
    trie_view::{AccountTrieView, StateTrieView},
    update_tx_state, TxStateUpdate, TxStateView, TxStateViewWithUpdate,
};

struct ExecutorBackend<'a, StateView: TxStateView + ?Sized> {
    state_view: &'a StateView,
    state_root: H256,
}

impl<'a, StateView: TxStateView + ?Sized> ExecutorBackend<'a, StateView> {
    fn new(state_view: &'a StateView, state_root: H256) -> Self {
        Self {
            state_view,
            state_root,
        }
    }

    fn map_acc_data<T>(
        &self,
        acc_address: Address,
        default: impl FnOnce() -> T,
        f: impl FnOnce(&AccountData) -> T,
    ) -> Result<T> {
        let view = AccountTrieView::new(self.state_view);
        let acc_data = read_trie_without_proof(&view, self.state_root, &acc_address)?;
        Ok(acc_data.as_ref().map_or_else(default, f))
    }
}

impl<'a, StateView: TxStateView + ?Sized> slimchain_tx_executor::Backend
    for ExecutorBackend<'a, StateView>
{
    fn get_nonce(&self, acc_address: Address) -> Result<Nonce> {
        self.map_acc_data(acc_address, Default::default, |d| d.nonce)
    }

    fn get_code(&self, acc_address: Address) -> Result<Code> {
        self.map_acc_data(acc_address, Default::default, |d| d.code.clone())
    }

    fn get_value(&self, acc_address: Address, key: StateKey) -> Result<StateValue> {
        let acc_state_root = self.map_acc_data(acc_address, H256::zero, |d| d.acc_state_root)?;

        let view = StateTrieView::new(self.state_view, acc_address);
        let value = read_trie_without_proof(&view, acc_state_root, &key)?.unwrap_or_default();
        Ok(value)
    }
}

pub(crate) fn exec_tx(
    db: &DBPtr,
    pending_update: &TxStateUpdate,
    signed_tx_req: &SignedTxRequest,
) -> Result<TxStateUpdate> {
    let state_root = pending_update.root;
    let state_view = TxStateViewWithUpdate::new(db, pending_update);
    let backend = ExecutorBackend::new(&state_view, state_root);
    let output = execute_tx(signed_tx_req.clone(), &backend)?;
    let new_update = update_tx_state(&state_view, state_root, &output.writes)?;

    let mut update = pending_update.clone();
    update.merge(new_update);
    Ok(update)
}
