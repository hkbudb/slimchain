#[cfg(feature = "partial_trie")]
use crate::partial_trie::{PartialTrie, SubTree};
use crate::{
    proof::{Proof, SubProof},
    storage::{NodeLoader, TrieNode},
    traits::Value,
};
use alloc::{collections::VecDeque, format, string::String, vec::Vec};
use core::fmt;
use slimchain_common::{basic::H256, digest::Digestible, error::Result};

#[derive(Debug, Default)]
pub struct Draw {
    edges: Vec<(u32, u32, String)>,
    vertices: Vec<(u32, String)>,
    next_id: u32,
}

impl Draw {
    fn get_next_vertex_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn add_vertex(&mut self, id: u32, label: String) {
        self.vertices.push((id, label));
    }

    fn add_edge(&mut self, parent: u32, child: u32, label: String) {
        self.edges.push((parent, child, label))
    }

    pub fn to_dot(&self) -> String {
        let mut out = String::new();
        out += "digraph MerkleTrie {\n";
        for (id, label) in &self.vertices {
            let style = if *id == 0 { ",color=blue" } else { "" };
            out += &format!("    {} [label=\"{}\"{}];\n", id, label, style);
        }
        for (parent, child, label) in &self.edges {
            out += &format!("    {} -> {} [label=\"{}\"];\n", parent, child, label);
        }
        out += "}";
        out
    }

    #[cfg(feature = "std")]
    pub fn draw<P: AsRef<std::path::Path>>(&self, path: P) -> Result<()> {
        use std::process::Command;
        let path = path.as_ref();
        std::fs::write(path, self.to_dot())?;
        Command::new("dot")
            .arg("-Tpng")
            .arg(path)
            .arg("-o")
            .arg(format!("{}.png", path.to_string_lossy()))
            .status()?;
        Ok(())
    }
}

pub fn trie_to_draw<V: Value + fmt::Display>(
    trie: &impl NodeLoader<V>,
    root: H256,
) -> Result<Draw> {
    let mut draw = Draw::default();
    let mut queue: VecDeque<(H256, u32, String)> = VecDeque::new();
    queue.push_back((root, 0, String::new()));

    while let Some((cur, parent_id, edge_label)) = queue.pop_front() {
        let cur_id = draw.get_next_vertex_id();

        let node = match trie.check_address_and_load_node(cur)? {
            Some(n) => n,
            None => {
                continue;
            }
        };

        if cur_id != 0 {
            draw.add_edge(parent_id, cur_id, edge_label);
        }

        match &node {
            TrieNode::Extension(n) => {
                draw.add_vertex(
                    cur_id,
                    format!("Extension(n={})\n{}", n.nibbles, n.to_digest()),
                );
                queue.push_back((n.child, cur_id, String::new()));
            }
            TrieNode::Branch(n) => {
                draw.add_vertex(cur_id, format!("Branch\n{}", n.to_digest()));
                for (i, child) in n.children.iter().enumerate() {
                    if let Some(c) = child {
                        queue.push_back((*c, cur_id, format!("{:x}", i)));
                    }
                }
            }
            TrieNode::Leaf(n) => {
                draw.add_vertex(
                    cur_id,
                    format!("Leaf(n={},v={})\n{}", n.nibbles, n.value, n.to_digest()),
                );
            }
        }
    }

    Ok(draw)
}

pub fn proof_to_draw(trie: &Proof) -> Draw {
    let mut draw = Draw::default();
    let root_node = match trie.root.as_ref() {
        Some(root) => root,
        None => {
            return draw;
        }
    };

    let mut queue: VecDeque<(&SubProof, u32, String)> = VecDeque::new();
    queue.push_back((root_node, 0, String::new()));

    while let Some((cur, parent_id, edge_label)) = queue.pop_front() {
        let cur_id = draw.get_next_vertex_id();

        if cur_id != 0 {
            draw.add_edge(parent_id, cur_id, edge_label);
        }

        match cur {
            SubProof::Hash(h) => {
                draw.add_vertex(cur_id, format!("Hash\n{}", h));
            }
            SubProof::Extension(n) => {
                draw.add_vertex(
                    cur_id,
                    format!("Extension(n={})\n{}", n.nibbles, n.to_digest()),
                );
                queue.push_back((&n.child, cur_id, String::new()));
            }
            SubProof::Branch(n) => {
                draw.add_vertex(cur_id, format!("Branch\n{}", n.to_digest()));
                for (i, child) in n.children.iter().enumerate() {
                    if let Some(c) = child.as_ref() {
                        queue.push_back((c, cur_id, format!("{:x}", i)));
                    }
                }
            }
            SubProof::Leaf(n) => {
                draw.add_vertex(
                    cur_id,
                    format!(
                        "Leaf(n={},v={})\n{}",
                        n.nibbles,
                        n.value_hash,
                        n.to_digest(),
                    ),
                );
            }
        }
    }

    draw
}

#[cfg(feature = "partial_trie")]
pub fn partial_trie_to_draw(trie: &PartialTrie) -> Draw {
    let mut draw = Draw::default();
    let root_node = match trie.root.as_ref() {
        Some(root) => root,
        None => {
            return draw;
        }
    };

    let mut queue: VecDeque<(&SubTree, u32, String)> = VecDeque::new();
    queue.push_back((root_node, 0, String::new()));

    while let Some((cur, parent_id, edge_label)) = queue.pop_front() {
        let cur_id = draw.get_next_vertex_id();

        if cur_id != 0 {
            draw.add_edge(parent_id, cur_id, edge_label);
        }

        match cur {
            SubTree::Hash(h) => {
                draw.add_vertex(cur_id, format!("Hash\n{}", h));
            }
            SubTree::Extension(n) => {
                draw.add_vertex(
                    cur_id,
                    format!("Extension(n={})\n{}", n.nibbles, n.to_digest()),
                );
                queue.push_back((&n.child, cur_id, String::new()));
            }
            SubTree::Branch(n) => {
                draw.add_vertex(cur_id, format!("Branch\n{}", n.to_digest()));
                for (i, child) in n.children.iter().enumerate() {
                    if let Some(c) = child.as_ref() {
                        queue.push_back((c, cur_id, format!("{:x}", i)));
                    }
                }
            }
            SubTree::Leaf(n) => {
                draw.add_vertex(
                    cur_id,
                    format!(
                        "Leaf(n={},v={})\n{}",
                        n.nibbles,
                        n.value_hash,
                        n.to_digest(),
                    ),
                );
            }
        }
    }

    draw
}
