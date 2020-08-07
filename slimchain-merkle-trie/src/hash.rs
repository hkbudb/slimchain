use crate::nibbles::NibbleBuf;
use slimchain_common::{
    basic::H256,
    digest::{blake2b_hash_to_h256, default_blake2, Digestible},
};

pub(crate) fn extension_node_hash(nibbles: &NibbleBuf, child_hash: H256) -> H256 {
    if child_hash.is_zero() {
        return H256::zero();
    }

    let mut hash_state = default_blake2().to_state();
    hash_state.update(nibbles.to_digest().as_bytes());
    hash_state.update(child_hash.as_bytes());
    blake2b_hash_to_h256(hash_state.finalize())
}

pub(crate) fn branch_node_hash(children: impl Iterator<Item = Option<H256>>) -> H256 {
    let mut has_child = false;
    let mut hash_state = default_blake2().to_state();

    for child in children {
        let child_hash = child.unwrap_or_else(H256::zero);
        has_child = has_child || (!child_hash.is_zero());
        hash_state.update(child_hash.as_bytes());
    }

    if !has_child {
        return H256::zero();
    }

    blake2b_hash_to_h256(hash_state.finalize())
}

pub(crate) fn leaf_node_hash(nibbles: &NibbleBuf, value_hash: H256) -> H256 {
    if value_hash.is_zero() {
        return H256::zero();
    }

    let mut hash_state = default_blake2().to_state();
    hash_state.update(nibbles.to_digest().as_bytes());
    hash_state.update(value_hash.as_bytes());
    blake2b_hash_to_h256(hash_state.finalize())
}
