use super::exec_tx;
use crate::{
    block::{BlockHeader, BlockLoaderTrait, BlockTrait, BlockTxList},
    db::DBPtr,
};
use chrono::Utc;
use futures::prelude::*;
use serde::Deserialize;
use slimchain_chain::config::MinerConfig;
use slimchain_common::{
    basic::BlockHeight,
    error::{Context as _, Result},
    tx_req::SignedTxRequest,
};
use slimchain_tx_state::TxStateUpdate;
use slimchain_utils::record_event;
use std::time::Instant;
use tokio::time::timeout_at;

#[tracing::instrument(level = "info", skip(miner_cfg, db, last_block_height, tx_reqs, new_block_fn), fields(height = last_block_height.0 + 1), err)]
pub async fn propose_block<Block, TxStream, NewBlockFn>(
    miner_cfg: &MinerConfig,
    db: &DBPtr,
    last_block_height: BlockHeight,
    tx_reqs: &mut TxStream,
    new_block_fn: NewBlockFn,
) -> Result<Option<(Block, TxStateUpdate)>>
where
    Block: BlockTrait + for<'de> Deserialize<'de> + 'static,
    TxStream: Stream<Item = SignedTxRequest> + Unpin,
    NewBlockFn: Fn(BlockHeader, &Block) -> Block + Send + 'static,
{
    let begin = Instant::now();
    let deadline = begin + miner_cfg.max_block_interval;

    let mut txs: Vec<SignedTxRequest> = Vec::with_capacity(miner_cfg.max_txs);

    let next_block_height = last_block_height.next_height();

    let last_block: Block = db
        .get_block(last_block_height)
        .context("Failed to get the last block.")?;
    let mut update = TxStateUpdate::default();
    update.root = last_block.state_root();

    while txs.len() < miner_cfg.max_txs {
        let tx_req = if txs.len() < miner_cfg.min_txs {
            tx_reqs.next().await
        } else {
            if Instant::now() > deadline {
                break;
            }

            match timeout_at(deadline.into(), tx_reqs.next()).await {
                Ok(req) => req,
                Err(_) => {
                    debug!("Wait tx proposal timeout.");
                    break;
                }
            }
        };

        let tx_req = match tx_req {
            Some(req) => req,
            None => {
                debug!("No tx req is available.");
                return Ok(None);
            }
        };

        record_event!("blk_recv_tx", "tx_id": tx_req.id(), "height": next_block_height.0);

        match exec_tx(db, &update, &tx_req).await {
            Ok(new_update) => {
                update = new_update;
            }
            Err(e) => {
                error!("Error during execution. Error: {}", e);
                continue;
            }
        }

        txs.push(tx_req);
    }

    let new_state_root = update.root;
    let block_header = BlockHeader::new(
        next_block_height,
        last_block.to_digest(),
        Utc::now(),
        BlockTxList(txs),
        new_state_root,
    );
    let new_blk = tokio::task::block_in_place(move || new_block_fn(block_header, &last_block));

    let end = Instant::now();
    record_event!("propose_end", "height": new_blk.block_height().0);
    info!(time = ?(end - begin));
    Ok(Some((new_blk, update)))
}
