use core::fmt;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub struct U4(pub u8);

impl From<u8> for U4 {
    fn from(value: u8) -> Self {
        Self::from_u8(value)
    }
}

impl From<usize> for U4 {
    fn from(value: usize) -> Self {
        Self::from_u8(value as u8)
    }
}

impl From<U4> for u8 {
    fn from(value: U4) -> Self {
        value.0
    }
}

impl From<U4> for usize {
    fn from(value: U4) -> Self {
        value.0 as usize
    }
}

impl fmt::Display for U4 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let &Self(ref value) = self;
        <u8 as fmt::Display>::fmt(value, f)
    }
}

impl fmt::UpperHex for U4 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let &Self(ref value) = self;
        <u8 as fmt::UpperHex>::fmt(value, f)
    }
}

impl fmt::LowerHex for U4 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let &Self(ref value) = self;
        <u8 as fmt::LowerHex>::fmt(value, f)
    }
}

impl fmt::Octal for U4 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let &Self(ref value) = self;
        <u8 as fmt::Octal>::fmt(value, f)
    }
}

impl fmt::Binary for U4 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let &Self(ref value) = self;
        <u8 as fmt::Binary>::fmt(value, f)
    }
}

impl U4 {
    pub const MAX: Self = Self(0xf);
    pub const MIN: Self = Self(0x0);

    pub fn min_value() -> Self {
        Self::MIN
    }

    pub fn max_value() -> Self {
        Self::MAX
    }

    pub fn new(value: u8) -> Self {
        Self::from_u8(value)
    }

    pub fn from_u8(value: u8) -> Self {
        assert!(value & 0xf0 == 0);
        Self(value)
    }

    #[allow(clippy::missing_safety_doc)]
    pub unsafe fn from_u8_unchecked(value: u8) -> Self {
        debug_assert!(value & 0xf0 == 0);
        Self(value)
    }
}
