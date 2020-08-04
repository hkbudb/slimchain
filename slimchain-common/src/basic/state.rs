use crate::basic::H256;
use crate::digest::Digestible;

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
pub struct StateKey(pub H256);

impl Digestible for StateKey {
    fn to_digest(&self) -> H256 {
        self.0
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
pub struct StateValue(pub H256);

impl Digestible for StateValue {
    fn to_digest(&self) -> H256 {
        self.0
    }
}

impl From<u64> for StateValue {
    fn from(input: u64) -> Self {
        H256::from_low_u64_le(input).into()
    }
}
