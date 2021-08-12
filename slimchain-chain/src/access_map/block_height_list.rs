use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::BlockHeight,
    utils::derive_more::{Deref, DerefMut},
};
use std::iter::FromIterator;

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize, Deref, DerefMut)]
pub struct BlockHeightList(imbl::Vector<BlockHeight>);

impl<V: Into<BlockHeight>> FromIterator<V> for BlockHeightList {
    fn from_iter<T: IntoIterator<Item = V>>(iter: T) -> Self {
        let list = Self(iter.into_iter().map(|v| v.into()).collect());
        assert!(list.is_monotonic_increasing());
        list
    }
}

impl BlockHeightList {
    pub fn is_monotonic_increasing(&self) -> bool {
        let mut iter = self.iter();
        if let Some(first) = iter.next() {
            let mut last = *first;
            for &v in iter {
                if v <= last {
                    return false;
                }
                last = v;
            }
        }

        true
    }

    pub fn add_block_height(&mut self, block_height: impl Into<BlockHeight>) {
        let block_height = block_height.into();
        if self.back() != Some(&block_height) {
            self.push_back(block_height);
        }
        debug_assert!(self.is_monotonic_increasing());
        debug_assert_eq!(self.back(), Some(&block_height));
    }

    pub fn remove_block_height(&mut self, block_height: impl Into<BlockHeight>) {
        let block_height = block_height.into();
        debug_assert_eq!(self.front(), Some(&block_height));
        self.pop_front();
    }

    pub fn conflicts_with(&self, other: impl Into<BlockHeight>) -> bool {
        let other = other.into();
        match self.back() {
            Some(&last) => other < last,
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_height_list() {
        let list1 = BlockHeightList::default();
        assert!(!list1.conflicts_with(1));

        let list2: BlockHeightList = [2].iter().copied().collect();
        assert!(list2.conflicts_with(1));
        assert!(!list2.conflicts_with(2));
        assert!(!list2.conflicts_with(3));

        let list3: BlockHeightList = [3, 5].iter().copied().collect();
        assert!(list3.conflicts_with(1));
        assert!(list3.conflicts_with(4));
        assert!(!list3.conflicts_with(5));
        assert!(!list3.conflicts_with(6));
        assert!(!list3.conflicts_with(8));
    }
}
