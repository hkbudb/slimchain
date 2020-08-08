use crate::{
    nibbles::{AsNibbles, Nibbles},
    proof::{self, Proof, SubProof},
    storage::{NodeLoader, TrieNode},
    traits::{Key, Value},
};
use alloc::boxed::Box;
use slimchain_common::{
    basic::H256,
    collections::{hash_map, HashMap},
    error::Result,
};

fn inner_read_trie<V: Value>(
    trie_node_loader: &impl NodeLoader<V>,
    root_node: TrieNode<V>,
    key: Nibbles<'_>,
) -> Result<(Option<V>, SubProof)> {
    use proof::{BranchNode, ExtensionNode, LeafNode};

    let mut read_proof = SubProof::default();
    let mut read_value: Option<V> = None;

    let mut cur_node = root_node;
    let mut cur_key = key;
    let mut cur_proof = &mut read_proof as *mut _;

    loop {
        match &cur_node {
            crate::TrieNode::Extension(n) => {
                if let Some(remaining) = cur_key.strip_prefix(&n.nibbles) {
                    if let Some(sub_node) = trie_node_loader.check_address_and_load_node(n.child)? {
                        let mut sub_proof = Box::new(SubProof::default());
                        let sub_proof_ptr = &mut *sub_proof as *mut _;
                        unsafe {
                            *cur_proof = SubProof::from_extension(ExtensionNode::new(
                                n.nibbles.clone(),
                                sub_proof,
                            ));
                        }
                        cur_node = sub_node;
                        cur_key = remaining;
                        cur_proof = sub_proof_ptr;
                        continue;
                    }
                }

                unsafe {
                    *cur_proof = SubProof::from_extension(ExtensionNode::new(
                        n.nibbles.clone(),
                        Box::new(SubProof::from_hash(n.child)),
                    ));
                }
                break;
            }
            crate::TrieNode::Branch(n) => {
                if let Some((child_idx, remaining)) = cur_key.split_first() {
                    if let Some(child) = n.get_child(child_idx) {
                        if let Some(sub_node) =
                            trie_node_loader.check_address_and_load_node(child)?
                        {
                            let mut sub_proof = Box::new(SubProof::default());
                            let sub_proof_ptr = &mut *sub_proof as *mut _;
                            let mut branch = BranchNode::from_hashes(&n.children);
                            *branch.get_child_mut(child_idx) = Some(sub_proof);
                            unsafe {
                                *cur_proof = SubProof::from_branch(branch);
                            }
                            cur_node = sub_node;
                            cur_key = remaining;
                            cur_proof = sub_proof_ptr;
                            continue;
                        }
                    }
                } else {
                    panic!("Invalid key. Branch node does not store value.");
                }

                let branch = BranchNode::from_hashes(&n.children);
                unsafe {
                    *cur_proof = SubProof::from_branch(branch);
                }
                break;
            }
            crate::TrieNode::Leaf(n) => {
                read_value = if cur_key == n.nibbles.as_nibbles() {
                    Some(n.value.clone())
                } else {
                    None
                };

                unsafe {
                    *cur_proof =
                        SubProof::from_leaf(LeafNode::new(n.nibbles.clone(), n.value.to_digest()));
                }
                break;
            }
        }
    }

    Ok((read_value, read_proof))
}

pub fn read_trie<K: Key, V: Value>(
    trie_node_loader: &impl NodeLoader<V>,
    root_address: H256,
    key: &K,
) -> Result<(Option<V>, Proof)> {
    let root_node = match trie_node_loader.check_address_and_load_node(root_address)? {
        Some(n) => n,
        None => {
            return Ok((None, Proof::new()));
        }
    };

    let (v, p) = inner_read_trie(trie_node_loader, root_node, key.as_nibbles())?;
    Ok((v, Proof::from_subproof(p)))
}

pub struct ReadTrieContext<K: Key, V: Value, L: NodeLoader<V>> {
    trie_node_loader: L,
    root_address: H256,
    cache: HashMap<K, Option<V>>,
    proof: Proof,
}

impl<K: Key, V: Value, L: NodeLoader<V>> ReadTrieContext<K, V, L> {
    pub fn new(trie_node_loader: L, root_address: H256) -> Self {
        Self {
            trie_node_loader,
            root_address,
            cache: HashMap::new(),
            proof: Proof::new(),
        }
    }

    pub fn get_trie_node_loader(&self) -> &L {
        &self.trie_node_loader
    }

    pub fn get_trie_node_loader_mut(&mut self) -> &mut L {
        &mut self.trie_node_loader
    }

    pub fn get_cache(&self) -> &HashMap<K, Option<V>> {
        &self.cache
    }

    pub fn into_proof(self) -> Proof {
        self.proof
    }

    pub fn get_proof(&self) -> &Proof {
        &self.proof
    }

    pub fn read(&mut self, key: &K) -> Result<Option<&'_ V>> {
        use hash_map::Entry;

        let v = match self.cache.entry(key.clone()) {
            Entry::Vacant(entry) => {
                let value = match self.proof.root.as_mut() {
                    Some(root) => match root.search_prefix(key.as_nibbles()) {
                        Some((sub_proof, sub_root, sub_key)) => {
                            let sub_root_node = self.trie_node_loader.load_node(sub_root)?;
                            let (v, p) =
                                inner_read_trie(&self.trie_node_loader, sub_root_node, sub_key)?;
                            unsafe {
                                *sub_proof = p;
                            }
                            v
                        }
                        None => None,
                    },
                    None => {
                        let (v, p) = read_trie(&self.trie_node_loader, self.root_address, key)?;
                        self.proof = p;
                        v
                    }
                };

                entry.insert(value)
            }
            Entry::Occupied(entry) => entry.into_mut(),
        };

        Ok(v.as_ref())
    }
}
