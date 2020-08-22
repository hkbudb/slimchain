use super::TxWriteSetPartialTrie;
use serde::{Deserialize, Serialize};
use slimchain_common::tx::TxTrait;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxProposal<Tx: TxTrait> {
    pub tx: Tx,
    pub write_trie: TxWriteSetPartialTrie,
}

impl<Tx: TxTrait> TxProposal<Tx> {
    pub fn new(tx: Tx, write_trie: TxWriteSetPartialTrie) -> Self {
        Self { tx, write_trie }
    }
}
