use crate::{
    basic::{Address, BlockHeight, H256},
    digest::{blake2b_hash_to_h256, default_blake2, Digestible},
    ed25519::{verify_multi_signature, Keypair, PubSigPair},
    error::Result,
    rw_set::{TxReadSet, TxWriteData},
    tx_req::{tx_id_from_caller_and_input, TxRequest},
};
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

pub trait TxTrait: Digestible + Sized + Send + Sync {
    fn tx_caller(&self) -> Address;
    fn tx_input(&self) -> &TxRequest;
    fn tx_block_height(&self) -> BlockHeight;
    fn tx_state_root(&self) -> H256;
    fn tx_reads(&self) -> &TxReadSet;
    fn tx_writes(&self) -> &TxWriteData;

    fn id(&self) -> H256 {
        tx_id_from_caller_and_input(self.tx_caller(), self.tx_input())
    }

    fn verify_sig(&self) -> Result<()>;
}

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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct MultiSignedTx {
    pub raw_tx: RawTx,
    pub pk_sig_pairs: Vec<PubSigPair>,
}

impl Digestible for MultiSignedTx {
    fn to_digest(&self) -> H256 {
        let mut hash_state = default_blake2().to_state();
        hash_state.update(self.raw_tx.to_digest().as_bytes());
        for pk_sig in &self.pk_sig_pairs {
            hash_state.update(pk_sig.to_digest().as_bytes());
        }
        let hash = hash_state.finalize();
        blake2b_hash_to_h256(hash)
    }
}

impl TxTrait for MultiSignedTx {
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
        verify_multi_signature(hash, &self.pk_sig_pairs)
    }
}

impl From<SignedTx> for MultiSignedTx {
    fn from(input: SignedTx) -> Self {
        let mut pk_sig_pairs = Vec::new();
        pk_sig_pairs.push(input.pk_sig);
        Self {
            raw_tx: input.raw_tx,
            pk_sig_pairs,
        }
    }
}

impl MultiSignedTx {
    pub fn add_pk_sig(&mut self, pk_sig: PubSigPair) {
        self.pk_sig_pairs.push(pk_sig);
    }
}
