use super::SubProof;
use crate::{
    hash::extension_node_hash,
    nibbles::{NibbleBuf, Nibbles},
};
use alloc::boxed::Box;
use serde::{Deserialize, Serialize};
use slimchain_common::{basic::H256, digest::Digestible};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub(crate) struct ExtensionNode {
    pub(crate) nibbles: NibbleBuf,
    pub(crate) child: Box<SubProof>,
}

impl Digestible for ExtensionNode {
    fn to_digest(&self) -> H256 {
        extension_node_hash(&self.nibbles, self.child.to_digest())
    }
}

impl ExtensionNode {
    pub(crate) fn new(nibbles: NibbleBuf, child: Box<SubProof>) -> Self {
        Self { nibbles, child }
    }

    pub(crate) fn value_hash(&self, key: Nibbles<'_>) -> Option<H256> {
        match key.strip_prefix(&self.nibbles) {
            Some(remaining) => self.child.value_hash(remaining),
            None => Some(H256::zero()),
        }
    }

    pub(crate) fn search_prefix<'a>(
        &mut self,
        key: Nibbles<'a>,
    ) -> Option<(*mut SubProof, H256, Nibbles<'a>)> {
        match key.strip_prefix(&self.nibbles) {
            Some(remaining) => self.child.search_prefix(remaining),
            None => None,
        }
    }
}
