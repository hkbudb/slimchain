use crate::basic::{H160, H256};
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
pub struct Address(pub H160);

impl Digestible for Address {
    fn to_digest(&self) -> H256 {
        self.0.as_bytes().to_digest()
    }
}
