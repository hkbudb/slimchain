use serde::{Deserialize, Serialize};
use slimchain_chain::{block::BlockTrait, loader::TxLoaderTrait};
use slimchain_common::{
    basic::BlockHeight,
    error::{ensure, Result},
    tx::TxTrait,
};

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct BlockProposal<Block: BlockTrait, Tx: TxTrait> {
    block: Block,
    txs: Vec<Tx>,
}

impl<Block: BlockTrait, Tx: TxTrait> BlockProposal<Block, Tx> {
    pub fn new(block: Block, txs: Vec<Tx>) -> Self {
        Self { block, txs }
    }

    pub fn from_existing_block(block: Block, tx_loader: &impl TxLoaderTrait<Tx>) -> Result<Self> {
        let height = block.block_height();
        ensure!(
            !height.is_zero(),
            "Cannot create the block proposal from the genesis block."
        );

        let txs = block.tx_list().to_txs(tx_loader)?;
        Ok(Self::new(block, txs))
    }

    pub fn get_block_height(&self) -> BlockHeight {
        self.block.block_height()
    }

    pub fn get_block(&self) -> &Block {
        &self.block
    }

    pub fn get_block_mut(&mut self) -> &mut Block {
        &mut self.block
    }

    pub fn get_txs(&self) -> &[Tx] {
        &self.txs
    }

    pub fn unpack(self) -> (Block, Vec<Tx>) {
        (self.block, self.txs)
    }
}
