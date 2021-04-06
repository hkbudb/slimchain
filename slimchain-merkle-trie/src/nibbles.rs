use crate::u4::U4;
use alloc::{format, string::String, vec::Vec};
use core::{
    cmp::Ordering,
    fmt,
    iter::{FromIterator, IntoIterator, Iterator},
};
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{Address, StateKey, H160, H256, H512},
    digest::Digestible,
    error::{anyhow, Result},
};

pub trait AsNibbles {
    fn as_nibbles(&self) -> Nibbles<'_>;
}

impl<'a, T: AsNibbles> AsNibbles for &'a T {
    fn as_nibbles(&self) -> Nibbles<'_> {
        (*self).as_nibbles()
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Hash)]
pub struct NibbleBuf {
    pub data: Vec<u8>,
    pub skip_last: bool,
}

impl fmt::Debug for NibbleBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("NibbleBuf").field(&self.hex_str()).finish()
    }
}

impl fmt::Display for NibbleBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.hex_str())
    }
}

impl Default for NibbleBuf {
    fn default() -> Self {
        NibbleBuf {
            data: Vec::new(),
            skip_last: false,
        }
    }
}

impl Ord for NibbleBuf {
    fn cmp(&self, other: &Self) -> Ordering {
        self.data
            .cmp(&other.data)
            .then_with(|| other.skip_last.cmp(&self.skip_last))
    }
}

impl PartialOrd for NibbleBuf {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl AsNibbles for NibbleBuf {
    fn as_nibbles(&self) -> Nibbles<'_> {
        Nibbles {
            data: &self.data[..],
            skip_first: false,
            skip_last: self.skip_last,
        }
    }
}

impl Digestible for NibbleBuf {
    fn to_digest(&self) -> H256 {
        self.hex_str().to_digest()
    }
}

impl FromIterator<U4> for NibbleBuf {
    fn from_iter<I: IntoIterator<Item = U4>>(iter: I) -> Self {
        let mut len: usize = 0;
        let mut last: u8 = 0;
        let mut data: Vec<u8> = Vec::new();
        let mut skip_last = false;

        for v in iter {
            if len % 2 == 0 {
                last = v.0 << 4;
            } else {
                last |= v.0;
                data.push(last);
            }
            len += 1;
        }

        if len % 2 == 1 {
            skip_last = true;
            data.push(last);
        }

        NibbleBuf { data, skip_last }
    }
}

impl NibbleBuf {
    pub fn try_from_hex_str(s: &str) -> Result<Self> {
        s.as_bytes()
            .iter()
            .map(|&c| match c {
                b'0'..=b'9' => Ok(U4::from(c - b'0')),
                b'a'..=b'f' => Ok(U4::from(c - b'a' + 10)),
                b'A'..=b'F' => Ok(U4::from(c - b'A' + 10)),
                _ => Err(anyhow!("invalid input {:?}", s)),
            })
            .collect()
    }

    pub fn from_hex_str(s: &str) -> Self {
        Self::try_from_hex_str(s).expect("Failed to create NibbleBuf from hex string.")
    }

    pub fn hex_str(&self) -> String {
        self.as_nibbles().hex_str()
    }

    pub fn iter(&self) -> NibbleIterator<'_> {
        NibbleIterator::new(self.as_nibbles())
    }

    pub fn len(&self) -> usize {
        let mut out = self.data.len() * 2;
        if self.skip_last && out > 0 {
            out -= 1;
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, pos: usize) -> Option<U4> {
        if pos >= self.len() {
            return None;
        }
        self.data.get(pos / 2).map(|&v| {
            if pos % 2 == 0 {
                unsafe { U4::from_u8_unchecked((v & 0xf0) >> 4) }
            } else {
                unsafe { U4::from_u8_unchecked(v & 0x0f) }
            }
        })
    }
}

#[derive(Clone, Copy)]
pub struct Nibbles<'a> {
    pub data: &'a [u8],
    pub skip_first: bool,
    pub skip_last: bool,
}

impl fmt::Debug for Nibbles<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Nibbles").field(&self.hex_str()).finish()
    }
}

impl fmt::Display for Nibbles<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.hex_str())
    }
}

impl Default for Nibbles<'_> {
    fn default() -> Self {
        Self::new(&[])
    }
}

impl<'a> PartialEq for Nibbles<'a> {
    fn eq<'b>(&self, other: &Nibbles<'b>) -> bool {
        self.len() == other.len() && self.iter().zip(other.iter()).all(|(a, b)| a == b)
    }
}

impl AsNibbles for Nibbles<'_> {
    fn as_nibbles(&self) -> Nibbles<'_> {
        *self
    }
}

impl<'a> Nibbles<'a> {
    pub fn new(bytes: &[u8]) -> Nibbles<'_> {
        Nibbles {
            data: bytes,
            skip_first: false,
            skip_last: false,
        }
    }

    pub fn hex_str(&self) -> String {
        let mut s = String::with_capacity(self.len());
        for v in self.iter() {
            s.push_str(&format!("{:x}", v));
        }
        s
    }

    pub fn iter(&self) -> NibbleIterator<'_> {
        NibbleIterator::new(*self)
    }

    pub fn len(&self) -> usize {
        let mut out = self.data.len() * 2;
        if self.skip_first && out > 0 {
            out -= 1;
        }
        if self.skip_last && out > 0 {
            out -= 1;
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, mut pos: usize) -> Option<U4> {
        if pos >= self.len() {
            return None;
        }
        if self.skip_first {
            pos += 1;
        }
        self.data.get(pos / 2).map(|&v| {
            if pos % 2 == 0 {
                unsafe { U4::from_u8_unchecked((v & 0xf0) >> 4) }
            } else {
                unsafe { U4::from_u8_unchecked(v & 0xf) }
            }
        })
    }

    pub fn split_at(&self, mid: usize) -> (Nibbles<'a>, Nibbles<'a>) {
        debug_assert!(mid <= self.len());
        let data_mid = mid / 2;
        let mut lhs_data_mid = data_mid;
        let mut rhs_data_mid = data_mid;
        let skip_at_mid = match (self.skip_first, mid % 2 == 0) {
            (true, true) => {
                lhs_data_mid += 1;
                true
            }
            (true, false) => {
                lhs_data_mid += 1;
                rhs_data_mid += 1;
                false
            }
            (false, true) => false,
            (false, false) => {
                lhs_data_mid += 1;
                true
            }
        };
        let lhs = Nibbles {
            data: &self.data[..lhs_data_mid],
            skip_first: self.skip_first,
            skip_last: skip_at_mid,
        };
        let rhs = Nibbles {
            data: &self.data[rhs_data_mid..],
            skip_first: skip_at_mid,
            skip_last: self.skip_last,
        };
        (lhs, rhs)
    }

    pub fn split_first(&self) -> Option<(U4, Nibbles<'a>)> {
        if self.is_empty() {
            return None;
        }

        let (lhs, rhs) = self.split_at(1);
        Some((lhs.get(0).unwrap(), rhs))
    }

    pub fn common_prefix_len(&self, other: &impl AsNibbles) -> usize {
        self.iter()
            .zip(other.as_nibbles().iter())
            .take_while(|(a, b)| a == b)
            .count()
    }

    pub fn is_starting_with(&self, prefix: &impl AsNibbles) -> bool {
        self.common_prefix_len(prefix) == prefix.as_nibbles().len()
    }

    pub fn strip_prefix(&self, prefix: &impl AsNibbles) -> Option<Nibbles<'a>> {
        let prefix = prefix.as_nibbles();
        if self.common_prefix_len(&prefix) == prefix.len() {
            Some(self.split_at(prefix.len()).1)
        } else {
            None
        }
    }

    pub fn to_nibble_buf(&self) -> NibbleBuf {
        if self.skip_first {
            let mut data = Vec::with_capacity(self.data.len());
            let mut last = (self.data[0] & 0x0f) << 4;
            for v in self.data.iter().skip(1) {
                data.push(((v & 0xf0) >> 4) | last);
                last = (v & 0x0f) << 4;
            }

            if !self.skip_last {
                data.push(last);
            }

            NibbleBuf {
                data,
                skip_last: !self.skip_last,
            }
        } else {
            let mut data = self.data.to_vec();

            if self.skip_last {
                *data.last_mut().unwrap() &= 0xf0;
            }

            NibbleBuf {
                data,
                skip_last: self.skip_last,
            }
        }
    }
}

pub struct NibbleIterator<'a> {
    slice: Nibbles<'a>,
    pos: usize,
    len: usize,
}

impl<'a> NibbleIterator<'a> {
    fn new(slice: Nibbles<'a>) -> Self {
        Self {
            slice,
            pos: 0,
            len: slice.len(),
        }
    }
}

impl<'a> Iterator for NibbleIterator<'a> {
    type Item = U4;

    fn next(&mut self) -> Option<Self::Item> {
        let cur = self.pos;
        self.pos += 1;
        self.slice.get(cur)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let size = self.len - self.pos;
        (size, Some(size))
    }
}

impl<'a> From<&'a H160> for Nibbles<'a> {
    fn from(input: &'a H160) -> Nibbles<'a> {
        Nibbles::new(input.as_bytes())
    }
}

impl<'a> From<&'a H256> for Nibbles<'a> {
    fn from(input: &'a H256) -> Nibbles<'a> {
        Nibbles::new(input.as_bytes())
    }
}

impl<'a> From<&'a H512> for Nibbles<'a> {
    fn from(input: &'a H512) -> Nibbles<'a> {
        Nibbles::new(input.as_bytes())
    }
}

impl<'a> From<&'a Address> for Nibbles<'a> {
    fn from(input: &'a Address) -> Nibbles<'a> {
        Nibbles::new(input.as_bytes())
    }
}

impl<'a> From<&'a StateKey> for Nibbles<'a> {
    fn from(input: &'a StateKey) -> Nibbles<'a> {
        Nibbles::new(input.as_bytes())
    }
}

impl From<NibbleBuf> for H160 {
    fn from(input: NibbleBuf) -> Self {
        assert_eq!(40, input.len());
        H160::from_slice(&input.data)
    }
}

impl From<NibbleBuf> for H256 {
    fn from(input: NibbleBuf) -> Self {
        assert_eq!(64, input.len());
        H256::from_slice(&input.data)
    }
}

impl From<NibbleBuf> for H512 {
    fn from(input: NibbleBuf) -> Self {
        assert_eq!(128, input.len());
        H512::from_slice(&input.data)
    }
}

impl From<NibbleBuf> for Address {
    fn from(input: NibbleBuf) -> Self {
        let out: H160 = input.into();
        Address::from(out)
    }
}

impl From<NibbleBuf> for StateKey {
    fn from(input: NibbleBuf) -> Self {
        let out: H256 = input.into();
        StateKey::from(out)
    }
}

impl AsNibbles for H160 {
    fn as_nibbles(&self) -> Nibbles<'_> {
        self.into()
    }
}

impl AsNibbles for H256 {
    fn as_nibbles(&self) -> Nibbles<'_> {
        self.into()
    }
}

impl AsNibbles for H512 {
    fn as_nibbles(&self) -> Nibbles<'_> {
        self.into()
    }
}

impl AsNibbles for Address {
    fn as_nibbles(&self) -> Nibbles<'_> {
        self.into()
    }
}

impl AsNibbles for StateKey {
    fn as_nibbles(&self) -> Nibbles<'_> {
        self.into()
    }
}

/// return (common, remaining1, remaining2).
pub fn split_at_common_prefix<'a, 'b>(
    nibbles1: &Nibbles<'a>,
    nibbles2: &Nibbles<'b>,
) -> (Nibbles<'a>, Nibbles<'a>, Nibbles<'b>) {
    let prefix_len = nibbles1.common_prefix_len(nibbles2);
    let (common, remaining1) = nibbles1.split_at(prefix_len);
    let (_, remaining2) = nibbles2.split_at(prefix_len);
    (common, remaining1, remaining2)
}

/// return (common, remaining1, remaining2).
pub fn split_at_common_prefix_buf(
    nibbles1: &impl AsNibbles,
    nibbles2: &impl AsNibbles,
) -> (NibbleBuf, NibbleBuf, NibbleBuf) {
    let nibbles1 = nibbles1.as_nibbles();
    let nibbles2 = nibbles2.as_nibbles();
    let (common, remaining1, remaining2) = split_at_common_prefix(&nibbles1, &nibbles2);
    (
        common.to_nibble_buf(),
        remaining1.to_nibble_buf(),
        remaining2.to_nibble_buf(),
    )
}

/// return (common, first1, remaining1, first2, remaining2).
pub fn split_at_common_prefix_buf2(
    nibbles1: &impl AsNibbles,
    nibbles2: &impl AsNibbles,
) -> (NibbleBuf, U4, NibbleBuf, U4, NibbleBuf) {
    let nibbles1 = nibbles1.as_nibbles();
    let nibbles2 = nibbles2.as_nibbles();
    let (common, remaining1, remaining2) = split_at_common_prefix(&nibbles1, &nibbles2);
    let (first1, remaining1) = remaining1
        .split_first()
        .expect("Remaining nibbles are empty.");
    let (first2, remaining2) = remaining2
        .split_first()
        .expect("Remaining nibbles are empty.");
    (
        common.to_nibble_buf(),
        first1,
        remaining1.to_nibble_buf(),
        first2,
        remaining2.to_nibble_buf(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_from_hex_str() {
        let nibble_buf1 = NibbleBuf {
            data: b"\x12\xab".to_vec(),
            skip_last: false,
        };
        let nibble_buf2 = NibbleBuf {
            data: b"\x12\xa0".to_vec(),
            skip_last: true,
        };
        assert_eq!(NibbleBuf::from_hex_str("12ab"), nibble_buf1);
        assert_eq!(NibbleBuf::from_hex_str("12AB"), nibble_buf1);
        assert_eq!(NibbleBuf::from_hex_str("12a"), nibble_buf2);
        assert_eq!(NibbleBuf::from_hex_str("12A"), nibble_buf2);
        assert!(NibbleBuf::try_from_hex_str("XYZ").is_err());
    }

    #[test]
    fn test_hex_str() {
        let nibble_buf1 = NibbleBuf {
            data: b"\x12\xab".to_vec(),
            skip_last: false,
        };
        let nibble_buf2 = NibbleBuf {
            data: b"\x12\xa0".to_vec(),
            skip_last: true,
        };
        assert_eq!("12ab", nibble_buf1.hex_str());
        assert_eq!("12a", nibble_buf2.hex_str());
    }

    #[test]
    fn test_nibble_buf_iter() {
        assert!(core::iter::empty().collect::<NibbleBuf>().is_empty());
        let nibble_buf1 = NibbleBuf {
            data: b"\x12\x34".to_vec(),
            skip_last: false,
        };
        assert_eq!(4, nibble_buf1.len());
        assert_eq!(
            vec![b'\x01', b'\x02', b'\x03', b'\x04'],
            nibble_buf1.iter().map(|x| x.into()).collect::<Vec<_>>()
        );
        assert_eq!(nibble_buf1, nibble_buf1.iter().collect::<NibbleBuf>());
        let nibble_buf2 = NibbleBuf {
            data: b"\x12\x30".to_vec(),
            skip_last: true,
        };
        assert_eq!(3, nibble_buf2.len());
        assert_eq!(
            vec![b'\x01', b'\x02', b'\x03'],
            nibble_buf2.iter().map(|x| x.into()).collect::<Vec<_>>()
        );
        assert_eq!(nibble_buf2, nibble_buf2.iter().collect::<NibbleBuf>());
    }

    #[test]
    fn test_nibble_buf_get() {
        let nibble_buf = NibbleBuf {
            data: b"\x12\x30".to_vec(),
            skip_last: true,
        };
        assert_eq!(Some(U4(b'\x01')), nibble_buf.get(0));
        assert_eq!(Some(U4(b'\x02')), nibble_buf.get(1));
        assert_eq!(Some(U4(b'\x03')), nibble_buf.get(2));
        assert_eq!(None, nibble_buf.get(3));
    }

    #[test]
    fn test_nibbles_iter() {
        let bytes = b"\x12\x34";
        let nibbles1 = Nibbles {
            data: &bytes[..],
            skip_first: false,
            skip_last: false,
        };
        assert_eq!(4, nibbles1.len());
        assert_eq!(
            vec![b'\x01', b'\x02', b'\x03', b'\x04'],
            nibbles1.iter().map(|x| x.into()).collect::<Vec<_>>()
        );
        let nibbles2 = Nibbles {
            data: &bytes[..],
            skip_first: true,
            skip_last: false,
        };
        assert_eq!(3, nibbles2.len());
        assert_eq!(
            vec![b'\x02', b'\x03', b'\x04'],
            nibbles2.iter().map(|x| x.into()).collect::<Vec<_>>()
        );
        let nibbles3 = Nibbles {
            data: &bytes[..],
            skip_first: false,
            skip_last: true,
        };
        assert_eq!(3, nibbles3.len());
        assert_eq!(
            vec![b'\x01', b'\x02', b'\x03'],
            nibbles3.iter().map(|x| x.into()).collect::<Vec<_>>()
        );
        let nibbles4 = Nibbles {
            data: &bytes[..],
            skip_first: true,
            skip_last: true,
        };
        assert_eq!(2, nibbles4.len());
        assert_eq!(
            vec![b'\x02', b'\x03'],
            nibbles4.iter().map(|x| x.into()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_split() {
        let bytes = b"\x12\x34\x56\x78";
        let nibbles1 = Nibbles {
            data: &bytes[..],
            skip_first: false,
            skip_last: false,
        };
        let (l1, r1) = nibbles1.split_at(4);
        assert_eq!(
            NibbleBuf {
                data: b"\x12\x34".to_vec(),
                skip_last: false,
            },
            l1.to_nibble_buf(),
        );
        assert_eq!(
            NibbleBuf {
                data: b"\x56\x78".to_vec(),
                skip_last: false,
            },
            r1.to_nibble_buf(),
        );
        let (l2, r2) = nibbles1.split_at(5);
        assert_eq!(
            NibbleBuf {
                data: b"\x12\x34\x50".to_vec(),
                skip_last: true,
            },
            l2.to_nibble_buf(),
        );
        assert_eq!(
            NibbleBuf {
                data: b"\x67\x80".to_vec(),
                skip_last: true,
            },
            r2.to_nibble_buf(),
        );
        let nibbles2 = Nibbles {
            data: &bytes[..],
            skip_first: true,
            skip_last: true,
        };
        let (l3, r3) = nibbles2.split_at(3);
        assert_eq!(
            NibbleBuf {
                data: b"\x23\x40".to_vec(),
                skip_last: true,
            },
            l3.to_nibble_buf(),
        );
        assert_eq!(
            NibbleBuf {
                data: b"\x56\x70".to_vec(),
                skip_last: true,
            },
            r3.to_nibble_buf(),
        );
        let (l4, r4) = nibbles2.split_at(2);
        assert_eq!(
            NibbleBuf {
                data: b"\x23".to_vec(),
                skip_last: false,
            },
            l4.to_nibble_buf(),
        );
        assert_eq!(
            NibbleBuf {
                data: b"\x45\x67".to_vec(),
                skip_last: false,
            },
            r4.to_nibble_buf(),
        );
        let (l5, r5) = nibbles2.split_at(0);
        assert!(l5.is_empty());
        assert_eq!(nibbles2, r5);
        let (l6, r6) = nibbles2.split_at(6);
        assert_eq!(nibbles2, l6);
        assert!(r6.is_empty());
    }

    #[test]
    fn test_split_first() {
        let bytes = b"\x12\x34";
        let nibbles1 = Nibbles {
            data: &bytes[..],
            skip_first: false,
            skip_last: false,
        };
        let nibbles2 = Nibbles {
            data: &bytes[..1],
            skip_first: false,
            skip_last: true,
        };
        assert_eq!(None, Nibbles::default().split_first());
        assert_eq!(
            Some((
                U4(b'\x01'),
                Nibbles {
                    data: &bytes[..],
                    skip_first: true,
                    skip_last: false,
                }
            )),
            nibbles1.split_first()
        );
        assert_eq!(
            Some((
                U4(b'\x01'),
                Nibbles {
                    data: &[],
                    skip_first: false,
                    skip_last: false,
                }
            )),
            nibbles2.split_first()
        );
    }

    #[test]
    fn test_common_prefix() {
        let bytes1 = b"\x12\x34\x56";
        let bytes2 = b"\x01\x23\x45";
        let bytes3 = b"\x01\x25\x67";
        let bytes4 = b"\x12";
        let nibbles1 = Nibbles {
            data: &bytes1[..],
            skip_first: false,
            skip_last: false,
        };
        let nibbles2 = Nibbles::default();
        let nibbles3 = Nibbles {
            data: &bytes4[..],
            skip_first: false,
            skip_last: false,
        };
        assert_eq!(0, nibbles1.common_prefix_len(&nibbles2));
        assert!(nibbles1.is_starting_with(&nibbles2));
        assert_eq!(Some(nibbles1), nibbles1.strip_prefix(&nibbles2));
        assert!(nibbles1.is_starting_with(&nibbles3));
        assert_eq!(
            Some(String::from("3456")),
            nibbles1.strip_prefix(&nibbles3).map(|n| n.hex_str())
        );
        assert_eq!(
            0,
            nibbles1.common_prefix_len(&Nibbles {
                data: &bytes2[..],
                skip_first: false,
                skip_last: false,
            })
        );
        assert_eq!(
            None,
            nibbles1.strip_prefix(&Nibbles {
                data: &bytes2[..],
                skip_first: false,
                skip_last: false,
            })
        );
        assert_eq!(
            5,
            nibbles1.common_prefix_len(&Nibbles {
                data: &bytes2[..],
                skip_first: true,
                skip_last: false,
            })
        );
        assert_eq!(
            4,
            nibbles1.common_prefix_len(&Nibbles {
                data: &bytes2[..],
                skip_first: true,
                skip_last: true,
            })
        );
        assert_eq!(
            2,
            nibbles1.common_prefix_len(&Nibbles {
                data: &bytes3[..],
                skip_first: true,
                skip_last: false,
            })
        );
    }

    #[test]
    fn test_split_at_common_prefix() {
        let buf1 = NibbleBuf::from_hex_str("1234abcd");
        let buf2 = NibbleBuf::from_hex_str("1234567");
        let buf3 = NibbleBuf::from_hex_str("abcdef");
        let (common, remaining1, remaining2) = split_at_common_prefix_buf(&buf1, &buf2);
        assert_eq!("1234", common.hex_str());
        assert_eq!("abcd", remaining1.hex_str());
        assert_eq!("567", remaining2.hex_str());
        let (common, remaining1, remaining2) = split_at_common_prefix_buf(&buf1, &buf3);
        assert!(common.is_empty());
        assert_eq!(buf1, remaining1);
        assert_eq!(buf3, remaining2);
    }

    #[test]
    fn test_to_nibble_buf() {
        let bytes = b"\x12\x34\x56\x78\x9a";
        assert_eq!(
            NibbleBuf {
                data: b"\x12\x34\x56\x78\x9a".to_vec(),
                skip_last: false,
            },
            Nibbles {
                data: &bytes[..],
                skip_first: false,
                skip_last: false,
            }
            .to_nibble_buf()
        );
        assert_eq!(
            NibbleBuf {
                data: b"\x12\x34\x56\x78\x90".to_vec(),
                skip_last: true,
            },
            Nibbles {
                data: &bytes[..],
                skip_first: false,
                skip_last: true,
            }
            .to_nibble_buf()
        );
        assert_eq!(
            NibbleBuf {
                data: b"\x23\x45\x67\x89\xa0".to_vec(),
                skip_last: true,
            },
            Nibbles {
                data: &bytes[..],
                skip_first: true,
                skip_last: false,
            }
            .to_nibble_buf()
        );
        assert_eq!(
            NibbleBuf {
                data: b"\x23\x45\x67\x89".to_vec(),
                skip_last: false,
            },
            Nibbles {
                data: &bytes[..],
                skip_first: true,
                skip_last: true,
            }
            .to_nibble_buf()
        );
    }

    #[test]
    fn test_nibbles_eq() {
        let bytes1 = b"\x12\x34\x56";
        let bytes2 = b"\x01\x23\x45\x67";
        let nibbles = Nibbles {
            data: &bytes1[..],
            skip_first: false,
            skip_last: false,
        };
        assert_eq!(
            nibbles,
            Nibbles {
                data: &bytes2[..],
                skip_first: true,
                skip_last: true,
            }
        );
        assert_ne!(
            nibbles,
            Nibbles {
                data: &bytes2[..],
                skip_first: true,
                skip_last: false,
            }
        );
    }
}
