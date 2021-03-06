use crate::{
    block::BlockTrait,
    db::{DBPtr, Transaction},
};
use serde::Serialize;
use slimchain_chain::latest::LatestTxCountPtr;
use slimchain_common::error::Result;
use slimchain_tx_state::TxStateUpdate;
use slimchain_utils::record_event;

#[tracing::instrument(level = "info", skip(db, new_block, update, latest_tx_count), fields(height = new_block.block_height().0), err)]
pub async fn commit_block<Block>(
    db: &DBPtr,
    new_block: &Block,
    update: &TxStateUpdate,
    latest_tx_count: &LatestTxCountPtr,
) -> Result<()>
where
    Block: BlockTrait + Serialize,
{
    let block_height = new_block.block_height();
    let txs = new_block.tx_list();
    let tx_len = txs.len();

    let mut db_tx = Transaction::with_capacity(1);
    db_tx.insert_meta_object("height", &block_height)?;
    db_tx.insert_block(new_block)?;
    db_tx.update_state(update)?;
    db.write_async(db_tx).await?;

    info!("Commit {} TX.", tx_len);
    latest_tx_count.add(tx_len);
    let tx_ids: Vec<_> = txs.iter().map(|tx| tx.id()).collect();
    record_event!("tx_commit", "tx_ids": tx_ids, "height": block_height.0);

    Ok(())
}
