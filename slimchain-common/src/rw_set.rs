use crate::{
    basic::{Address, Code, Nonce, StateKey, StateValue, H256},
    collections::{hash_map::Entry, HashMap, HashSet},
    digest::{blake2b_hash_to_h256, default_blake2, Digestible},
};
use alloc::vec::Vec;
use bitflags::bitflags;
use serde::{Deserialize, Serialize};

bitflags! {
    #[derive(Default, Serialize, Deserialize)]
    pub struct AccessFlags: u8 {
        const NONCE = 0b001;
        const CODE  = 0b010;
        const STATE = 0b100;
    }
}

impl AccessFlags {
    pub fn get_nonce(self) -> bool {
        self.contains(Self::NONCE)
    }

    pub fn get_code(self) -> bool {
        self.contains(Self::CODE)
    }

    pub fn get_state(self) -> bool {
        self.contains(Self::STATE)
    }

    pub fn set_nonce(&mut self, value: bool) {
        self.set(Self::NONCE, value);
    }

    pub fn set_code(&mut self, value: bool) {
        self.set(Self::CODE, value);
    }

    pub fn set_state(&mut self, value: bool) {
        self.set(Self::STATE, value);
    }
}

#[derive(
    Debug,
    Default,
    Clone,
    Eq,
    PartialEq,
    Serialize,
    Deserialize,
    derive_more::Deref,
    derive_more::DerefMut,
)]
pub struct TxReadSet(pub HashMap<Address, AccountReadSet>);

impl Digestible for TxReadSet {
    fn to_digest(&self) -> H256 {
        let mut hash_state = default_blake2().to_state();
        let mut sorted: Vec<_> = self.0.iter().collect();
        sorted.sort_unstable_by_key(|input| input.0);
        for (k, v) in &sorted {
            hash_state.update(k.as_bytes());
            hash_state.update(v.to_digest().as_bytes());
        }
        let hash = hash_state.finalize();
        blake2b_hash_to_h256(hash)
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccountReadSet {
    pub access_flags: AccessFlags,
    pub values: HashSet<StateKey>,
}

impl Digestible for AccountReadSet {
    fn to_digest(&self) -> H256 {
        let mut hash_state = default_blake2().to_state();
        if self.get_nonce() {
            hash_state.update(b"\x01");
        } else {
            hash_state.update(b"\x00");
        }
        if self.get_code() {
            hash_state.update(b"\x01");
        } else {
            hash_state.update(b"\x00");
        }
        let mut values_sorted: Vec<_> = self.values.iter().collect();
        values_sorted.sort_unstable();
        for v in &values_sorted {
            hash_state.update(v.as_bytes());
        }
        let hash = hash_state.finalize();
        blake2b_hash_to_h256(hash)
    }
}

impl AccountReadSet {
    pub fn get_nonce(&self) -> bool {
        self.access_flags.get_nonce()
    }

    pub fn set_nonce(&mut self, value: bool) {
        self.access_flags.set_nonce(value);
    }

    pub fn get_code(&self) -> bool {
        self.access_flags.get_code()
    }

    pub fn set_code(&mut self, value: bool) {
        self.access_flags.set_code(value);
    }

    pub fn get_values(&self) -> &HashSet<StateKey> {
        &self.values
    }

    pub fn is_empty(&self) -> bool {
        self.access_flags.is_empty() && self.values.is_empty()
    }
}

#[derive(
    Debug,
    Default,
    Clone,
    Eq,
    PartialEq,
    Serialize,
    Deserialize,
    derive_more::Deref,
    derive_more::DerefMut,
)]
pub struct TxReadData(pub HashMap<Address, AccountReadData>);

impl TxReadData {
    pub fn to_set(&self) -> TxReadSet {
        let mut out = TxReadSet::default();
        for (k, v) in &self.0 {
            out.0.insert(*k, v.to_set());
        }
        out
    }

    pub fn get_nonce(&self, address: Address) -> Option<Nonce> {
        self.0.get(&address).and_then(|acc| acc.nonce)
    }

    pub fn add_nonce(&mut self, address: Address, nonce: Nonce) {
        self.0.entry(address).or_default().nonce = Some(nonce);
    }

    pub fn remove_nonce(&mut self, address: Address) {
        match self.0.entry(address) {
            Entry::Occupied(mut e) => {
                let acc_data = e.get_mut();
                acc_data.nonce = None;
                if acc_data.is_empty() {
                    e.remove();
                }
            }
            Entry::Vacant(_) => {}
        }
    }

    pub fn add_code(&mut self, address: Address, code: Code) {
        self.0.entry(address).or_default().code = Some(code);
    }

    pub fn add_value(&mut self, address: Address, key: StateKey, value: StateValue) {
        *self
            .0
            .entry(address)
            .or_default()
            .values
            .entry(key)
            .or_default() = value;
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccountReadData {
    pub nonce: Option<Nonce>,
    pub code: Option<Code>,
    pub values: HashMap<StateKey, StateValue>,
}

impl AccountReadData {
    pub fn to_set(&self) -> AccountReadSet {
        let mut access_flags = AccessFlags::empty();
        access_flags.set_nonce(self.nonce.is_some());
        access_flags.set_code(self.code.is_some());
        AccountReadSet {
            access_flags,
            values: self.values.keys().copied().collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.nonce.is_none() && self.code.is_none() && self.values.is_empty()
    }
}

#[derive(
    Debug,
    Default,
    Clone,
    Eq,
    PartialEq,
    Serialize,
    Deserialize,
    derive_more::Deref,
    derive_more::DerefMut,
)]
pub struct TxWriteData(pub HashMap<Address, AccountWriteData>);

impl Digestible for TxWriteData {
    fn to_digest(&self) -> H256 {
        let mut hash_state = default_blake2().to_state();
        let mut sorted: Vec<_> = self.0.iter().collect();
        sorted.sort_unstable_by_key(|input| input.0);
        for (k, v) in &sorted {
            hash_state.update(k.as_bytes());
            hash_state.update(v.to_digest().as_bytes());
        }
        let hash = hash_state.finalize();
        blake2b_hash_to_h256(hash)
    }
}

impl TxWriteData {
    pub fn delete_account(&mut self, address: Address) {
        self.0.insert(
            address,
            AccountWriteData {
                nonce: Some(Nonce::default()),
                code: Some(Code::new()),
                values: HashMap::new(),
                reset_values: true,
            },
        );
    }

    pub fn add_nonce(&mut self, address: Address, nonce: Nonce) {
        self.0.entry(address).or_default().nonce = Some(nonce);
    }

    pub fn add_code(&mut self, address: Address, code: Code) {
        self.0.entry(address).or_default().code = Some(code);
    }

    pub fn add_reset_values(&mut self, address: Address) {
        self.0.entry(address).or_default().reset_values = true;
    }

    pub fn add_value(&mut self, address: Address, key: StateKey, value: StateValue) {
        *self
            .0
            .entry(address)
            .or_default()
            .values
            .entry(key)
            .or_default() = value;
    }

    pub fn merge(&mut self, new: TxWriteData) {
        for (k, v) in new.0.into_iter() {
            match self.entry(k) {
                Entry::Occupied(mut e) => {
                    e.get_mut().merge(v);
                }
                Entry::Vacant(e) => {
                    e.insert(v);
                }
            }
        }
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccountWriteData {
    pub nonce: Option<Nonce>,
    pub code: Option<Code>,
    pub values: HashMap<StateKey, StateValue>,
    pub reset_values: bool,
}

impl Digestible for AccountWriteData {
    fn to_digest(&self) -> H256 {
        let mut hash_state = default_blake2().to_state();
        match &self.nonce {
            Some(n) => {
                hash_state.update(b"\x01");
                hash_state.update(n.to_digest().as_bytes());
            }
            None => {
                hash_state.update(b"\x00");
            }
        }
        match &self.code {
            Some(code) => {
                hash_state.update(b"\x01");
                hash_state.update(code.to_digest().as_bytes());
            }
            None => {
                hash_state.update(b"\x00");
            }
        }
        if self.reset_values {
            hash_state.update(b"\x01");
        } else {
            hash_state.update(b"\x00");
        }
        let mut sorted: Vec<_> = self.values.iter().collect();
        sorted.sort_unstable_by_key(|input| input.0);
        for (k, v) in &sorted {
            hash_state.update(k.as_bytes());
            hash_state.update(v.as_bytes());
        }
        let hash = hash_state.finalize();
        blake2b_hash_to_h256(hash)
    }
}

impl AccountWriteData {
    pub fn merge(&mut self, new: AccountWriteData) {
        if new.nonce.is_some() {
            self.nonce = new.nonce;
        }

        if new.code.is_some() {
            self.code = new.code;
        }

        if new.reset_values {
            self.reset_values = true;
            self.values = new.values;
        } else {
            self.values.extend(new.values.into_iter());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_data() {
        let mut read = TxReadData::default();
        read.add_code(
            crate::create_address!("0000000000000000000000000000000000000000"),
            b"code".to_vec().into(),
        );
        read.add_nonce(
            crate::create_address!("0000000000000000000000000000000000000000"),
            1.into(),
        );
        read.add_nonce(
            crate::create_address!("0000000000000000000000000000000000000001"),
            1.into(),
        );
        read.remove_nonce(crate::create_address!(
            "0000000000000000000000000000000000000000"
        ));
        read.add_value(
            crate::create_address!("0000000000000000000000000000000000000000"),
            crate::create_state_key!(
                "0000000000000000000000000000000000000000000000000000000000000000"
            ),
            1.into(),
        );
        let expect = crate::create_tx_read_data! {
            "0000000000000000000000000000000000000000" => {
                code: b"code",
                values: {
                    "0000000000000000000000000000000000000000000000000000000000000000" => 1,
                }
            },
            "0000000000000000000000000000000000000001" => {
                nonce: 1,
            },
        };
        assert_eq!(read, expect);
    }

    #[test]
    fn test_write_data() {
        let mut write = TxWriteData::default();
        write.delete_account(crate::create_address!(
            "0000000000000000000000000000000000000000"
        ));
        write.add_code(
            crate::create_address!("0000000000000000000000000000000000000001"),
            b"code".to_vec().into(),
        );
        write.add_nonce(
            crate::create_address!("0000000000000000000000000000000000000001"),
            1.into(),
        );
        write.add_reset_values(crate::create_address!(
            "0000000000000000000000000000000000000002"
        ));
        write.add_value(
            crate::create_address!("0000000000000000000000000000000000000002"),
            crate::create_state_key!(
                "0000000000000000000000000000000000000000000000000000000000000000"
            ),
            1.into(),
        );
        let expect = crate::create_tx_write_set! {
            "0000000000000000000000000000000000000000" => {
                nonce: 0,
                code: b"",
                reset_values: true,
            },
            "0000000000000000000000000000000000000001" => {
                nonce: 1,
                code: b"code",
            },
            "0000000000000000000000000000000000000002" => {
                reset_values: true,
                values: {
                    "0000000000000000000000000000000000000000000000000000000000000000" => 1,
                }
            },
        };
        assert_eq!(write, expect);
    }

    #[test]
    fn test_write_merge() {
        let mut write1 = crate::create_tx_write_set! {
            "0000000000000000000000000000000000000000" => {
                nonce: 1,
            },
            "0000000000000000000000000000000000000001" => {
                reset_values: true,
                code: b"code",
                values: {
                    "0000000000000000000000000000000000000000000000000000000000000000" => 1,
                    "0000000000000000000000000000000000000000000000000000000000000001" => 2,
                }
            },
        };
        let write2 = crate::create_tx_write_set! {
            "0000000000000000000000000000000000000001" => {
                values: {
                    "0000000000000000000000000000000000000000000000000000000000000002" => 3,
                }
            },
            "0000000000000000000000000000000000000002" => {
                nonce: 1,
                values: {
                    "0000000000000000000000000000000000000000000000000000000000000000" => 1,
                }
            },
        };
        let write3 = crate::create_tx_write_set! {
            "0000000000000000000000000000000000000000" => {
                nonce: 1,
            },
            "0000000000000000000000000000000000000001" => {
                reset_values: true,
                code: b"code",
                values: {
                    "0000000000000000000000000000000000000000000000000000000000000000" => 1,
                    "0000000000000000000000000000000000000000000000000000000000000001" => 2,
                    "0000000000000000000000000000000000000000000000000000000000000002" => 3,
                }
            },
            "0000000000000000000000000000000000000002" => {
                nonce: 1,
                values: {
                    "0000000000000000000000000000000000000000000000000000000000000000" => 1,
                }
            },
        };
        write1.merge(write2);
        assert_eq!(write1, write3);
    }
}
