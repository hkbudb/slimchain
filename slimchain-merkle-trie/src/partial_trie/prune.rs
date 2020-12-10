use super::{BranchNode, ExtensionNode, PartialTrie, SubTree};
use crate::{
    nibbles::{AsNibbles, NibbleBuf, Nibbles},
    u4::U4,
};
use alloc::{sync::Arc, vec::Vec};
use slimchain_common::{
    digest::Digestible,
    error::{bail, Result},
};

fn prune_key_inner(
    trie: &PartialTrie,
    key: Nibbles,
    kept_prefix_len: usize,
) -> Result<PartialTrie> {
    let mut root = match &trie.root {
        Some(root) => root.clone(),
        None => bail!("Cannot prune, root is empty"),
    };

    if kept_prefix_len == 0 {
        return Ok(PartialTrie::from_root_hash(root.to_digest()));
    }

    #[allow(clippy::large_enum_variant)]
    enum TempNode {
        SubTree(Arc<SubTree>),
        Extension { nibbles: NibbleBuf },
        Branch { node: BranchNode, index: U4 },
    }

    let mut temp_nodes: Vec<TempNode> = Vec::new();
    let mut temp_node_prefix_len: usize = 0;
    let mut cur_key = key;
    let mut cur_ptr = &root;

    while temp_node_prefix_len <= kept_prefix_len {
        match cur_ptr.as_ref() {
            SubTree::Hash(_) => {
                // Branch has already been pruned.
                return Ok(trie.clone());
            }
            SubTree::Extension(n) => {
                if let Some(remaining) = cur_key.strip_prefix(&n.nibbles) {
                    temp_nodes.push(TempNode::Extension {
                        nibbles: n.nibbles.clone(),
                    });
                    temp_node_prefix_len += n.nibbles.len();

                    cur_ptr = &n.child;
                    cur_key = remaining;
                } else {
                    // The pruned value is zero.
                    return Ok(trie.clone());
                }
            }
            SubTree::Branch(n) => {
                if let Some((child_idx, remaining)) = cur_key.split_first() {
                    temp_nodes.push(TempNode::Branch {
                        node: BranchNode::new(n.children.clone()),
                        index: child_idx,
                    });
                    temp_node_prefix_len += 1;

                    match n.get_child(child_idx) {
                        Some(child) => {
                            cur_ptr = child;
                            cur_key = remaining;
                        }
                        None => {
                            // The pruned value is zero.
                            return Ok(trie.clone());
                        }
                    }
                } else {
                    bail!("Invalid key. Branch node does not store value.");
                }
            }
            SubTree::Leaf(_) => {
                // No node is pruned.
                return Ok(trie.clone());
            }
        }
    }

    temp_nodes.push(TempNode::SubTree(Arc::new(SubTree::from_hash(
        cur_ptr.to_digest(),
    ))));

    for node in temp_nodes.into_iter().rev() {
        match node {
            TempNode::SubTree(t) => {
                root = t;
            }
            TempNode::Extension { nibbles } => {
                root = Arc::new(SubTree::from_extension(ExtensionNode::new(nibbles, root)));
            }
            TempNode::Branch { mut node, index } => {
                *node.get_child_mut(index) = Some(root);
                root = Arc::new(SubTree::from_branch(node));
            }
        }
    }

    let new_trie = PartialTrie::from_subtree(root);
    debug_assert_eq!(trie.root_hash(), new_trie.root_hash());
    Ok(new_trie)
}

pub fn prune_key(
    trie: &PartialTrie,
    key: &impl AsNibbles,
    kept_prefix_len: usize,
) -> Result<PartialTrie> {
    prune_key_inner(trie, key.as_nibbles(), kept_prefix_len)
}

pub fn prune_key2(
    trie: &PartialTrie,
    key: &impl AsNibbles,
    other_keys: impl Iterator<Item = impl AsNibbles>,
    kept_root: bool,
) -> Result<PartialTrie> {
    let key = key.as_nibbles();
    let key_len = key.len();
    let mut kept_prefix_len = other_keys
        .map(|other_k| key.common_prefix_len(&other_k))
        .filter(|&l| l != key_len) // filter self
        .max()
        .unwrap_or(0);

    if kept_prefix_len == 0 && kept_root {
        kept_prefix_len = 1;
    }

    prune_key_inner(trie, key, kept_prefix_len)
}
