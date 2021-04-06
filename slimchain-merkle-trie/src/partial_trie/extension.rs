use super::SubTree;
use crate::{
    hash::extension_node_hash,
    nibbles::{NibbleBuf, Nibbles},
};
use alloc::{boxed::Box, sync::Arc};
#[cfg(feature = "cache_hash")]
use crossbeam_utils::atomic::AtomicCell;
use serde::{Deserialize, Serialize};
use slimchain_common::{basic::H256, digest::Digestible};

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct ExtensionNode {
    pub(crate) nibbles: NibbleBuf,
    pub(crate) child: Arc<SubTree>,
    #[cfg(feature = "cache_hash")]
    #[serde(skip)]
    node_hash: AtomicCell<Option<H256>>,
}

impl Digestible for ExtensionNode {
    fn to_digest(&self) -> H256 {
        #[cfg(feature = "cache_hash")]
        if let Some(h) = self.node_hash.load() {
            return h;
        }

        let h = extension_node_hash(&self.nibbles, self.child.to_digest());
        #[cfg(feature = "cache_hash")]
        self.node_hash.store(Some(h));
        h
    }
}

impl Clone for ExtensionNode {
    fn clone(&self) -> Self {
        Self {
            nibbles: self.nibbles.clone(),
            child: self.child.clone(),
            #[cfg(feature = "cache_hash")]
            node_hash: AtomicCell::new(self.node_hash.load()),
        }
    }
}

impl From<crate::proof::ExtensionNode> for ExtensionNode {
    fn from(input: crate::proof::ExtensionNode) -> Self {
        Self::new(input.nibbles, Arc::new((*input.child).into()))
    }
}

impl From<ExtensionNode> for crate::proof::ExtensionNode {
    fn from(input: ExtensionNode) -> Self {
        crate::proof::ExtensionNode::new(input.nibbles, Box::new((*input.child).clone().into()))
    }
}

impl PartialEq for ExtensionNode {
    fn eq(&self, other: &Self) -> bool {
        self.nibbles == other.nibbles && self.child == other.child
    }
}

impl Eq for ExtensionNode {}

impl ExtensionNode {
    pub(crate) fn new(nibbles: NibbleBuf, child: Arc<SubTree>) -> Self {
        debug_assert!(!nibbles.is_empty());
        Self {
            nibbles,
            child,
            #[cfg(feature = "cache_hash")]
            node_hash: AtomicCell::new(None),
        }
    }

    pub(crate) fn value_hash(&self, key: Nibbles<'_>) -> Option<H256> {
        match key.strip_prefix(&self.nibbles) {
            Some(remaining) => self.child.value_hash(remaining),
            None => Some(H256::zero()),
        }
    }
}
