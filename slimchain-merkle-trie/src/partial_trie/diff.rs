use super::{BranchNode, ExtensionNode, LeafNode, PartialTrie, SubTree};
use crate::{
    nibbles::{AsNibbles, NibbleBuf},
    traits::Key,
    u4::U4,
};
use alloc::{collections::VecDeque, sync::Arc, vec::Vec};
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::H256,
    collections::{hash_map::Entry, HashMap},
    digest::Digestible,
    error::{bail, ensure, Result},
};

#[derive(Debug, Default, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct PartialTrieDiff(pub(crate) HashMap<NibbleBuf, Arc<SubTree>>);

impl PartialTrieDiff {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn diff_from_empty(fork: &PartialTrie) -> Self {
        let mut diff = Self::default();

        let fork_root = match fork.root.as_ref() {
            Some(root) => root.clone(),
            None => return diff,
        };
        diff.0.insert(NibbleBuf::default(), fork_root);

        diff
    }

    pub fn to_standalone_trie(&self) -> Result<PartialTrie> {
        if self.0.is_empty() {
            return Ok(PartialTrie::default());
        }

        match self.0.get(&NibbleBuf::default()) {
            Some(root) => Ok(PartialTrie::from_subtree(root.clone())),
            None => bail!("PartialTrieDiff#to_standalone_trie: Invalid partial trie diff."),
        }
    }

    pub fn value_hash(&self, key: &impl Key) -> Option<H256> {
        let key = key.as_nibbles();

        for (prefix, subtree) in self.0.iter() {
            if let Some(remaining) = key.strip_prefix(prefix) {
                return subtree.value_hash(remaining);
            }
        }

        None
    }
}

pub fn diff_missing_branches(
    main: &PartialTrie,
    fork: &PartialTrie,
    fail_on_leaf_conflict: bool,
) -> Result<PartialTrieDiff> {
    let mut diff = PartialTrieDiff::default();

    let fork_root = match fork.root.as_ref() {
        Some(root) => root.clone(),
        None => return Ok(diff),
    };
    let main_root = match main.root.as_ref() {
        Some(root) => root.clone(),
        None => bail!("write-write conflict"),
    };

    let mut queue: VecDeque<(Arc<SubTree>, Arc<SubTree>, Vec<U4>)> = VecDeque::new();
    queue.push_back((main_root, fork_root, Vec::new()));

    while let Some((main_node, fork_node, mut cur_nibbles)) = queue.pop_front() {
        match (main_node.as_ref(), fork_node.as_ref()) {
            (_, SubTree::Hash(_)) => continue,
            (SubTree::Hash(h), _) => {
                debug_assert_eq!(fork_node.to_digest(), *h, "Invalid hash during diff.");
                let cur_nibbles = cur_nibbles.iter().copied().collect();
                diff.0.insert(cur_nibbles, fork_node);
            }
            (SubTree::Extension(main_n), SubTree::Extension(fork_n)) => {
                let remaining = match fork_n.nibbles.as_nibbles().strip_prefix(&main_n.nibbles) {
                    Some(remaining) => remaining,
                    None => bail!("write-write conflict"),
                };

                cur_nibbles.extend(main_n.nibbles.iter());

                let (remaining_child_idx, remaining) = match remaining.split_first() {
                    Some(res) => res,
                    None => {
                        queue.push_back((main_n.child.clone(), fork_n.child.clone(), cur_nibbles));
                        continue;
                    }
                };

                let remaining_child_node = if remaining.is_empty() {
                    fork_n.child.clone()
                } else {
                    Arc::new(SubTree::from_extension(ExtensionNode::new(
                        remaining.to_nibble_buf(),
                        fork_n.child.clone(),
                    )))
                };

                let mut branch_node = BranchNode::default();
                *branch_node.get_child_mut(remaining_child_idx) = Some(remaining_child_node);

                queue.push_back((
                    main_n.child.clone(),
                    Arc::new(SubTree::from_branch(branch_node)),
                    cur_nibbles,
                ));
            }
            (SubTree::Extension(main_n), SubTree::Leaf(fork_n)) => {
                let remaining = match fork_n.nibbles.as_nibbles().strip_prefix(&main_n.nibbles) {
                    Some(remaining) => remaining,
                    None => bail!("write-write conflict"),
                };

                cur_nibbles.extend(main_n.nibbles.iter());

                let (remaining_child_idx, remaining) = remaining
                    .split_first()
                    .expect("Invalid leaf node in the fork version of the partial trie.");

                let remaining_child_node = Arc::new(SubTree::from_leaf(LeafNode::new(
                    remaining.to_nibble_buf(),
                    fork_n.value_hash,
                )));

                let mut branch_node = BranchNode::default();
                *branch_node.get_child_mut(remaining_child_idx) = Some(remaining_child_node);

                queue.push_back((
                    main_n.child.clone(),
                    Arc::new(SubTree::from_branch(branch_node)),
                    cur_nibbles,
                ));
            }
            (SubTree::Branch(_), SubTree::Extension(fork_n)) => {
                let (remaining_child_idx, remaining) = fork_n
                    .nibbles
                    .as_nibbles()
                    .split_first()
                    .expect("Invalid extension node in the fork version of the partial trie.");

                let remaining_child_node = if remaining.is_empty() {
                    fork_n.child.clone()
                } else {
                    Arc::new(SubTree::from_extension(ExtensionNode::new(
                        remaining.to_nibble_buf(),
                        fork_n.child.clone(),
                    )))
                };

                let mut branch_node = BranchNode::default();
                *branch_node.get_child_mut(remaining_child_idx) = Some(remaining_child_node);

                queue.push_back((
                    main_node,
                    Arc::new(SubTree::from_branch(branch_node)),
                    cur_nibbles,
                ));
            }
            (SubTree::Branch(main_n), SubTree::Branch(fork_n)) => {
                for (i, (main_child, fork_child)) in main_n
                    .children
                    .iter()
                    .zip(fork_n.children.iter())
                    .enumerate()
                {
                    match (main_child, fork_child) {
                        (Some(main_c), Some(fork_c)) => {
                            let mut cur_nibbles = cur_nibbles.clone();
                            cur_nibbles.push(unsafe { U4::from_u8_unchecked(i as u8) });
                            queue.push_back((main_c.clone(), fork_c.clone(), cur_nibbles));
                        }
                        (None, Some(_)) => bail!("write-write conflict"),
                        (_, None) => {}
                    }
                }
            }
            (SubTree::Branch(_), SubTree::Leaf(fork_n)) => {
                let (remaining_child_idx, remaining) = fork_n
                    .nibbles
                    .as_nibbles()
                    .split_first()
                    .expect("Invalid leaf node in the fork version of the partial trie.");

                let remaining_child_node = Arc::new(SubTree::from_leaf(LeafNode::new(
                    remaining.to_nibble_buf(),
                    fork_n.value_hash,
                )));

                let mut branch_node = BranchNode::default();
                *branch_node.get_child_mut(remaining_child_idx) = Some(remaining_child_node);

                queue.push_back((
                    main_node,
                    Arc::new(SubTree::from_branch(branch_node)),
                    cur_nibbles,
                ));
            }
            (SubTree::Leaf(main_n), SubTree::Leaf(fork_n)) => {
                if main_n.nibbles != fork_n.nibbles {
                    bail!("write-write conflict");
                }

                if fail_on_leaf_conflict && main_n.value_hash != fork_n.value_hash {
                    bail!("write-write conflict");
                }
            }
            (SubTree::Extension(_), SubTree::Branch(_))
            | (SubTree::Leaf(_), SubTree::Extension(_))
            | (SubTree::Leaf(_), SubTree::Branch(_)) => bail!("write-write conflict"),
        }
    }

    Ok(diff)
}

pub fn apply_diff(
    base: &PartialTrie,
    diff: &PartialTrieDiff,
    check_hash: bool,
) -> Result<PartialTrie> {
    let mut root = match &base.root {
        Some(root) => root.clone(),
        None => {
            ensure!(diff.0.is_empty(), "Invalid diff.");
            return Ok(base.clone());
        }
    };

    #[allow(clippy::large_enum_variant)]
    enum TempNode {
        SubTree(Arc<SubTree>),
        Extension { nibbles: NibbleBuf },
        Branch { node: BranchNode, index: U4 },
    }

    for (nibbles, trie) in diff.0.iter() {
        let mut cur_nibbles = nibbles.as_nibbles();
        let mut cur_ptr = &root;
        let mut temp_nodes: Vec<TempNode> = Vec::new();

        loop {
            match cur_ptr.as_ref() {
                SubTree::Hash(h) if cur_nibbles.is_empty() => {
                    debug_assert!(!h.is_zero());
                    if check_hash {
                        ensure!(
                            trie.to_digest() == *h,
                            "Hash mismatched when applying diff."
                        );
                    } else {
                        debug_assert_eq!(
                            trie.to_digest(),
                            *h,
                            "Hash mismatched when applying diff."
                        );
                    }
                    temp_nodes.push(TempNode::SubTree(trie.clone()));
                    break;
                }
                SubTree::Hash(_) => bail!("Invalid diff."),
                SubTree::Extension(n) => {
                    if let Some(remaining) = cur_nibbles.strip_prefix(&n.nibbles) {
                        temp_nodes.push(TempNode::Extension {
                            nibbles: n.nibbles.clone(),
                        });

                        cur_ptr = &n.child;
                        cur_nibbles = remaining;
                    } else {
                        bail!("Invalid diff.");
                    }
                }
                SubTree::Branch(n) => {
                    if let Some((first, remaining)) = cur_nibbles.split_first() {
                        temp_nodes.push(TempNode::Branch {
                            node: BranchNode::new(n.children.clone()),
                            index: first,
                        });

                        match n.get_child(first) {
                            Some(child) => {
                                cur_ptr = child;
                                cur_nibbles = remaining;
                            }
                            None => {
                                bail!("Invalid diff.");
                            }
                        }
                    } else {
                        bail!("Invalid diff. Branch node does not store value.");
                    }
                }
                SubTree::Leaf(_) => bail!("Invalid diff."),
            }
        }

        for node in temp_nodes.into_iter().rev() {
            debug_assert!(!root.to_digest().is_zero());
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
    }

    Ok(PartialTrie::from_subtree(root))
}

fn merge_diff_subtree(lhs: &Arc<SubTree>, rhs: &Arc<SubTree>) -> Arc<SubTree> {
    match (lhs.as_ref(), rhs.as_ref()) {
        (_, SubTree::Hash(h)) => {
            debug_assert_eq!(lhs.to_digest(), *h);
            lhs.clone()
        }
        (SubTree::Hash(h), _) => {
            debug_assert_eq!(rhs.to_digest(), *h);
            rhs.clone()
        }
        (SubTree::Extension(l), SubTree::Extension(r)) => {
            debug_assert_eq!(l.nibbles, r.nibbles);
            merge_diff_subtree(&l.child, &r.child)
        }
        (SubTree::Branch(l), SubTree::Branch(r)) => {
            let mut merged_branch = BranchNode::default();
            for (i, (l_c, r_c)) in l.children.iter().zip(r.children.iter()).enumerate() {
                let merged_c = match (l_c.as_ref(), r_c.as_ref()) {
                    (Some(l_sub), Some(r_sub)) => Some(merge_diff_subtree(l_sub, r_sub)),
                    (None, None) => None,
                    (_, _) => unreachable!(),
                };
                let index = unsafe { U4::from_u8_unchecked(i as u8) };
                *merged_branch.get_child_mut(index) = merged_c;
            }
            Arc::new(SubTree::from_branch(merged_branch))
        }
        (SubTree::Leaf(l), SubTree::Leaf(r)) => {
            debug_assert_eq!(l.nibbles, r.nibbles);
            debug_assert_eq!(l.value_hash, r.value_hash);
            lhs.clone()
        }
        (_, _) => unreachable!(),
    }
}

pub fn merge_diff(lhs: &PartialTrieDiff, rhs: &PartialTrieDiff) -> PartialTrieDiff {
    let mut out = lhs.clone();

    for (prefix, subtree) in rhs.0.iter() {
        match out.0.entry(prefix.clone()) {
            Entry::Occupied(mut o) => {
                let merged_subtree = merge_diff_subtree(o.get(), subtree);
                *o.get_mut() = merged_subtree;
            }
            Entry::Vacant(v) => {
                v.insert(subtree.clone());
            }
        }
    }

    out
}
