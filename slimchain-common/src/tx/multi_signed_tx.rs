use super::{RawTx, SignedTx, TxTrait};
use crate::{
    basic::{Address, BlockHeight, H256},
    collections::HashSet,
    digest::{blake2b_hash_to_h256, default_blake2, Digestible},
    ed25519::{ed25519_dalek::PUBLIC_KEY_LENGTH, verify_multi_signature, PubSigPair, PublicKey},
    error::{anyhow, bail, ensure, Result},
    rw_set::{TxReadSet, TxWriteData},
    tx_req::TxRequest,
};
use alloc::vec::Vec;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};

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
        if let Some(known_pks) = KNOWN_PKS.get() {
            let mut pks = HashSet::new();

            for pk_sig in &self.pk_sig_pairs {
                ensure!(known_pks.has_key(pk_sig.public()), "Unknown public key.");
                pks.insert(pk_sig.public().to_bytes());
            }
            ensure!(
                pks.len() >= known_pks.quorum,
                "The number of signatures is less than the quorum."
            );
        } else {
            bail!("Known public keys are not set.");
        }

        let hash = self.raw_tx.to_digest();
        verify_multi_signature(hash, &self.pk_sig_pairs)?;

        Ok(())
    }
}

impl From<SignedTx> for MultiSignedTx {
    fn from(input: SignedTx) -> Self {
        Self {
            raw_tx: input.raw_tx,
            pk_sig_pairs: alloc::vec![input.pk_sig],
        }
    }
}

impl MultiSignedTx {
    pub fn add_pk_sig(&mut self, pk_sig: PubSigPair) {
        self.pk_sig_pairs.push(pk_sig);
    }

    pub fn set_known_pks(pks: &[PublicKey], quorum: usize) -> Result<()> {
        KNOWN_PKS
            .set(KnownPks::new(pks, quorum))
            .map_err(|_e| anyhow!("Known public keys already init."))
    }
}

struct KnownPks {
    known_pks: HashSet<[u8; PUBLIC_KEY_LENGTH]>,
    quorum: usize,
}

impl KnownPks {
    fn new(pks: &[PublicKey], quorum: usize) -> Self {
        let mut known_pks = HashSet::new();
        for pk in pks {
            known_pks.insert(pk.to_bytes());
        }
        Self { known_pks, quorum }
    }

    fn has_key(&self, key: &PublicKey) -> bool {
        self.known_pks.contains(key.as_bytes())
    }
}

static KNOWN_PKS: OnceCell<KnownPks> = OnceCell::new();

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basic::H160;
    use crate::ed25519::Keypair;

    #[test]
    fn test_multi_signed_tx() {
        let tx_req = TxRequest::Call {
            nonce: 1.into(),
            address: H160::repeat_byte(0xf).into(),
            data: b"data".to_vec(),
        };

        let raw_tx = RawTx {
            caller: Address::default(),
            input: tx_req,
            block_height: 1.into(),
            state_root: H256::zero(),
            reads: TxReadSet::default(),
            writes: TxWriteData::default(),
        };

        let mut rng = rand::thread_rng();
        let keypair1 = Keypair::generate(&mut rng);
        let keypair2 = Keypair::generate(&mut rng);
        MultiSignedTx::set_known_pks(&[keypair1.public, keypair2.public], 2).unwrap();

        let signed_tx1 = raw_tx.clone().sign(&keypair1);
        let signed_tx2 = raw_tx.sign(&keypair2);

        let mut multi_signed_tx = MultiSignedTx::from(signed_tx1);
        assert!(multi_signed_tx.verify_sig().is_err());
        multi_signed_tx.add_pk_sig(signed_tx2.pk_sig);
        assert!(multi_signed_tx.verify_sig().is_ok());

        // clean up
        unsafe {
            let known_pks_ptr = core::mem::transmute::<
                *const OnceCell<KnownPks>,
                *mut OnceCell<KnownPks>,
            >(&KNOWN_PKS);
            (*known_pks_ptr).take();
        }
    }
}
