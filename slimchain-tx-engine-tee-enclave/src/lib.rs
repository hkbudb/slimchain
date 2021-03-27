#![no_std]
#![allow(clippy::missing_safety_doc)]

#[macro_use]
extern crate sgx_tstd as std;

use once_cell::race::OnceBox;
use slimchain_common::ed25519::Keypair;
use std::boxed::Box;

pub(crate) mod exec_tx;
pub(crate) mod quote_pk;
pub(crate) mod rand;

pub(crate) fn get_key_pair() -> &'static Keypair {
    static KEY_PAIR: OnceBox<Keypair> = OnceBox::new();
    KEY_PAIR.get_or_init(|| {
        let mut rng = rand::os_rng();
        Box::new(Keypair::generate(&mut rng))
    })
}
