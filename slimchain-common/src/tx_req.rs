use crate::{
    basic::{Address, Code, Nonce, H256},
    digest::{blake2, blake2b_hash_to_h160, blake2b_hash_to_h256, default_blake2, Digestible},
    ed25519::{Keypair, PubSigPair, PublicKey},
    error::Result,
};
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

crate::create_id_type_u64!(TxReqId);

pub fn caller_address_from_pk(pk: &PublicKey) -> Address {
    let hash = blake2(20).hash(&pk.to_bytes()[..]);
    blake2b_hash_to_h160(hash).into()
}

pub(crate) fn tx_id_from_caller_and_input(caller: Address, input: &TxRequest) -> H256 {
    let mut hash_state = default_blake2().to_state();
    hash_state.update(caller.to_digest().as_bytes());
    hash_state.update(input.to_digest().as_bytes());
    blake2b_hash_to_h256(hash_state.finalize())
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum TxRequest {
    Create {
        nonce: Nonce,
        code: Code,
    },
    Call {
        nonce: Nonce,
        address: Address,
        data: Vec<u8>,
    },
}

impl Digestible for TxRequest {
    fn to_digest(&self) -> H256 {
        let mut hash_state = default_blake2().to_state();
        let hash = match self {
            TxRequest::Create { nonce, code } => {
                hash_state.update(b"Create");
                hash_state.update(nonce.to_digest().as_bytes());
                hash_state.update(code.to_digest().as_bytes());
                hash_state.finalize()
            }
            TxRequest::Call {
                nonce,
                address,
                data,
            } => {
                hash_state.update(b"Call");
                hash_state.update(nonce.to_digest().as_bytes());
                hash_state.update(address.to_digest().as_bytes());
                hash_state.update(&data[..]);
                hash_state.finalize()
            }
        };
        blake2b_hash_to_h256(hash)
    }
}

impl TxRequest {
    pub fn nonce(&self) -> Nonce {
        match self {
            TxRequest::Call { nonce, .. } | TxRequest::Create { nonce, .. } => *nonce,
        }
    }

    pub fn sign(self, keypair: &Keypair) -> SignedTxRequest {
        let hash = self.to_digest();
        SignedTxRequest {
            input: self,
            pk_sig: PubSigPair::create(keypair, hash),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignedTxRequest {
    pub input: TxRequest,
    pub pk_sig: PubSigPair,
}

impl Digestible for SignedTxRequest {
    fn to_digest(&self) -> H256 {
        let mut hash_state = default_blake2().to_state();
        hash_state.update(self.input.to_digest().as_bytes());
        hash_state.update(self.pk_sig.to_digest().as_bytes());
        let hash = hash_state.finalize();
        blake2b_hash_to_h256(hash)
    }
}

impl SignedTxRequest {
    pub fn verify(&self) -> Result<()> {
        let hash = self.input.to_digest();
        self.pk_sig.verify(hash)
    }

    pub fn caller_address(&self) -> Address {
        caller_address_from_pk(&self.pk_sig.public())
    }

    pub fn id(&self) -> H256 {
        tx_id_from_caller_and_input(self.caller_address(), &self.input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basic::H160;

    #[test]
    fn test_sign_verify_tx_req() {
        let tx_req = TxRequest::Call {
            nonce: 1.into(),
            address: H160::repeat_byte(0xf).into(),
            data: b"data".to_vec(),
        };

        let mut rng = rand::thread_rng();
        let keypair = Keypair::generate(&mut rng);
        let signed_tx_req = tx_req.sign(&keypair);
        assert!(signed_tx_req.verify().is_ok());
    }

    #[test]
    fn test_tx_req_serde() {
        let tx_req = TxRequest::Call {
            nonce: 1.into(),
            address: H160::repeat_byte(0xf).into(),
            data: b"data".to_vec(),
        };

        let mut rng = rand::thread_rng();
        let keypair = Keypair::generate(&mut rng);
        let signed_tx_req = tx_req.sign(&keypair);

        let bin = postcard::to_allocvec(&signed_tx_req).unwrap();
        assert_eq!(
            postcard::from_bytes::<SignedTxRequest>(&bin[..]).unwrap(),
            signed_tx_req
        );
    }
}
