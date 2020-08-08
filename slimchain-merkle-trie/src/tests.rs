#![allow(clippy::cognitive_complexity)]

use crate::prelude::*;
use core::fmt;
use slimchain_common::{collections::HashMap, error::Context};

#[derive(Debug, Default, Clone, Eq, PartialEq, Hash)]
struct Key(NibbleBuf);

impl AsNibbles for Key {
    fn as_nibbles(&self) -> Nibbles<'_> {
        self.0.as_nibbles()
    }
}

macro_rules! key {
    ($x: literal) => {
        Key(NibbleBuf::from_hex_str($x))
    };
}

#[derive(Debug, Default, Clone, Copy, Eq, PartialEq)]
struct Value(i32);

impl Digestible for Value {
    fn to_digest(&self) -> H256 {
        if self.0 == 0 {
            H256::zero()
        } else {
            self.0.to_digest()
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<i32> for Value {
    fn from(i: i32) -> Self {
        Self(i)
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq)]
struct TestTrie {
    root: H256,
    nodes: HashMap<H256, TrieNode<Value>>,
}

impl NodeLoader<Value> for TestTrie {
    fn load_node(&self, id: H256) -> Result<TrieNode<Value>> {
        self.nodes.get(&id).cloned().context("Unknown node")
    }
}

impl NodeLoader<Value> for &'_ TestTrie {
    fn load_node(&self, id: H256) -> Result<TrieNode<Value>> {
        self.nodes.get(&id).cloned().context("Unknown node")
    }
}

impl TestTrie {
    #[cfg(feature = "write")]
    fn apply(&mut self, apply: Apply<Value>) {
        self.root = apply.root;
        self.nodes.extend(apply.nodes.into_iter());
    }
}

// using example adopted from https://ethereum.stackexchange.com/q/57486
// data:
//  0a711355 1
//  0a77d337 2
//  0a7f9365 3
//  0a77d397 4
//
fn build_test_trie() -> TestTrie {
    let mut trie = TestTrie::default();
    let leaf1 = LeafNode {
        nibbles: NibbleBuf::from_hex_str("7"),
        value: Value(2),
    };
    let leaf1_hash = leaf1.to_digest();
    trie.nodes
        .insert(leaf1_hash, TrieNode::Leaf(Box::new(leaf1)));
    let leaf2 = LeafNode {
        nibbles: NibbleBuf::from_hex_str("7"),
        value: Value(4),
    };
    let leaf2_hash = leaf2.to_digest();
    trie.nodes
        .insert(leaf2_hash, TrieNode::Leaf(Box::new(leaf2)));
    let mut branch1_leaves = [None; 16];
    branch1_leaves[0x3] = Some(leaf1_hash);
    branch1_leaves[0x9] = Some(leaf2_hash);
    let branch1 = BranchNode {
        children: branch1_leaves,
    };
    let branch1_hash = branch1.to_digest();
    trie.nodes
        .insert(branch1_hash, TrieNode::Branch(Box::new(branch1)));
    let leaf3 = LeafNode {
        nibbles: NibbleBuf::from_hex_str("1355"),
        value: Value(1),
    };
    let leaf3_hash = leaf3.to_digest();
    trie.nodes
        .insert(leaf3_hash, TrieNode::Leaf(Box::new(leaf3)));
    let ext1 = ExtensionNode {
        nibbles: NibbleBuf::from_hex_str("d3"),
        child: branch1_hash,
    };
    let ext1_hash = ext1.to_digest();
    trie.nodes
        .insert(ext1_hash, TrieNode::Extension(Box::new(ext1)));
    let leaf4 = LeafNode {
        nibbles: NibbleBuf::from_hex_str("9365"),
        value: Value(3),
    };
    let leaf4_hash = leaf4.to_digest();
    trie.nodes
        .insert(leaf4_hash, TrieNode::Leaf(Box::new(leaf4)));
    let mut branch2_leaves = [None; 16];
    branch2_leaves[0x1] = Some(leaf3_hash);
    branch2_leaves[0x7] = Some(ext1_hash);
    branch2_leaves[0xf] = Some(leaf4_hash);
    let branch2 = BranchNode {
        children: branch2_leaves,
    };
    let branch2_hash = branch2.to_digest();
    trie.nodes
        .insert(branch2_hash, TrieNode::Branch(Box::new(branch2)));
    let ext2 = ExtensionNode {
        nibbles: NibbleBuf::from_hex_str("0a7"),
        child: branch2_hash,
    };
    let ext2_hash = ext2.to_digest();
    trie.nodes
        .insert(ext2_hash, TrieNode::Extension(Box::new(ext2)));
    trie.root = ext2_hash;
    trie
}

#[cfg(feature = "read")]
#[test]
fn test_trie_read() {
    let empty_trie = TestTrie::default();
    let (v, p) = read_trie(&empty_trie, H256::zero(), &key!("12345678")).unwrap();
    assert_eq!(None, v);
    assert_eq!(H256::zero(), p.root_hash());
    assert!(p.value_hash(&key!("12345678")).unwrap().is_zero());

    let trie = build_test_trie();
    let (v, p) = read_trie(&trie, trie.root, &key!("12345678")).unwrap();
    assert_eq!(None, v);
    assert_eq!(trie.root, p.root_hash());
    assert!(p.value_hash(&key!("12345678")).unwrap().is_zero());
    assert_eq!(None, p.value_hash(&key!("0a711355")));

    let (v, p) = read_trie(&trie, trie.root, &key!("0a705678")).unwrap();
    assert_eq!(None, v);
    assert_eq!(trie.root, p.root_hash());
    assert!(p.value_hash(&key!("0a705678")).unwrap().is_zero());
    assert_eq!(None, p.value_hash(&key!("0a711355")));

    let (v, p) = read_trie(&trie, trie.root, &key!("0a715678")).unwrap();
    assert_eq!(None, v);
    assert_eq!(trie.root, p.root_hash());
    assert!(p.value_hash(&key!("0a715678")).unwrap().is_zero());

    let (v, p) = read_trie(&trie, trie.root, &key!("0a711355")).unwrap();
    assert_eq!(Some(1.into()), v);
    assert_eq!(trie.root, p.root_hash());
    assert_eq!(Some(1.to_digest()), p.value_hash(&key!("0a711355")));

    let (v, p) = read_trie(&trie, trie.root, &key!("0a77d337")).unwrap();
    assert_eq!(Some(2.into()), v);
    assert_eq!(trie.root, p.root_hash());
    assert_eq!(Some(2.to_digest()), p.value_hash(&key!("0a77d337")));
    assert_eq!(None, p.value_hash(&key!("0a77d397")));

    let (v, p) = read_trie(&trie, trie.root, &key!("0a7f9365")).unwrap();
    assert_eq!(Some(3.into()), v);
    assert_eq!(trie.root, p.root_hash());
    assert_eq!(Some(3.to_digest()), p.value_hash(&key!("0a7f9365")));

    let (v, p) = read_trie(&trie, trie.root, &key!("0a77d397")).unwrap();
    assert_eq!(Some(4.into()), v);
    assert_eq!(trie.root, p.root_hash());
    assert_eq!(Some(4.to_digest()), p.value_hash(&key!("0a77d397")));
}

#[cfg(feature = "read")]
#[test]
fn test_trie_read_ctx() {
    let empty_trie = TestTrie::default();
    let mut ctx1: ReadTrieContext<Key, _, _> = ReadTrieContext::new(&empty_trie, H256::zero());
    assert_eq!(None, ctx1.read(&key!("12345678")).unwrap());
    assert_eq!(None, ctx1.read(&key!("12345678")).unwrap());
    assert_eq!(None, ctx1.read(&key!("00000000")).unwrap());
    let p = ctx1.into_proof();
    assert_eq!(H256::zero(), p.root_hash());
    assert!(p.value_hash(&key!("12345678")).unwrap().is_zero());
    assert!(p.value_hash(&key!("00000000")).unwrap().is_zero());

    let trie = build_test_trie();
    let mut ctx2: ReadTrieContext<Key, _, _> = ReadTrieContext::new(&trie, trie.root);
    assert_eq!(None, ctx2.read(&key!("12345678")).unwrap());
    assert_eq!(Some(&1.into()), ctx2.read(&key!("0a711355")).unwrap());
    assert_eq!(None, ctx2.read(&key!("0a705678")).unwrap());
    assert_eq!(None, ctx2.read(&key!("0a775678")).unwrap());
    assert_eq!(Some(&2.into()), ctx2.read(&key!("0a77d337")).unwrap());

    let p = ctx2.into_proof();
    assert_eq!(trie.root, p.root_hash());
    assert!(p.value_hash(&key!("12345678")).unwrap().is_zero());
    assert_eq!(Some(1.to_digest()), p.value_hash(&key!("0a711355")));
    assert!(p.value_hash(&key!("0a705678")).unwrap().is_zero());
    assert!(p.value_hash(&key!("0a775678")).unwrap().is_zero());
    assert_eq!(Some(2.to_digest()), p.value_hash(&key!("0a77d337")));
    assert_eq!(None, p.value_hash(&key!("0a7f9365")));
    assert_eq!(None, p.value_hash(&key!("0a77d397")));

    #[cfg(feature = "partial_trie")]
    {
        let partial_trie = PartialTrie::from(p);
        assert_eq!(trie.root, partial_trie.root_hash());
        assert!(partial_trie
            .value_hash(&key!("12345678"))
            .unwrap()
            .is_zero());
        assert_eq!(
            1.to_digest(),
            partial_trie.value_hash(&key!("0a711355")).unwrap()
        );
        assert!(partial_trie
            .value_hash(&key!("0a705678"))
            .unwrap()
            .is_zero());
        assert!(partial_trie
            .value_hash(&key!("0a775678"))
            .unwrap()
            .is_zero());
        assert_eq!(
            2.to_digest(),
            partial_trie.value_hash(&key!("0a77d337")).unwrap()
        );
        assert!(partial_trie.value_hash(&key!("0a7f9365")).is_none());
        assert!(partial_trie.value_hash(&key!("0a77d397")).is_none());

        let p2: Proof = partial_trie.into();
        assert_eq!(trie.root, p2.root_hash());
    }
}

#[cfg(feature = "write")]
#[test]
fn test_trie_write() {
    let mut trie = TestTrie::default();
    let mut ctx: WriteTrieContext<Key, _, _> = WriteTrieContext::new(&trie, trie.root);
    ctx.insert(&key!("0a711355"), 1.into()).unwrap();
    ctx.insert(&key!("0a77d337"), 2.into()).unwrap();
    ctx.insert(&key!("0a7f9365"), 3.into()).unwrap();
    ctx.insert(&key!("0a77d397"), 4.into()).unwrap();
    ctx.insert(&key!("0a701234"), 5.into()).unwrap();
    ctx.insert(&key!("0a701256"), 6.into()).unwrap();
    ctx.insert(&key!("0a701234"), 0.into()).unwrap();
    ctx.insert(&key!("0a701256"), 0.into()).unwrap();
    let changes = ctx.changes();
    trie.apply(changes);
    assert_eq!(build_test_trie(), trie);

    #[cfg(feature = "partial_trie")]
    {
        let mut ctx: WritePartialTrieContext<Key> =
            WritePartialTrieContext::new(PartialTrie::new());
        ctx.insert_with_value(&key!("0a711355"), &Value(1)).unwrap();
        ctx.insert_with_value(&key!("0a77d337"), &Value(2)).unwrap();
        ctx.insert_with_value(&key!("0a7f9365"), &Value(3)).unwrap();
        ctx.insert_with_value(&key!("0a77d397"), &Value(4)).unwrap();
        ctx.insert_with_value(&key!("0a701234"), &Value(5)).unwrap();
        ctx.insert_with_value(&key!("0a701256"), &Value(6)).unwrap();
        ctx.insert_with_value(&key!("0a701234"), &Value(0)).unwrap();
        ctx.insert_with_value(&key!("0a701256"), &Value(0)).unwrap();
        let trie2 = ctx.finish();
        assert_eq!(trie2.root_hash(), trie.root);
    }
}

#[cfg(all(feature = "partial_trie", feature = "read", feature = "write"))]
#[test]
fn test_partial_trie_update() {
    let trie1 = build_test_trie();

    let mut read_ctx: ReadTrieContext<Key, _, _> = ReadTrieContext::new(&trie1, trie1.root);
    read_ctx.read(&key!("0a77d337")).unwrap();
    read_ctx.read(&key!("0b123456")).unwrap();
    let partial_trie1: PartialTrie = read_ctx.into_proof().into();
    assert_eq!(trie1.root, partial_trie1.root_hash());

    let mut write_ctx: WriteTrieContext<Key, _, _> = WriteTrieContext::new(&trie1, trie1.root);
    write_ctx.insert(&key!("0a77d337"), 5.into()).unwrap();
    write_ctx.insert(&key!("0b123456"), 6.into()).unwrap();
    let mut trie2 = trie1.clone();
    trie2.apply(write_ctx.changes());

    let mut partial_trie2_write_ctx: WritePartialTrieContext<Key> =
        WritePartialTrieContext::new(partial_trie1);
    partial_trie2_write_ctx
        .insert_with_value(&key!("0a77d337"), &Value(5))
        .unwrap();
    partial_trie2_write_ctx
        .insert_with_value(&key!("0b123456"), &Value(6))
        .unwrap();
    let partial_trie2 = partial_trie2_write_ctx.finish();

    assert_eq!(trie2.root, partial_trie2.root_hash());

    let mut read_ctx: ReadTrieContext<Key, _, _> = ReadTrieContext::new(&trie1, trie1.root);
    read_ctx.read(&key!("0a7113ab")).unwrap();
    read_ctx.read(&key!("0b123abc")).unwrap();
    let partial_trie_missing1: PartialTrie = read_ctx.into_proof().into();
    assert_eq!(trie1.root, partial_trie_missing1.root_hash());

    let mut read_ctx: ReadTrieContext<Key, _, _> = ReadTrieContext::new(&trie2, trie2.root);
    read_ctx.read(&key!("0a7f9312")).unwrap();
    read_ctx.read(&key!("0a7d1234")).unwrap();
    let partial_trie_missing2: PartialTrie = read_ctx.into_proof().into();
    assert_eq!(trie2.root, partial_trie_missing2.root_hash());

    let diff_missing1 =
        diff_missing_branches(&partial_trie2, &partial_trie_missing1, true).unwrap();
    let diff_missing2 =
        diff_missing_branches(&partial_trie2, &partial_trie_missing2, true).unwrap();
    let diff = merge_diff(&diff_missing1, &diff_missing2);

    let partial_trie2_merge = apply_diff(&partial_trie2, &diff, true).unwrap();
    assert_eq!(trie2.root, partial_trie2_merge.root_hash());

    let mut write_ctx: WriteTrieContext<Key, _, _> = WriteTrieContext::new(&trie2, trie2.root);
    write_ctx.insert(&key!("0a7f9312"), 7.into()).unwrap();
    write_ctx.insert(&key!("0a7d1234"), 8.into()).unwrap();
    write_ctx.insert(&key!("0a7113ab"), 9.into()).unwrap();
    write_ctx.insert(&key!("0b123abc"), 10.into()).unwrap();
    let mut trie3 = trie2.clone();
    trie3.apply(write_ctx.changes());

    let mut partial_trie3_write_ctx: WritePartialTrieContext<Key> =
        WritePartialTrieContext::new(partial_trie2_merge);
    partial_trie3_write_ctx
        .insert_with_value(&key!("0a7f9312"), &Value(7))
        .unwrap();
    partial_trie3_write_ctx
        .insert_with_value(&key!("0a7d1234"), &Value(8))
        .unwrap();
    partial_trie3_write_ctx
        .insert_with_value(&key!("0a7113ab"), &Value(9))
        .unwrap();
    partial_trie3_write_ctx
        .insert_with_value(&key!("0b123abc"), &Value(10))
        .unwrap();
    let partial_trie3 = partial_trie3_write_ctx.finish();
    assert_eq!(trie3.root, partial_trie3.root_hash());
}

#[cfg(all(feature = "partial_trie", feature = "read", feature = "write"))]
#[test]
fn test_partial_trie_update_whole_trie() {
    let trie1 = build_test_trie();

    let mut read_ctx: ReadTrieContext<Key, _, _> = ReadTrieContext::new(&trie1, trie1.root);
    read_ctx.read(&key!("0a77d337")).unwrap();
    read_ctx.read(&key!("0b123456")).unwrap();
    let partial_trie1: PartialTrie = read_ctx.into_proof().into();
    assert_eq!(trie1.root, partial_trie1.root_hash());

    let partial_trie2 = PartialTrie::from_root_hash(trie1.root);
    let diff = diff_missing_branches(&partial_trie2, &partial_trie1, true).unwrap();
    let partial_trie3 = apply_diff(&partial_trie2, &diff, true).unwrap();
    assert_eq!(trie1.root, partial_trie3.root_hash());

    let diff2 = PartialTrieDiff::diff_from_empty(&partial_trie1);
    assert_eq!(diff, diff2);
    let partial_trie4 = diff2.to_standalone_trie().unwrap();
    assert_eq!(trie1.root, partial_trie4.root_hash());
}

#[cfg(all(feature = "partial_trie", feature = "read", feature = "write"))]
#[test]
fn test_partial_trie_prune() {
    let trie = build_test_trie();

    let mut read_ctx: ReadTrieContext<Key, _, _> = ReadTrieContext::new(&trie, trie.root);
    read_ctx.read(&key!("0a77d337")).unwrap();
    read_ctx.read(&key!("0a711355")).unwrap();

    let partial_trie: PartialTrie = read_ctx.into_proof().into();

    let t1 = prune_unused_key(&partial_trie, &key!("0a77d337")).unwrap();
    assert!(!t1.can_be_pruned());
    assert_eq!(t1.root_hash(), trie.root);

    let t2 = prune_unused_key(&partial_trie, &key!("0a711355")).unwrap();
    assert!(!t2.can_be_pruned());
    assert_eq!(t2.root_hash(), trie.root);

    let t3 = prune_unused_keys(
        &partial_trie,
        [key!("0a77d337"), key!("0a711355")].iter().cloned(),
    )
    .unwrap();
    assert!(t3.can_be_pruned());
    assert_eq!(t3.root_hash(), trie.root);
}
