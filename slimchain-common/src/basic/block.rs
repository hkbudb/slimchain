use crate::basic::H256;
use crate::digest::Digestible;
use core::ops::{Add, AddAssign, Sub, SubAssign};

#[derive(
    Debug,
    Default,
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    derive_more::Deref,
    derive_more::DerefMut,
    derive_more::Display,
    derive_more::From,
    derive_more::Into,
)]
pub struct BlockHeight(pub u64);

impl Digestible for BlockHeight {
    fn to_digest(&self) -> H256 {
        self.0.to_digest()
    }
}

impl BlockHeight {
    pub fn prev_height(self) -> Self {
        debug_assert!(!self.is_zero());
        Self(self.0 - 1)
    }

    pub fn next_height(self) -> Self {
        Self(self.0 + 1)
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }
}

#[derive(
    Debug,
    Default,
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    derive_more::Deref,
    derive_more::DerefMut,
    derive_more::Display,
    derive_more::From,
    derive_more::Into,
)]
pub struct BlockDistance(pub i64);

impl Sub<Self> for BlockHeight {
    type Output = BlockDistance;

    fn sub(self, rhs: Self) -> Self::Output {
        BlockDistance(self.0 as i64 - rhs.0 as i64)
    }
}

impl Add<BlockDistance> for BlockHeight {
    type Output = BlockHeight;

    fn add(self, rhs: BlockDistance) -> Self::Output {
        BlockHeight((self.0 as i64 + rhs.0) as u64)
    }
}

impl Sub<BlockDistance> for BlockHeight {
    type Output = BlockHeight;

    fn sub(self, rhs: BlockDistance) -> Self::Output {
        BlockHeight((self.0 as i64 - rhs.0) as u64)
    }
}

impl AddAssign<BlockDistance> for BlockHeight {
    fn add_assign(&mut self, rhs: BlockDistance) {
        *self = *self + rhs
    }
}

impl SubAssign<BlockDistance> for BlockHeight {
    fn sub_assign(&mut self, rhs: BlockDistance) {
        *self = *self - rhs
    }
}
