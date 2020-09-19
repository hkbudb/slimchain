use crate::{
    block::BlockTrait,
    block_proposal::BlockProposal,
    db::{DBPtr, Transaction},
    latest::LatestBlockHeaderPtr,
};
use serde::Serialize;
use slimchain_common::{error::Result, tx::TxTrait};
use slimchain_tx_state::TxStateUpdate;
use slimchain_utils::record_event;

fn record_txs<Tx, Block>(blk_proposal: &BlockProposal<Block, Tx>)
where
    Tx: TxTrait,
    Block: BlockTrait,
{
    let txs = blk_proposal.get_txs();
    info!("Commit {} TX.", txs.len());
    let tx_ids: Vec<_> = txs.iter().map(|tx| tx.id()).collect();
    record_event!("tx_commit", "txs": tx_ids, "height": blk_proposal.get_block().block_height().0);
}

#[allow(clippy::unit_arg)]
#[tracing::instrument(level = "info", skip(blk_proposal, db, latest_block_header), fields(height = blk_proposal.get_block().block_height().0), err)]
pub async fn commit_block<Tx, Block>(
    blk_proposal: &BlockProposal<Block, Tx>,
    db: &DBPtr,
    latest_block_header: &LatestBlockHeaderPtr,
) -> Result<()>
where
    Tx: TxTrait + Serialize,
    Block: BlockTrait + Serialize,
{
    let mut db_tx = Transaction::with_capacity(1);
    let blk = blk_proposal.get_block();
    db_tx.insert_block(blk)?;
    db.write_async(db_tx).await?;
    latest_block_header.set_from_block(blk);
    record_txs(blk_proposal);
    Ok(())
}

#[allow(clippy::unit_arg)]
#[tracing::instrument(level = "info", skip(blk_proposal, state_update, db, latest_block_header), fields(height = blk_proposal.get_block().block_height().0), err)]
pub async fn commit_block_storage_node<Tx, Block>(
    blk_proposal: &BlockProposal<Block, Tx>,
    state_update: &TxStateUpdate,
    db: &DBPtr,
    latest_block_header: &LatestBlockHeaderPtr,
) -> Result<()>
where
    Tx: TxTrait + Serialize,
    Block: BlockTrait + Serialize,
{
    let mut db_tx = Transaction::new();
    let blk = blk_proposal.get_block();
    let txs = blk_proposal.get_txs();

    db_tx.insert_block(blk)?;
    for (&tx_hash, tx) in blk.tx_list().iter().zip(txs.iter()) {
        debug_assert_eq!(tx_hash, tx.to_digest());
        db_tx.insert_tx(tx_hash, tx)?;
    }
    db_tx.update_state(state_update)?;

    db.write_async(db_tx).await?;
    latest_block_header.set_from_block(blk);
    record_txs(blk_proposal);
    Ok(())
}
