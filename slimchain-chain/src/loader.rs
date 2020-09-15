use crate::block::BlockTrait;
use slimchain_common::{
    basic::{BlockHeight, H256},
    error::Result,
    tx::TxTrait,
};
use std::sync::Arc;

pub trait TxLoaderTrait<Tx: TxTrait> {
    fn get_tx(&self, tx_hash: H256) -> Result<Tx>;
}

impl<Tx: TxTrait, Loader: TxLoaderTrait<Tx> + ?Sized> TxLoaderTrait<Tx> for Arc<Loader> {
    fn get_tx(&self, tx_hash: H256) -> Result<Tx> {
        self.as_ref().get_tx(tx_hash)
    }
}

pub trait BlockLoaderTrait<Block: BlockTrait> {
    fn get_non_genesis_block(&self, height: BlockHeight) -> Result<Block>;

    fn get_block(&self, height: BlockHeight) -> Result<Block> {
        if height.is_zero() {
            Ok(Block::genesis_block())
        } else {
            self.get_non_genesis_block(height)
        }
    }
}

impl<Block: BlockTrait, Loader: BlockLoaderTrait<Block> + ?Sized> BlockLoaderTrait<Block>
    for Arc<Loader>
{
    fn get_non_genesis_block(&self, height: BlockHeight) -> Result<Block> {
        self.as_ref().get_non_genesis_block(height)
    }
}
