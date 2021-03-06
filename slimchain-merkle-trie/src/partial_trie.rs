use crate::traits::Key;
use alloc::sync::Arc;
use serde::{Deserialize, Serialize};
use slimchain_common::{basic::H256, digest::Digestible};

pub(crate) mod branch;
pub(crate) use branch::*;
pub(crate) mod extension;
pub(crate) use extension::*;
pub(crate) mod leaf;
pub(crate) use leaf::*;
pub(crate) mod sub_tree;
pub(crate) use sub_tree::*;

pub mod diff;
pub use diff::*;
pub mod prune;
pub use prune::*;

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PartialTrie {
    pub(crate) root: Option<Arc<SubTree>>,
}

impl From<crate::proof::Proof> for PartialTrie {
    fn from(input: crate::proof::Proof) -> Self {
        match input.root {
            Some(root) => Self::from_subtree(Arc::new(root.into())),
            None => Self::default(),
        }
    }
}

impl From<PartialTrie> for crate::proof::Proof {
    fn from(input: PartialTrie) -> Self {
        use crate::proof::Proof;
        match input.root {
            Some(root) => Proof::from_subproof((*root).clone().into()),
            None => Proof::default(),
        }
    }
}

impl PartialTrie {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn from_subtree(root: Arc<SubTree>) -> Self {
        Self { root: Some(root) }
    }

    pub fn from_root_hash(root_hash: H256) -> Self {
        if root_hash.is_zero() {
            Self::default()
        } else {
            Self::from_subtree(Arc::new(SubTree::from_hash(root_hash)))
        }
    }

    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    pub fn root_hash(&self) -> H256 {
        match self.root.as_ref() {
            Some(root) => root.to_digest(),
            None => H256::zero(),
        }
    }

    pub fn value_hash(&self, key: &impl Key) -> Option<H256> {
        let key = key.as_nibbles();
        match self.root.as_ref() {
            Some(root) => root.value_hash(key),
            None => Some(H256::zero()),
        }
    }
}
