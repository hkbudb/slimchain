use super::exec_tx;
use crate::{
    block::{BlockLoaderTrait, BlockTrait},
    db::DBPtr,
};
use serde::Deserialize;
use slimchain_common::{
    basic::BlockHeight,
    error::{ensure, Context as _, Result},
};
use slimchain_tx_state::TxStateUpdate;
use slimchain_utils::record_time;
use std::time::Instant;

#[tracing::instrument(level = "info", skip(db, last_block_height, new_block, verify_consensus_fn), fields(height = last_block_height.0 + 1), err)]
pub async fn verify_block<Block, VerifyConsensusFn>(
    db: &DBPtr,
    last_block_height: BlockHeight,
    new_block: &Block,
    verify_consensus_fn: VerifyConsensusFn,
) -> Result<TxStateUpdate>
where
    Block: BlockTrait + for<'de> Deserialize<'de>,
    VerifyConsensusFn: Fn(&Block, &Block) -> Result<()>,
{
    let begin = Instant::now();

    let last_block: Block = db
        .get_block(last_block_height)
        .context("Failed to get the last block.")?;

    new_block.verify_block_header(&last_block)?;
    verify_consensus_fn(new_block, &last_block)?;

    let mut update = TxStateUpdate::default();
    update.root = last_block.state_root();

    for tx_req in new_block.tx_list().iter() {
        let new_update = exec_tx(db, &update, tx_req).await?;
        update = new_update;
    }

    ensure!(
        new_block.state_root() == update.root,
        "Invalid state root in the block proposal (expect: {}, actual: {}).",
        new_block.state_root(),
        update.root,
    );

    let time = Instant::now() - begin;
    record_time!("verify_block", time, "height": new_block.block_height().0);
    info!(?time);
    Ok(update)
}
