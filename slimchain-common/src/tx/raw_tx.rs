use super::{SignedTx, TxTrait};
use crate::{
    basic::{Address, BlockHeight, H256},
    digest::{blake2b_hash_to_h256, default_blake2, Digestible},
    ed25519::{Keypair, PubSigPair},
    error::Result,
    rw_set::{TxReadSet, TxWriteData},
    tx_req::TxRequest,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RawTx {
    pub caller: Address,
    pub input: TxRequest,
    pub block_height: BlockHeight,
    pub state_root: H256,
    pub reads: TxReadSet,
    pub writes: TxWriteData,
}

impl Digestible for RawTx {
    fn to_digest(&self) -> H256 {
        let mut hash_state = default_blake2().to_state();
        hash_state.update(self.caller.as_bytes());
        hash_state.update(self.input.to_digest().as_bytes());
        hash_state.update(self.block_height.to_digest().as_bytes());
        hash_state.update(self.state_root.as_bytes());
        hash_state.update(self.reads.to_digest().as_bytes());
        hash_state.update(self.writes.to_digest().as_bytes());
        let hash = hash_state.finalize();
        blake2b_hash_to_h256(hash)
    }
}

impl TxTrait for RawTx {
    fn tx_caller(&self) -> Address {
        self.caller
    }

    fn tx_input(&self) -> &TxRequest {
        &self.input
    }

    fn tx_block_height(&self) -> BlockHeight {
        self.block_height
    }

    fn tx_state_root(&self) -> H256 {
        self.state_root
    }

    fn tx_reads(&self) -> &TxReadSet {
        &self.reads
    }

    fn tx_writes(&self) -> &TxWriteData {
        &self.writes
    }

    fn verify_sig(&self) -> Result<()> {
        Ok(())
    }
}

impl RawTx {
    pub fn sign(self, keypair: &Keypair) -> SignedTx {
        let hash = self.to_digest();
        SignedTx {
            raw_tx: self,
            pk_sig: PubSigPair::create(keypair, hash),
        }
    }
}
