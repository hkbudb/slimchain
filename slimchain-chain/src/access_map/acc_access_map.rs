use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{Address, StateKey},
    rw_set::{ReadAccessFlags, WriteAccessFlags},
    utils::derive_more::{Deref, DerefMut},
};

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccountReadAccess {
    flags: ReadAccessFlags,
    values: im::HashSet<StateKey>,
}

impl AccountReadAccess {
    pub fn get_nonce(&self) -> bool {
        self.flags.contains(ReadAccessFlags::NONCE)
    }

    pub fn get_code(&self) -> bool {
        self.flags.contains(ReadAccessFlags::CODE)
    }

    pub fn value_iter(&self) -> impl Iterator<Item = &'_ StateKey> {
        self.values.iter()
    }

    pub fn set_nonce(&mut self, flag: bool) {
        self.flags.set_nonce(flag);
    }

    pub fn set_code(&mut self, flag: bool) {
        self.flags.set_code(flag);
    }

    pub fn add_value(&mut self, key: StateKey) {
        self.values.insert(key);
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccountWriteAccess {
    flags: WriteAccessFlags,
    values: im::HashSet<StateKey>,
}

impl AccountWriteAccess {
    pub fn get_nonce(&self) -> bool {
        self.flags.contains(WriteAccessFlags::NONCE)
    }

    pub fn get_code(&self) -> bool {
        self.flags.contains(WriteAccessFlags::CODE)
    }

    pub fn get_reset_values(&self) -> bool {
        self.flags.contains(WriteAccessFlags::RESET_VALUES)
    }

    pub fn value_iter(&self) -> impl Iterator<Item = &'_ StateKey> {
        self.values.iter()
    }

    pub fn set_nonce(&mut self, flag: bool) {
        self.flags.set_nonce(flag);
    }

    pub fn set_code(&mut self, flag: bool) {
        self.flags.set_code(flag);
    }

    pub fn set_reset_values(&mut self, flag: bool) {
        self.flags.set_reset_values(flag);
    }

    pub fn add_value(&mut self, key: StateKey) {
        self.values.insert(key);
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize, Deref, DerefMut)]
pub struct ReadAccessItem(pub im::HashMap<Address, AccountReadAccess>);

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize, Deref, DerefMut)]
pub struct WriteAccessItem(pub im::HashMap<Address, AccountWriteAccess>);
