use crate::traits::Key;
use serde::{Deserialize, Serialize};
use slimchain_common::{basic::H256, digest::Digestible};

pub(crate) mod branch;
pub(crate) use branch::*;
pub(crate) mod extension;
pub(crate) use extension::*;
pub(crate) mod leaf;
pub(crate) use leaf::*;
pub(crate) mod sub_proof;
pub(crate) use sub_proof::*;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Proof {
    pub(crate) root: Option<SubProof>,
}

impl Proof {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn from_subproof(root: SubProof) -> Self {
        Self { root: Some(root) }
    }

    pub fn from_root_hash(root_hash: H256) -> Self {
        if root_hash.is_zero() {
            Self::default()
        } else {
            Self::from_subproof(SubProof::from_hash(root_hash))
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
