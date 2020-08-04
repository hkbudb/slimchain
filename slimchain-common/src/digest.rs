use crate::basic::{H160, H256};

pub use blake2b_simd::{Hash as Blake2bHash, Params as Blake2bParams};

#[inline]
pub fn blake2b_hash_to_h160(input: Blake2bHash) -> H160 {
    H160::from_slice(input.as_bytes())
}

#[inline]
pub fn blake2b_hash_to_h256(input: Blake2bHash) -> H256 {
    H256::from_slice(input.as_bytes())
}

pub const DEFAULT_DIGEST_LEN: usize = 32;

#[inline]
pub fn blake2(size: usize) -> Blake2bParams {
    let mut params = Blake2bParams::new();
    params.hash_length(size);
    params
}

#[inline]
pub fn default_blake2() -> Blake2bParams {
    blake2(DEFAULT_DIGEST_LEN)
}

pub trait Digestible {
    fn to_digest(&self) -> H256;
}

impl Digestible for [u8] {
    fn to_digest(&self) -> H256 {
        let hash = default_blake2().hash(self);
        blake2b_hash_to_h256(hash)
    }
}

impl Digestible for alloc::vec::Vec<u8> {
    fn to_digest(&self) -> H256 {
        self.as_slice().to_digest()
    }
}

impl Digestible for str {
    fn to_digest(&self) -> H256 {
        self.as_bytes().to_digest()
    }
}

impl Digestible for alloc::string::String {
    fn to_digest(&self) -> H256 {
        self.as_bytes().to_digest()
    }
}

macro_rules! impl_digestible_for_numeric {
    ($x: ty) => {
        impl Digestible for $x {
            fn to_digest(&self) -> H256 {
                self.to_le_bytes().to_digest()
            }
        }
    };
    ($($x: ty),*) => {$(impl_digestible_for_numeric!($x);)*}
}

impl_digestible_for_numeric!(i8, i16, i32, i64);
impl_digestible_for_numeric!(u8, u16, u32, u64);
impl_digestible_for_numeric!(f32, f64);

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn test_to_digest() {
        let expect = H256(*b"\x32\x4d\xcf\x02\x7d\xd4\xa3\x0a\x93\x2c\x44\x1f\x36\x5a\x25\xe8\x6b\x17\x3d\xef\xa4\xb8\xe5\x89\x48\x25\x34\x71\xb8\x1b\x72\xcf");
        assert_eq!(b"hello"[..].to_digest(), expect);
        assert_eq!("hello".to_digest(), expect);
        assert_eq!("hello".to_string().to_digest(), expect);
    }
}
