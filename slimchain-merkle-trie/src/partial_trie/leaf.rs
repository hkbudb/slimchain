use crate::{
    hash::leaf_node_hash,
    nibbles::{AsNibbles, NibbleBuf, Nibbles},
};
use crossbeam_utils::atomic::AtomicCell;
use serde::{Deserialize, Serialize};
use slimchain_common::{basic::H256, digest::Digestible};

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct LeafNode {
    pub(crate) nibbles: NibbleBuf,
    pub(crate) value_hash: H256,
    #[serde(skip)]
    node_hash: AtomicCell<Option<H256>>,
}

impl Digestible for LeafNode {
    fn to_digest(&self) -> H256 {
        if let Some(h) = self.node_hash.load() {
            return h;
        }

        let h = leaf_node_hash(&self.nibbles, self.value_hash);
        self.node_hash.store(Some(h));
        h
    }
}

impl Clone for LeafNode {
    fn clone(&self) -> Self {
        Self {
            nibbles: self.nibbles.clone(),
            value_hash: self.value_hash,
            node_hash: AtomicCell::new(self.node_hash.load()),
        }
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
            node_hash: AtomicCell::new(None),
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
