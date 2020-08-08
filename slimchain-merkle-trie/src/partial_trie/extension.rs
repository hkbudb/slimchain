use super::SubTree;
use crate::{
    hash::extension_node_hash,
    nibbles::{NibbleBuf, Nibbles},
};
use alloc::{boxed::Box, sync::Arc};
use core::cell::Cell;
use serde::{Deserialize, Serialize};
use slimchain_common::{basic::H256, digest::Digestible};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub(crate) struct ExtensionNode {
    pub(crate) nibbles: NibbleBuf,
    pub(crate) child: Arc<SubTree>,
    #[serde(skip)]
    node_hash: Cell<Option<H256>>,
}

impl Digestible for ExtensionNode {
    fn to_digest(&self) -> H256 {
        if let Some(h) = self.node_hash.get() {
            return h;
        }

        let h = extension_node_hash(&self.nibbles, self.child.to_digest());
        self.node_hash.set(Some(h));
        h
    }
}

impl From<crate::proof::ExtensionNode> for ExtensionNode {
    fn from(input: crate::proof::ExtensionNode) -> Self {
        Self::new(input.nibbles, Arc::new((*input.child).into()))
    }
}

impl Into<crate::proof::ExtensionNode> for ExtensionNode {
    fn into(self) -> crate::proof::ExtensionNode {
        crate::proof::ExtensionNode::new(self.nibbles, Box::new((*self.child).clone().into()))
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
            node_hash: Cell::new(None),
        }
    }

    pub(crate) fn value_hash(&self, key: Nibbles<'_>) -> Option<H256> {
        match key.strip_prefix(&self.nibbles) {
            Some(remaining) => self.child.value_hash(remaining),
            None => Some(H256::zero()),
        }
    }
}
