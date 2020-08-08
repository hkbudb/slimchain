use super::{BranchNode, ExtensionNode, LeafNode};
use crate::nibbles::Nibbles;
use alloc::boxed::Box;
use serde::{Deserialize, Serialize};
use slimchain_common::{basic::H256, digest::Digestible};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum SubTree {
    Hash(H256),
    Extension(Box<ExtensionNode>),
    Branch(Box<BranchNode>),
    Leaf(Box<LeafNode>),
}

impl Default for SubTree {
    fn default() -> Self {
        Self::Hash(H256::zero())
    }
}

impl Digestible for SubTree {
    fn to_digest(&self) -> H256 {
        match self {
            Self::Hash(h) => *h,
            Self::Extension(n) => n.to_digest(),
            Self::Branch(n) => n.to_digest(),
            Self::Leaf(n) => n.to_digest(),
        }
    }
}

impl From<crate::proof::SubProof> for SubTree {
    fn from(input: crate::proof::SubProof) -> Self {
        use crate::proof::SubProof;
        match input {
            SubProof::Hash(h) => Self::from_hash(h),
            SubProof::Extension(n) => Self::from_extension((*n).into()),
            SubProof::Branch(n) => Self::from_branch((*n).into()),
            SubProof::Leaf(n) => Self::from_leaf((*n).into()),
        }
    }
}

impl Into<crate::proof::SubProof> for SubTree {
    fn into(self) -> crate::proof::SubProof {
        use crate::proof::SubProof;
        match self {
            Self::Hash(h) => SubProof::from_hash(h),
            Self::Extension(n) => SubProof::from_extension((*n).into()),
            Self::Branch(n) => SubProof::from_branch((*n).into()),
            Self::Leaf(n) => SubProof::from_leaf((*n).into()),
        }
    }
}

impl SubTree {
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

    pub(crate) fn can_be_pruned(&self) -> bool {
        match self {
            Self::Hash(_) => true,
            _ => false,
        }
    }
}
