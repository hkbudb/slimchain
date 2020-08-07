use crate::nibbles::AsNibbles;
use core::hash::Hash;
use slimchain_common::digest::Digestible;

pub trait Key: Clone + Eq + Hash + AsNibbles {}
impl<T: Clone + Eq + Hash + AsNibbles> Key for T {}

pub trait Value: Clone + Digestible {}
impl<T: Clone + Digestible> Value for T {}
