use crate::basic::H256;
use crate::digest::Digestible;
use alloc::vec::Vec;
use core::fmt;

#[derive(
    Default,
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
    derive_more::From,
    derive_more::Into,
)]
pub struct Code(pub Vec<u8>);

impl Digestible for Code {
    fn to_digest(&self) -> H256 {
        if self.is_empty() {
            H256::zero()
        } else {
            self.0.to_digest()
        }
    }
}

impl fmt::Debug for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Code [len={}]", self.len())
    }
}

impl Code {
    pub fn new() -> Self {
        Self::default()
    }
}
