use super::TxWriteSetTrie;
use serde::{Deserialize, Serialize};
use slimchain_common::tx::TxTrait;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxProposal<Tx: TxTrait> {
    pub tx: Tx,
    pub write_trie: TxWriteSetTrie,
}

impl<Tx: TxTrait> TxProposal<Tx> {
    pub fn new(tx: Tx, write_trie: TxWriteSetTrie) -> Self {
        Self { tx, write_trie }
    }
}
