#[cfg(feature = "partial_trie")]
use crate::partial_trie::{PartialTrie, PartialTrieDiff, SubTree};
use crate::{
    nibbles::{AsNibbles, NibbleBuf},
    proof::{Proof, SubProof},
    storage::{NodeLoader, TrieNode},
    traits::Value,
    u4::U4,
};
use alloc::{
    collections::{BTreeMap, VecDeque},
    format,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use core::fmt;
use slimchain_common::{basic::H256, collections::HashMap, digest::Digestible, error::Result};

#[derive(Debug)]
struct Vertex {
    id: u32,
    label: String,
    styles: Vec<String>,
}

impl Vertex {
    fn add_style(&mut self, style: impl ToString) {
        self.styles.push(style.to_string());
    }
}

#[derive(Debug)]
struct Edge {
    parent_vertex: u32,
    child_vertex: u32,
    label: Option<String>,
    styles: Vec<String>,
}

#[derive(Debug)]
pub struct Graph {
    name: String,
    label: Option<String>,
    styles: Vec<String>,
    edges: BTreeMap<(u32, u32), Edge>,
    vertices: BTreeMap<u32, Vertex>,
    nibble_vertex_map: HashMap<NibbleBuf, u32>,
    next_id: u32,
}

impl Graph {
    fn new(name: impl ToString) -> Self {
        Self {
            name: name.to_string(),
            label: None,
            styles: Vec::new(),
            edges: BTreeMap::new(),
            vertices: BTreeMap::new(),
            nibble_vertex_map: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn set_label(&mut self, label: impl ToString) {
        self.label = Some(label.to_string());
    }

    pub fn add_style(&mut self, style: impl ToString) {
        self.styles.push(style.to_string());
    }

    fn next_vertex_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn add_vertex(&mut self, id: u32, label: String, nibbles: NibbleBuf) {
        self.vertices.insert(
            id,
            Vertex {
                id,
                label,
                styles: Vec::new(),
            },
        );
        self.nibble_vertex_map.insert(nibbles, id);
    }

    fn add_edge(&mut self, parent_vertex: u32, child_vertex: u32, label: Option<String>) {
        self.edges.insert(
            (parent_vertex, child_vertex),
            Edge {
                parent_vertex,
                child_vertex,
                label,
                styles: Vec::new(),
            },
        );
    }

    pub fn vertex_dot_id(&self, nibbles: impl AsNibbles) -> Option<String> {
        let nibbles = nibbles.as_nibbles().to_nibble_buf();
        self.nibble_vertex_map
            .get(&nibbles)
            .map(|id| format!("{}_{}", self.name, id))
    }

    pub fn to_dot(&self, subgraph: bool) -> String {
        let mut out = String::new();

        if subgraph {
            out.push_str(&format!("subgraph cluster_{} {{\n", self.name));
        } else {
            out.push_str(&format!("digraph {} {{\n", self.name));
        }

        if let Some(label) = &self.label {
            out.push_str(&format!("    label=\"{}\";\n", label.replace("\n", "\\n")));
        }

        for style in &self.styles {
            out.push_str(&format!("    {};\n", style));
        }

        if self.label.is_some() || self.styles.len() > 0 {
            out.push_str("\n");
        }

        for Vertex { id, label, styles } in self.vertices.values() {
            let mut meta = Vec::new();
            meta.push(format!("label=\"{}\"", label.replace("\n", "\\n")));
            meta.extend(styles.iter().cloned());
            out.push_str(&format!("    {}_{} [{}];\n", self.name, id, meta.join(",")));
        }

        for Edge {
            parent_vertex,
            child_vertex,
            label,
            styles,
        } in self.edges.values()
        {
            let mut meta = Vec::new();
            if let Some(label) = label {
                meta.push(format!("label=\"{}\"", label.replace("\n", "\\n")));
            }
            meta.extend(styles.iter().cloned());
            out.push_str(&format!(
                "    {}_{} -> {}_{} [{}];\n",
                self.name,
                parent_vertex,
                self.name,
                child_vertex,
                meta.join(",")
            ));
        }

        out.push_str("}\n");
        out
    }

    pub fn from_trie<V: Value + fmt::Display>(
        name: impl ToString,
        trie: &impl NodeLoader<V>,
        root: H256,
    ) -> Result<Self> {
        let mut out = Self::new(name);
        let mut queue: VecDeque<(H256, u32, Option<String>, Vec<U4>)> = VecDeque::new();
        queue.push_back((root, 0, None, Vec::new()));

        while let Some((cur, parent_id, edge_label, nibbles)) = queue.pop_front() {
            let cur_id = out.next_vertex_id();
            let nibble_buf = nibbles.iter().copied().collect();

            let node = match trie.check_address_and_load_node(cur)? {
                Some(n) => n,
                None => continue,
            };

            if cur_id != parent_id {
                out.add_edge(parent_id, cur_id, edge_label);
            }

            match &node {
                TrieNode::Extension(n) => {
                    out.add_vertex(
                        cur_id,
                        format!("Extension(n={})\n{}", n.nibbles, n.to_digest()),
                        nibble_buf,
                    );
                    let mut c_nibbles = nibbles.clone();
                    c_nibbles.extend(n.nibbles.iter());
                    queue.push_back((n.child, cur_id, None, c_nibbles));
                }
                TrieNode::Branch(n) => {
                    out.add_vertex(cur_id, format!("Branch\n{}", n.to_digest()), nibble_buf);
                    for (i, child) in n.children.iter().enumerate() {
                        if let Some(c) = child {
                            let mut c_nibbles = nibbles.clone();
                            c_nibbles.push(unsafe { U4::from_u8_unchecked(i as u8) });
                            queue.push_back((*c, cur_id, Some(format!("{:x}", i)), c_nibbles));
                        }
                    }
                }
                TrieNode::Leaf(n) => {
                    out.add_vertex(
                        cur_id,
                        format!("Leaf(n={})\n{}", n.nibbles, n.to_digest()),
                        nibble_buf,
                    );
                    let c_id = out.next_vertex_id();
                    let c_nibble_buf = nibbles.iter().copied().chain(n.nibbles.iter()).collect();
                    out.add_vertex(
                        c_id,
                        format!("Value(v={})\n{}", n.value, n.value.to_digest()),
                        c_nibble_buf,
                    );
                    out.add_edge(cur_id, c_id, None);
                }
            }
        }

        if let Some(root_vertex) = out.vertices.get_mut(&0) {
            root_vertex.add_style("style=filled");
            root_vertex.add_style("fillcolor=cyan");
        }

        Ok(out)
    }

    pub fn from_proof(name: impl ToString, proof: &Proof) -> Self {
        let mut out = Self::new(name);
        let root_node = match proof.root.as_ref() {
            Some(root) => root,
            None => return out,
        };

        let mut queue: VecDeque<(&SubProof, u32, Option<String>, Vec<U4>)> = VecDeque::new();
        queue.push_back((root_node, 0, None, Vec::new()));

        while let Some((cur, parent_id, edge_label, nibbles)) = queue.pop_front() {
            let cur_id = out.next_vertex_id();
            let nibble_buf = nibbles.iter().copied().collect();

            if cur_id != parent_id {
                out.add_edge(parent_id, cur_id, edge_label);
            }

            match cur {
                SubProof::Hash(h) => {
                    out.add_vertex(cur_id, format!("Hash\n{}", h), nibble_buf);
                }
                SubProof::Extension(n) => {
                    out.add_vertex(
                        cur_id,
                        format!("Extension(n={})\n{}", n.nibbles, n.to_digest()),
                        nibble_buf,
                    );
                    let mut c_nibbles = nibbles.clone();
                    c_nibbles.extend(n.nibbles.iter());
                    queue.push_back((&n.child, cur_id, None, c_nibbles));
                }
                SubProof::Branch(n) => {
                    out.add_vertex(cur_id, format!("Branch\n{}", n.to_digest()), nibble_buf);
                    for (i, child) in n.children.iter().enumerate() {
                        if let Some(c) = child.as_ref() {
                            let mut c_nibbles = nibbles.clone();
                            c_nibbles.push(unsafe { U4::from_u8_unchecked(i as u8) });
                            queue.push_back((c, cur_id, Some(format!("{:x}", i)), c_nibbles));
                        }
                    }
                }
                SubProof::Leaf(n) => {
                    out.add_vertex(
                        cur_id,
                        format!("Leaf(n={})\n{}", n.nibbles, n.to_digest()),
                        nibble_buf,
                    );
                    let c_id = out.next_vertex_id();
                    let c_nibble_buf = nibbles.iter().copied().chain(n.nibbles.iter()).collect();
                    out.add_vertex(c_id, format!("Value\n{}", n.value_hash), c_nibble_buf);
                    out.add_edge(cur_id, c_id, None);
                }
            }
        }

        if let Some(root_vertex) = out.vertices.get_mut(&0) {
            root_vertex.add_style("style=filled");
            root_vertex.add_style("fillcolor=cyan");
        }

        out
    }

    #[cfg(feature = "partial_trie")]
    fn from_subtree(name: impl ToString, subtree: &Arc<SubTree>) -> Self {
        let mut out = Self::new(name);
        let mut queue: VecDeque<(&SubTree, u32, Option<String>, Vec<U4>)> = VecDeque::new();
        queue.push_back((subtree, 0, None, Vec::new()));

        while let Some((cur, parent_id, edge_label, nibbles)) = queue.pop_front() {
            let cur_id = out.next_vertex_id();
            let nibble_buf = nibbles.iter().copied().collect();

            if cur_id != parent_id {
                out.add_edge(parent_id, cur_id, edge_label);
            }

            match cur {
                SubTree::Hash(h) => {
                    out.add_vertex(cur_id, format!("Hash\n{}", h), nibble_buf);
                }
                SubTree::Extension(n) => {
                    out.add_vertex(
                        cur_id,
                        format!("Extension(n={})\n{}", n.nibbles, n.to_digest()),
                        nibble_buf,
                    );
                    let mut c_nibbles = nibbles.clone();
                    c_nibbles.extend(n.nibbles.iter());
                    queue.push_back((&n.child, cur_id, None, c_nibbles));
                }
                SubTree::Branch(n) => {
                    out.add_vertex(cur_id, format!("Branch\n{}", n.to_digest()), nibble_buf);
                    for (i, child) in n.children.iter().enumerate() {
                        if let Some(c) = child.as_ref() {
                            let mut c_nibbles = nibbles.clone();
                            c_nibbles.push(unsafe { U4::from_u8_unchecked(i as u8) });
                            queue.push_back((c, cur_id, Some(format!("{:x}", i)), c_nibbles));
                        }
                    }
                }
                SubTree::Leaf(n) => {
                    out.add_vertex(
                        cur_id,
                        format!("Leaf(n={})\n{}", n.nibbles, n.to_digest()),
                        nibble_buf,
                    );
                    let c_id = out.next_vertex_id();
                    let c_nibble_buf = nibbles.iter().copied().chain(n.nibbles.iter()).collect();
                    out.add_vertex(c_id, format!("Value\n{}", n.value_hash), c_nibble_buf);
                    out.add_edge(cur_id, c_id, None);
                }
            }
        }

        if let Some(root_vertex) = out.vertices.get_mut(&0) {
            root_vertex.add_style("style=filled");
            root_vertex.add_style("fillcolor=cyan");
        }

        out
    }

    #[cfg(feature = "partial_trie")]
    pub fn from_partial_trie(name: impl ToString, trie: &PartialTrie) -> Self {
        match trie.root.as_ref() {
            Some(root) => Self::from_subtree(name, root),
            None => Self::new(name),
        }
    }
}

#[derive(Debug)]
struct MultiGraphEdge {
    parent_vertex: String,
    child_vertex: String,
    label: Option<String>,
    styles: Vec<String>,
}

#[derive(Debug)]
pub struct MultiGraph {
    name: String,
    label: Option<String>,
    styles: Vec<String>,
    sub_graphs: Vec<String>,
    edges: Vec<MultiGraphEdge>,
}

impl MultiGraph {
    pub fn new(name: impl ToString) -> Self {
        Self {
            name: name.to_string(),
            label: None,
            styles: Vec::new(),
            sub_graphs: Vec::new(),
            edges: Vec::new(),
        }
    }

    pub fn set_label(&mut self, label: impl ToString) {
        self.label = Some(label.to_string());
    }

    pub fn add_style(&mut self, style: impl ToString) {
        self.styles.push(style.to_string());
    }

    pub fn add_sub_graph(&mut self, sub_graph: &Graph) {
        self.sub_graphs.push(sub_graph.to_dot(true));
    }

    pub fn add_sub_multi_graph(&mut self, sub_graph: &MultiGraph) {
        self.sub_graphs.push(sub_graph.to_dot(true));
    }

    pub fn add_edge(
        &mut self,
        parent_vertex: String,
        child_vertex: String,
        label: Option<String>,
        styles: Vec<String>,
    ) {
        self.edges.push(MultiGraphEdge {
            parent_vertex,
            child_vertex,
            label,
            styles,
        });
    }

    pub fn to_dot(&self, subgraph: bool) -> String {
        let mut out = String::new();

        if subgraph {
            out.push_str(&format!("subgraph cluster_{} {{\n", self.name));
        } else {
            out.push_str(&format!("digraph {} {{\n", self.name));
        }

        if let Some(label) = &self.label {
            out.push_str(&format!("    label=\"{}\";\n", label.replace("\n", "\\n")));
        }

        for style in &self.styles {
            out.push_str(&format!("    {};\n", style));
        }

        if self.label.is_some() || self.styles.len() > 0 {
            out.push_str("\n");
        }

        for sub_graph in &self.sub_graphs {
            for line in sub_graph.lines() {
                out.push_str(&format!("    {}\n", line));
            }

            out.push_str("\n");
        }

        for MultiGraphEdge {
            parent_vertex,
            child_vertex,
            label,
            styles,
        } in &self.edges
        {
            let mut meta = Vec::with_capacity(2);
            if let Some(label) = label {
                meta.push(format!("label=\"{}\"", label.replace("\n", "\\n")));
            }
            meta.extend(styles.iter().cloned());
            out.push_str(&format!(
                "    {} -> {} [{}];\n",
                parent_vertex,
                child_vertex,
                meta.join(",")
            ));
        }

        out.push_str("}\n");
        out
    }

    #[cfg(feature = "partial_trie")]
    pub fn from_partial_trie_diff(name: impl ToString, diff: &PartialTrieDiff) -> Self {
        let name = name.to_string();
        let mut out = Self::new(name.clone());

        for (i, (prefix, subtree)) in diff.0.iter().enumerate() {
            let sub_graph_name = format!("{}_diff{}", name, i);
            let mut sub_graph = Graph::from_subtree(sub_graph_name, subtree);
            sub_graph.set_label(format!("prefix={}", prefix));
            out.add_sub_graph(&sub_graph);
        }

        out
    }
}

#[cfg(feature = "std")]
mod draw_graph {
    use super::*;
    use std::{fs, path::Path, process::Command};

    pub fn draw_dot(dot: String, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent_dir) = path.parent() {
            fs::create_dir_all(parent_dir)?;
        }
        fs::write(path, dot)?;
        Command::new("dot")
            .arg("-Tpdf")
            .arg(path)
            .arg("-o")
            .arg(format!("{}.pdf", path.to_string_lossy()))
            .status()?;
        Ok(())
    }

    pub fn draw_trie<V: Value + fmt::Display>(
        trie: &impl NodeLoader<V>,
        root: H256,
        path: impl AsRef<Path>,
    ) -> Result<()> {
        draw_dot(Graph::from_trie("trie", trie, root)?.to_dot(false), path)
    }

    pub fn draw_proof(proof: &Proof, path: impl AsRef<Path>) -> Result<()> {
        draw_dot(Graph::from_proof("proof", proof).to_dot(false), path)
    }

    #[cfg(feature = "partial_trie")]
    pub fn draw_partial_trie(trie: &PartialTrie, path: impl AsRef<Path>) -> Result<()> {
        draw_dot(
            Graph::from_partial_trie("partial_trie", trie).to_dot(false),
            path,
        )
    }

    #[cfg(feature = "partial_trie")]
    pub fn draw_partial_trie_diff(diff: &PartialTrieDiff, path: impl AsRef<Path>) -> Result<()> {
        draw_dot(
            MultiGraph::from_partial_trie_diff("partial_trie_diff", diff).to_dot(false),
            path,
        )
    }
}

#[cfg(feature = "std")]
pub use draw_graph::*;
