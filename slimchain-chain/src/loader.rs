use crate::block::BlockTrait;
use slimchain_common::{
    basic::{BlockHeight, H256},
    error::Result,
    tx::TxTrait,
};

pub trait TxLoaderTrait<Tx: TxTrait> {
    fn get_tx(&self, tx_hash: H256) -> Result<Tx>;
}

pub trait BlockLoaderTrait<Block: BlockTrait> {
    fn latest_block_height(&self) -> BlockHeight;
    fn get_non_genesis_block(&self, height: BlockHeight) -> Result<Block>;

    fn get_block(&self, height: BlockHeight) -> Result<Block> {
        if height.is_zero() {
            Ok(Block::genesis_block())
        } else {
            self.get_non_genesis_block(height)
        }
    }
}
