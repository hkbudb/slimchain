use super::{RawTx, TxTrait};
use crate::{
    basic::{Address, BlockHeight, H256},
    digest::{blake2b_hash_to_h256, default_blake2, Digestible},
    ed25519::PubSigPair,
    error::Result,
    rw_set::{TxReadSet, TxWriteData},
    tx_req::TxRequest,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignedTx {
    pub raw_tx: RawTx,
    pub pk_sig: PubSigPair,
}

impl Digestible for SignedTx {
    fn to_digest(&self) -> H256 {
        let mut hash_state = default_blake2().to_state();
        hash_state.update(self.raw_tx.to_digest().as_bytes());
        hash_state.update(self.pk_sig.to_digest().as_bytes());
        let hash = hash_state.finalize();
        blake2b_hash_to_h256(hash)
    }
}

impl TxTrait for SignedTx {
    fn tx_caller(&self) -> Address {
        self.raw_tx.tx_caller()
    }

    fn tx_input(&self) -> &TxRequest {
        self.raw_tx.tx_input()
    }

    fn tx_block_height(&self) -> BlockHeight {
        self.raw_tx.tx_block_height()
    }

    fn tx_state_root(&self) -> H256 {
        self.raw_tx.tx_state_root()
    }

    fn tx_reads(&self) -> &TxReadSet {
        self.raw_tx.tx_reads()
    }

    fn tx_writes(&self) -> &TxWriteData {
        self.raw_tx.tx_writes()
    }

    fn verify_sig(&self) -> Result<()> {
        let hash = self.raw_tx.to_digest();
        self.pk_sig.verify(hash)
    }
}
