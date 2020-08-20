#![no_std]
#![allow(clippy::missing_safety_doc)]

#[macro_use]
extern crate sgx_tstd as std;
#[macro_use]
extern crate lazy_static;

use slimchain_common::ed25519::Keypair;

pub(crate) mod exec_tx;
pub(crate) mod quote_pk;
pub(crate) mod rand;

lazy_static! {
    pub(crate) static ref KEY_PAIR: Keypair = {
        let mut rng = rand::thread_rng();
        Keypair::generate(&mut rng)
    };
}
