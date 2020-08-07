use crate::{
    hash::leaf_node_hash,
    nibbles::{AsNibbles, NibbleBuf, Nibbles},
};
use serde::{Deserialize, Serialize};
use slimchain_common::{basic::H256, digest::Digestible};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub(crate) struct LeafNode {
    pub(crate) nibbles: NibbleBuf,
    pub(crate) value_hash: H256,
}

impl Digestible for LeafNode {
    fn to_digest(&self) -> H256 {
        leaf_node_hash(&self.nibbles, self.value_hash)
    }
}

impl LeafNode {
    pub(crate) fn new(nibbles: NibbleBuf, value_hash: H256) -> Self {
        Self {
            nibbles,
            value_hash,
        }
    }

    pub(crate) fn value_hash(&self, key: Nibbles<'_>) -> Option<H256> {
        if key == self.nibbles.as_nibbles() {
            Some(self.value_hash)
        } else {
            Some(H256::zero())
        }
    }
}
