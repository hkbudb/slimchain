use crate::{
    nibbles::{split_at_common_prefix_buf2, AsNibbles, NibbleBuf},
    partial_trie::{BranchNode, ExtensionNode, LeafNode, PartialTrie, SubTree},
    traits::{Key, Value},
    u4::U4,
};
use alloc::{sync::Arc, vec::Vec};
use core::marker::PhantomData;
use slimchain_common::{
    basic::H256,
    digest::Digestible,
    error::{bail, Result},
};

fn write_leaf(nibbles: NibbleBuf, value_hash: H256) -> Option<Arc<SubTree>> {
    if value_hash.is_zero() {
        None
    } else {
        let n = LeafNode::new(nibbles, value_hash);
        Some(Arc::new(SubTree::from_leaf(n)))
    }
}

fn write_extension(nibbles: NibbleBuf, child: Option<Arc<SubTree>>) -> Option<Arc<SubTree>> {
    match child {
        Some(c) if !c.to_digest().is_zero() => {
            let n = ExtensionNode::new(nibbles, c);
            Some(Arc::new(SubTree::from_extension(n)))
        }
        _ => None,
    }
}

fn write_branch(n: BranchNode) -> Option<Arc<SubTree>> {
    if n.to_digest().is_zero() {
        None
    } else {
        Some(Arc::new(SubTree::from_branch(n)))
    }
}

pub struct WritePartialTrieContext<K: Key> {
    trie: PartialTrie,
    _marker: PhantomData<K>,
}

impl<K: Key> WritePartialTrieContext<K> {
    pub fn new(trie: PartialTrie) -> Self {
        Self {
            trie,
            _marker: PhantomData::default(),
        }
    }

    pub fn finish(self) -> PartialTrie {
        self.trie
    }

    pub fn insert_with_value<V: Value>(&mut self, key: &K, value: &V) -> Result<()> {
        self.insert(key, value.to_digest())
    }

    pub fn insert(&mut self, key: &K, value_hash: H256) -> Result<()> {
        let mut cur_key = key.as_nibbles();
        let mut cur_ptr = match self.trie.root.as_ref() {
            Some(root) => root,
            None => {
                self.trie.root = write_leaf(cur_key.to_nibble_buf(), value_hash);
                return Ok(());
            }
        };

        #[allow(clippy::large_enum_variant)]
        enum TempNode {
            SubTree(Option<Arc<SubTree>>),
            Extension { nibbles: NibbleBuf },
            Branch { node: BranchNode, index: U4 },
        }

        let mut temp_nodes: Vec<TempNode> = Vec::new();

        loop {
            match cur_ptr.as_ref() {
                SubTree::Hash(_) => {
                    bail!("Missing subtree in the partial trie.");
                }
                SubTree::Extension(n) => {
                    if let Some(remaining) = cur_key.strip_prefix(&n.nibbles) {
                        temp_nodes.push(TempNode::Extension {
                            nibbles: n.nibbles.clone(),
                        });

                        cur_ptr = &n.child;
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
                            *branch.get_child_mut(node_idx) = Some(n.child.clone());
                        } else {
                            let ext = write_extension(rest_node_key, Some(n.child.clone()));
                            *branch.get_child_mut(node_idx) = ext;
                        }
                        temp_nodes.push(TempNode::Branch {
                            node: branch,
                            index: cur_idx,
                        });

                        let leaf = write_leaf(rest_cur_key, value_hash);
                        temp_nodes.push(TempNode::SubTree(leaf));
                        break;
                    }
                }
                SubTree::Branch(n) => {
                    if let Some((first, remaining)) = cur_key.split_first() {
                        temp_nodes.push(TempNode::Branch {
                            node: BranchNode::new(n.children.clone()),
                            index: first,
                        });

                        match n.get_child(first) {
                            Some(child) => {
                                cur_ptr = child;
                                cur_key = remaining;
                            }
                            None => {
                                let leaf_hash = write_leaf(remaining.to_nibble_buf(), value_hash);
                                temp_nodes.push(TempNode::SubTree(leaf_hash));
                                break;
                            }
                        }
                    } else {
                        panic!("Invalid key. Branch node does not store value.");
                    }
                }
                SubTree::Leaf(n) => {
                    if cur_key == n.nibbles.as_nibbles() {
                        let leaf = write_leaf(cur_key.to_nibble_buf(), value_hash);
                        temp_nodes.push(TempNode::SubTree(leaf));
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
                        let node_leaf = write_leaf(rest_node_key, n.value_hash);
                        *branch.get_child_mut(node_idx) = node_leaf;

                        temp_nodes.push(TempNode::Branch {
                            node: branch,
                            index: cur_idx,
                        });

                        let leaf = write_leaf(rest_cur_key, value_hash);
                        temp_nodes.push(TempNode::SubTree(leaf));
                        break;
                    }
                }
            }
        }

        let mut new_root = None;
        for node in temp_nodes.into_iter().rev() {
            match node {
                TempNode::SubTree(t) => {
                    new_root = t;
                }
                TempNode::Extension { nibbles } => {
                    new_root = write_extension(nibbles, new_root);
                }
                TempNode::Branch { mut node, index } => {
                    *node.get_child_mut(index) = new_root;
                    new_root = write_branch(node);
                }
            }
        }

        self.trie.root = new_root;

        Ok(())
    }
}
