use super::{TxTrieDiff, TxWriteSetTrie};
use crate::write::TxStateUpdate;
use slimchain_common::{
    basic::{Address, StateKey, H256},
    error::Result,
    rw_set::TxWriteData,
};

pub trait TxTrieTrait: Clone + Send + Sync {
    fn root_hash(&self) -> H256;
    fn update_missing_branches(&mut self, fork: &TxWriteSetTrie) -> Result<()>;
    fn apply_diff(&mut self, diff: &TxTrieDiff, check_hash: bool) -> Result<()>;
    fn apply_writes(&mut self, writes: &TxWriteData) -> Result<TxStateUpdate>;

    fn prune_account(
        &mut self,
        acc_addr: Address,
        other_acc_addr: impl Iterator<Item = Address>,
    ) -> Result<()>;
    fn prune_acc_state_key(
        &mut self,
        acc_addr: Address,
        key: StateKey,
        other_keys: impl Iterator<Item = StateKey>,
    ) -> Result<()>;
}
