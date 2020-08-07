use super::SubProof;
use crate::{hash::branch_node_hash, nibbles::Nibbles, u4::U4};
use alloc::boxed::Box;
use serde::{Deserialize, Serialize};
use slimchain_common::{basic::H256, digest::Digestible};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub(crate) struct BranchNode {
    pub(crate) children: [Option<Box<SubProof>>; 16],
}

impl Digestible for BranchNode {
    fn to_digest(&self) -> H256 {
        let children = self
            .children
            .iter()
            .map(|c| c.as_ref().map(|n| n.to_digest()));
        branch_node_hash(children)
    }
}

impl BranchNode {
    pub(crate) fn from_hashes(children: &[Option<H256>; 16]) -> Self {
        let mut node = BranchNode::default();
        for (i, child) in children.iter().enumerate() {
            if let Some(hash) = child {
                unsafe {
                    *node.children.get_unchecked_mut(i) = Some(Box::new(SubProof::Hash(*hash)));
                }
            }
        }
        node
    }

    pub(crate) fn get_child(&self, index: U4) -> Option<&'_ SubProof> {
        let index: usize = index.into();
        unsafe { self.children.get_unchecked(index) }
            .as_ref()
            .map(|n| n.as_ref())
    }

    pub(crate) fn get_child_mut(&mut self, index: U4) -> &'_ mut Option<Box<SubProof>> {
        let index: usize = index.into();
        unsafe { self.children.get_unchecked_mut(index) }
    }

    pub(crate) fn value_hash(&self, key: Nibbles<'_>) -> Option<H256> {
        let (child_idx, remaining) = match key.split_first() {
            Some(res) => res,
            None => {
                panic!("Invalid key. Branch node does not store value.");
            }
        };

        match self.get_child(child_idx) {
            Some(child) => child.value_hash(remaining),
            None => Some(H256::zero()),
        }
    }

    pub(crate) fn search_prefix<'a>(
        &mut self,
        key: Nibbles<'a>,
    ) -> Option<(*mut SubProof, H256, Nibbles<'a>)> {
        let (child_idx, remaining) = key
            .split_first()
            .expect("Invalid key. Branch node does not store value.");
        match self.get_child_mut(child_idx) {
            Some(child) => child.search_prefix(remaining),
            None => None,
        }
    }
}
