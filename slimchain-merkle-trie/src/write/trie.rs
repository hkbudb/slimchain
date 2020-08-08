use crate::{
    nibbles::{split_at_common_prefix_buf2, AsNibbles, NibbleBuf},
    storage::{BranchNode, ExtensionNode, LeafNode, NodeLoader, TrieNode},
    traits::{Key, Value},
    u4::U4,
};
use alloc::{borrow::Cow, vec::Vec};
use core::marker::PhantomData;
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::H256,
    collections::{HashMap, HashSet},
    digest::Digestible,
    error::Result,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Apply<V: Value> {
    pub root: H256,
    pub nodes: HashMap<H256, TrieNode<V>>,
}

pub struct WriteTrieContext<K: Key, V: Value, L: NodeLoader<V>> {
    trie_node_loader: L,
    apply: Apply<V>,
    outdated: HashSet<H256>,
    _marker: PhantomData<K>,
}

impl<K: Key, V: Value, L: NodeLoader<V>> WriteTrieContext<K, V, L> {
    pub fn new(trie_node_loader: L, root_address: H256) -> Self {
        Self {
            trie_node_loader,
            apply: Apply {
                root: root_address,
                nodes: HashMap::new(),
            },
            outdated: HashSet::new(),
            _marker: PhantomData::default(),
        }
    }

    pub fn changes(self) -> Apply<V> {
        self.apply
    }

    fn write_leaf(&mut self, nibbles: NibbleBuf, value: V) -> H256 {
        let n = LeafNode { nibbles, value };
        let h = n.to_digest();
        if !h.is_zero() {
            self.outdated.remove(&h);
            self.apply.nodes.insert(h, TrieNode::from(n));
        }
        h
    }

    fn write_extension(&mut self, nibbles: NibbleBuf, child: H256) -> H256 {
        debug_assert!(!nibbles.is_empty());
        let n = ExtensionNode { nibbles, child };
        let h = n.to_digest();
        if !h.is_zero() {
            self.outdated.remove(&h);
            self.apply.nodes.insert(h, TrieNode::from(n));
        }
        h
    }

    fn write_branch(&mut self, n: BranchNode) -> H256 {
        let h = n.to_digest();
        if !h.is_zero() {
            self.outdated.remove(&h);
            self.apply.nodes.insert(h, TrieNode::from(n));
        }
        h
    }

    pub fn insert(&mut self, key: &K, value: V) -> Result<()> {
        let mut cur_addr = self.apply.root;
        let mut cur_key = key.as_nibbles();

        #[allow(clippy::large_enum_variant)]
        enum TempNode {
            Hash(H256),
            Extension { nibbles: NibbleBuf },
            Branch { node: BranchNode, index: U4 },
        }

        let mut temp_nodes: Vec<TempNode> = Vec::new();

        loop {
            self.outdated.insert(cur_addr);
            let cur_node = match self.get_node(cur_addr)? {
                Some(n) => n,
                None => {
                    let leaf_hash = self.write_leaf(cur_key.to_nibble_buf(), value);
                    temp_nodes.push(TempNode::Hash(leaf_hash));
                    break;
                }
            };

            match cur_node.as_ref() {
                TrieNode::Extension(n) => {
                    if let Some(remaining) = cur_key.strip_prefix(&n.nibbles) {
                        temp_nodes.push(TempNode::Extension {
                            nibbles: n.nibbles.clone(),
                        });

                        cur_addr = n.child;
                        cur_key = remaining;
                    } else {
                        let (common_key, cur_idx, rest_cur_key, node_idx, rest_node_key) =
                            split_at_common_prefix_buf2(&cur_key, &n.nibbles);

                        if !common_key.is_empty() {
                            temp_nodes.push(TempNode::Extension {
                                nibbles: common_key,
                            });
                        }

                        let mut branch = BranchNode::default();
                        if rest_node_key.is_empty() {
                            *branch.get_child_mut(node_idx) = Some(n.child);
                        } else {
                            let ext_child = n.child;
                            let ext_hash = self.write_extension(rest_node_key, ext_child);
                            *branch.get_child_mut(node_idx) = Some(ext_hash);
                        }
                        temp_nodes.push(TempNode::Branch {
                            node: branch,
                            index: cur_idx,
                        });

                        let leaf_hash = self.write_leaf(rest_cur_key, value);
                        temp_nodes.push(TempNode::Hash(leaf_hash));
                        break;
                    }
                }
                TrieNode::Branch(n) => {
                    if let Some((first, remaining)) = cur_key.split_first() {
                        temp_nodes.push(TempNode::Branch {
                            node: BranchNode::new(n.children),
                            index: first,
                        });

                        match n.get_child(first) {
                            Some(child) => {
                                cur_addr = child;
                                cur_key = remaining;
                            }
                            None => {
                                let leaf_hash = self.write_leaf(remaining.to_nibble_buf(), value);
                                temp_nodes.push(TempNode::Hash(leaf_hash));
                                break;
                            }
                        }
                    } else {
                        panic!("Invalid key. Branch node does not store value.");
                    }
                }
                TrieNode::Leaf(n) => {
                    if cur_key == n.nibbles.as_nibbles() {
                        let leaf_hash = self.write_leaf(cur_key.to_nibble_buf(), value);
                        temp_nodes.push(TempNode::Hash(leaf_hash));
                        break;
                    } else {
                        let (common_key, cur_idx, rest_cur_key, node_idx, rest_node_key) =
                            split_at_common_prefix_buf2(&cur_key, &n.nibbles);

                        if !common_key.is_empty() {
                            temp_nodes.push(TempNode::Extension {
                                nibbles: common_key,
                            });
                        }

                        let mut branch = BranchNode::default();
                        let node_value = n.value.clone();
                        let node_leaf_hash = self.write_leaf(rest_node_key, node_value);
                        *branch.get_child_mut(node_idx) = Some(node_leaf_hash);

                        temp_nodes.push(TempNode::Branch {
                            node: branch,
                            index: cur_idx,
                        });

                        let leaf_hash = self.write_leaf(rest_cur_key, value);
                        temp_nodes.push(TempNode::Hash(leaf_hash));
                        break;
                    }
                }
            }
        }

        let mut new_root = H256::zero();
        for node in temp_nodes.into_iter().rev() {
            match node {
                TempNode::Hash(h) => {
                    new_root = h;
                }
                TempNode::Extension { nibbles } => {
                    new_root = self.write_extension(nibbles, new_root);
                }
                TempNode::Branch { mut node, index } => {
                    if new_root.is_zero() {
                        *node.get_child_mut(index) = None;
                    } else {
                        *node.get_child_mut(index) = Some(new_root);
                    }
                    new_root = self.write_branch(node);
                }
            }
        }
        self.apply.root = new_root;

        for addr in self.outdated.drain() {
            self.apply.nodes.remove(&addr);
        }

        Ok(())
    }

    fn get_node(&self, address: H256) -> Result<Option<Cow<TrieNode<V>>>> {
        Ok(match self.apply.nodes.get(&address) {
            Some(n) => Some(Cow::Borrowed(n)),
            None => self
                .trie_node_loader
                .check_address_and_load_node(address)?
                .map(Cow::Owned),
        })
    }
}
