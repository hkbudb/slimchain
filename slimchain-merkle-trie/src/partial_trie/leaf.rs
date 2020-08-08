use crate::{
    hash::leaf_node_hash,
    nibbles::{AsNibbles, NibbleBuf, Nibbles},
};
use core::cell::Cell;
use serde::{Deserialize, Serialize};
use slimchain_common::{basic::H256, digest::Digestible};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub(crate) struct LeafNode {
    pub(crate) nibbles: NibbleBuf,
    pub(crate) value_hash: H256,
    #[serde(skip)]
    node_hash: Cell<Option<H256>>,
}

impl Digestible for LeafNode {
    fn to_digest(&self) -> H256 {
        if let Some(h) = self.node_hash.get() {
            return h;
        }

        let h = leaf_node_hash(&self.nibbles, self.value_hash);
        self.node_hash.set(Some(h));
        h
    }
}

impl From<crate::proof::LeafNode> for LeafNode {
    fn from(input: crate::proof::LeafNode) -> Self {
        Self::new(input.nibbles, input.value_hash)
    }
}

impl Into<crate::proof::LeafNode> for LeafNode {
    fn into(self) -> crate::proof::LeafNode {
        crate::proof::LeafNode::new(self.nibbles, self.value_hash)
    }
}

impl PartialEq for LeafNode {
    fn eq(&self, other: &Self) -> bool {
        self.nibbles == other.nibbles && self.value_hash == other.value_hash
    }
}

impl Eq for LeafNode {}

impl LeafNode {
    pub(crate) fn new(nibbles: NibbleBuf, value_hash: H256) -> Self {
        Self {
            nibbles,
            value_hash,
            node_hash: Cell::new(None),
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
