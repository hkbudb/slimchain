use crate::{hash::*, nibbles::NibbleBuf, traits::Value, u4::U4};
use alloc::boxed::Box;
use serde::{Deserialize, Serialize};
use slimchain_common::{basic::H256, digest::Digestible, error::Result};

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExtensionNode {
    pub nibbles: NibbleBuf,
    pub child: H256,
}

impl Digestible for ExtensionNode {
    fn to_digest(&self) -> H256 {
        extension_node_hash(&self.nibbles, self.child)
    }
}

impl ExtensionNode {
    pub fn new(nibbles: NibbleBuf, child: H256) -> Self {
        Self { nibbles, child }
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct BranchNode {
    pub children: [Option<H256>; 16],
}

impl Digestible for BranchNode {
    fn to_digest(&self) -> H256 {
        branch_node_hash(self.children.iter().copied())
    }
}

impl BranchNode {
    pub fn new(children: [Option<H256>; 16]) -> Self {
        Self { children }
    }

    pub fn get_child(&self, index: U4) -> Option<H256> {
        let index: usize = index.into();
        *unsafe { self.children.get_unchecked(index) }
    }

    pub fn get_child_mut(&mut self, index: U4) -> &'_ mut Option<H256> {
        let index: usize = index.into();
        unsafe { self.children.get_unchecked_mut(index) }
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct LeafNode<V: Value> {
    pub nibbles: NibbleBuf,
    pub value: V,
}

impl<V: Value> Digestible for LeafNode<V> {
    fn to_digest(&self) -> H256 {
        leaf_node_hash(&self.nibbles, self.value.to_digest())
    }
}

impl<V: Value> LeafNode<V> {
    pub fn new(nibbles: NibbleBuf, value: V) -> Self {
        Self { nibbles, value }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum TrieNode<V: Value> {
    Extension(Box<ExtensionNode>),
    Branch(Box<BranchNode>),
    Leaf(Box<LeafNode<V>>),
}

impl<V: Value> Digestible for TrieNode<V> {
    fn to_digest(&self) -> H256 {
        match self {
            TrieNode::Extension(n) => n.to_digest(),
            TrieNode::Branch(n) => n.to_digest(),
            TrieNode::Leaf(n) => n.to_digest(),
        }
    }
}

impl<V: Value> From<ExtensionNode> for TrieNode<V> {
    fn from(n: ExtensionNode) -> Self {
        Self::Extension(Box::new(n))
    }
}

impl<V: Value> From<BranchNode> for TrieNode<V> {
    fn from(n: BranchNode) -> Self {
        Self::Branch(Box::new(n))
    }
}

impl<V: Value> From<LeafNode<V>> for TrieNode<V> {
    fn from(n: LeafNode<V>) -> Self {
        Self::Leaf(Box::new(n))
    }
}

pub trait NodeLoader<V: Value> {
    fn load_node(&self, address: H256) -> Result<TrieNode<V>>;

    fn check_address_and_load_node(&self, address: H256) -> Result<Option<TrieNode<V>>> {
        if address.is_zero() {
            Ok(None)
        } else {
            self.load_node(address).map(Some)
        }
    }
}
