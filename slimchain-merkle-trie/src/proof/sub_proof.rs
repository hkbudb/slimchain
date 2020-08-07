use super::{BranchNode, ExtensionNode, LeafNode};
use crate::nibbles::Nibbles;
use alloc::boxed::Box;
use serde::{Deserialize, Serialize};
use slimchain_common::{basic::H256, digest::Digestible};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum SubProof {
    Hash(H256),
    Extension(Box<ExtensionNode>),
    Branch(Box<BranchNode>),
    Leaf(Box<LeafNode>),
}

impl Default for SubProof {
    fn default() -> Self {
        Self::Hash(H256::zero())
    }
}

impl Digestible for SubProof {
    fn to_digest(&self) -> H256 {
        match self {
            Self::Hash(h) => *h,
            Self::Extension(n) => n.to_digest(),
            Self::Branch(n) => n.to_digest(),
            Self::Leaf(n) => n.to_digest(),
        }
    }
}

impl SubProof {
    pub(crate) fn from_hash(h: H256) -> Self {
        Self::Hash(h)
    }

    pub(crate) fn from_extension(n: ExtensionNode) -> Self {
        Self::Extension(Box::new(n))
    }

    pub(crate) fn from_branch(n: BranchNode) -> Self {
        Self::Branch(Box::new(n))
    }

    pub(crate) fn from_leaf(n: LeafNode) -> Self {
        Self::Leaf(Box::new(n))
    }

    pub(crate) fn value_hash(&self, key: Nibbles<'_>) -> Option<H256> {
        match self {
            Self::Hash(_) => None,
            Self::Extension(n) => n.value_hash(key),
            Self::Branch(n) => n.value_hash(key),
            Self::Leaf(n) => n.value_hash(key),
        }
    }

    pub(crate) fn search_prefix<'a>(
        &mut self,
        key: Nibbles<'a>,
    ) -> Option<(*mut SubProof, H256, Nibbles<'a>)> {
        match self {
            Self::Hash(h) => {
                let hash = *h;
                Some((self as *mut _, hash, key))
            }
            Self::Extension(n) => n.search_prefix(key),
            Self::Branch(n) => n.search_prefix(key),
            Self::Leaf(_) => None,
        }
    }
}
