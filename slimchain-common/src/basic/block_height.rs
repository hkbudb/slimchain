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
pub struct BlockHeight(pub u64);

impl Digestible for BlockHeight {
    fn to_digest(&self) -> H256 {
        self.0.to_digest()
    }
}
