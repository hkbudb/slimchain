use crate::{
    basic::{Address, BlockHeight, H256},
    digest::Digestible,
    error::Result,
    rw_set::{TxReadSet, TxWriteData},
    tx_req::{tx_id_from_caller_and_input, TxRequest},
};

pub mod raw_tx;
pub use raw_tx::*;

pub mod signed_tx;
pub use signed_tx::*;

pub trait TxTrait: Digestible + Clone + Sized + Send + Sync {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basic::H160;
    use crate::ed25519::Keypair;

    #[test]
    fn test_signed_tx() {
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
        let keypair = Keypair::generate(&mut rng);
        let signed_tx = raw_tx.sign(&keypair);
        signed_tx.verify_sig().unwrap();
    }
}
