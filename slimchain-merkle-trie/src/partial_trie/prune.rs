use super::{BranchNode, ExtensionNode, PartialTrie, SubTree};
use crate::{
    nibbles::{AsNibbles, NibbleBuf, Nibbles},
    u4::U4,
};
use alloc::{format, sync::Arc, vec::Vec};
use slimchain_common::{
    digest::Digestible,
    error::{bail, Result},
};

pub fn prune_unused_key(trie: &PartialTrie, key: &impl AsNibbles) -> Result<PartialTrie> {
    let root = match &trie.root {
        Some(root) => root.clone(),
        None => bail!("Cannot prune, root is empty"),
    };

    let new_root = prune_unused_key_inner(root, key.as_nibbles())?;
    Ok(PartialTrie::from_subtree(new_root))
}

pub fn prune_unused_keys(
    trie: &PartialTrie,
    keys: impl Iterator<Item = impl AsNibbles>,
) -> Result<PartialTrie> {
    let mut root = match &trie.root {
        Some(root) => root.clone(),
        None => bail!("Cannot prune, root is empty"),
    };

    for k in keys {
        root = prune_unused_key_inner(root, k.as_nibbles())?;
    }
    Ok(PartialTrie::from_subtree(root))
}

fn prune_unused_key_inner(mut root: Arc<SubTree>, key: Nibbles) -> Result<Arc<SubTree>> {
    #[allow(clippy::large_enum_variant)]
    enum TempNode {
        SubTree(Arc<SubTree>),
        Extension { nibbles: NibbleBuf },
        Branch { node: BranchNode, index: U4 },
    }

    struct TailNode<'a> {
        temp_node_depth: usize,
        child: &'a Arc<SubTree>,
    }

    let mut temp_nodes: Vec<TempNode> = Vec::new();
    let mut tail_node: Option<TailNode> = None;
    let mut cur_key = key;
    let mut cur_ptr = &root;

    loop {
        match cur_ptr.as_ref() {
            SubTree::Hash(_) => bail!("Invalid key {}. Branch has already been pruned.", key),
            SubTree::Extension(n) => {
                if let Some(remaining) = cur_key.strip_prefix(&n.nibbles) {
                    temp_nodes.push(TempNode::Extension {
                        nibbles: n.nibbles.clone(),
                    });

                    cur_ptr = &n.child;
                    cur_key = remaining;
                } else {
                    // The pruned value is zero.
                    return Ok(root);
                }
            }
            SubTree::Branch(n) => {
                if let Some((child_idx, remaining)) = cur_key.split_first() {
                    temp_nodes.push(TempNode::Branch {
                        node: BranchNode::new(n.children.clone()),
                        index: child_idx,
                    });

                    match n.get_child(child_idx) {
                        Some(child) => {
                            if n.num_of_materialized_children() > 1 {
                                tail_node = Some(TailNode {
                                    temp_node_depth: temp_nodes.len(),
                                    child,
                                })
                            }

                            cur_ptr = child;
                            cur_key = remaining;
                        }
                        None => {
                            // The pruned value is zero.
                            return Ok(root);
                        }
                    }
                } else {
                    bail!("Invalid key. Branch node does not store value.");
                }
            }
            SubTree::Leaf(n) => {
                if cur_key == n.nibbles.as_nibbles() {
                    break;
                } else {
                    // The pruned value is zero.
                    return Ok(root);
                }
            }
        }
    }

    Ok(match tail_node {
        Some(TailNode {
            temp_node_depth,
            child,
        }) => {
            temp_nodes.truncate(temp_node_depth);
            temp_nodes.push(TempNode::SubTree(Arc::new(SubTree::from_hash(
                child.to_digest(),
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

            root
        }
        None => Arc::new(SubTree::from_hash(root.to_digest())),
    })
}
