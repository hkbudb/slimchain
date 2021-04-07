use crate::{block_proposal::BlockProposal, snapshot::Snapshot};
use chrono::Utc;
use futures::prelude::*;
use slimchain_chain::{
    block::{BlockHeader, BlockTrait, BlockTxList},
    config::{ChainConfig, MinerConfig},
    db::DBPtr,
};
use slimchain_common::{
    error::{Context as _, Result},
    rw_set::TxWriteData,
    tx::TxTrait,
};
use slimchain_tx_state::{update_tx_state, TxStateUpdate};
use slimchain_utils::record_event;
use std::time::Instant;
use tokio::time::timeout_at;

#[tracing::instrument(level = "info", skip(chain_cfg, miner_cfg, db, snapshot, tx_proposals, new_block_fn), fields(height = snapshot.current_height().0 + 1), err)]
pub async fn propose_block<Tx, Block, TxStream, NewBlockFn, NewBlockFnOutput>(
    chain_cfg: &ChainConfig,
    miner_cfg: &MinerConfig,
    db: &DBPtr,
    snapshot: &mut Snapshot<Block>,
    tx_proposals: &mut TxStream,
    new_block_fn: NewBlockFn,
) -> Result<Option<(BlockProposal<Block, Tx>, TxStateUpdate)>>
where
    Tx: TxTrait,
    Block: BlockTrait + 'static,
    TxStream: Stream<Item = Tx> + Unpin,
    NewBlockFn: Fn(BlockHeader, &Block) -> NewBlockFnOutput,
    NewBlockFnOutput: Future<Output = Result<Block>> + Send + 'static,
{
    let begin = Instant::now();
    let deadline = begin + miner_cfg.max_block_interval;

    let mut txs: Vec<Tx> = Vec::with_capacity(miner_cfg.max_txs);

    let last_block = snapshot
        .get_latest_block()
        .context("Failed to get last block")?;
    let prev_state_root = last_block.state_root();

    let last_block_height = snapshot.current_height();
    let next_block_height = last_block_height.next_height();

    debug_assert_eq!(last_block.block_height(), last_block_height);

    snapshot.access_map.alloc_new_block();
    let mut writes = TxWriteData::default();

    while txs.len() < miner_cfg.max_txs {
        let tx_proposal = if txs.len() < miner_cfg.min_txs {
            tx_proposals.next().await
        } else {
            if Instant::now() > deadline {
                break;
            }

            match timeout_at(deadline.into(), tx_proposals.next()).await {
                Ok(tx_proposal) => tx_proposal,
                Err(_) => {
                    debug!("Wait tx proposal timeout.");
                    break;
                }
            }
        };

        let tx = match tx_proposal {
            Some(tx) => tx,
            None => {
                debug!("No tx proposal is available.");
                return Ok(None);
            }
        };

        let tx_id = tx.id();
        record_event!("blk_recv_tx", "tx_id": tx_id, "height": next_block_height.0);

        let tx_block_height = tx.tx_block_height();
        if tx_block_height < snapshot.access_map.oldest_block_height() {
            debug!("Tx proposal is outdated.");
            record_event!("discard_tx", "tx_id": tx_id, "reason": "tx_outdated");
            continue;
        }
        if tx_block_height > last_block_height {
            warn!("Tx proposal is too new.");
            record_event!("discard_tx", "tx_id": tx_id, "reason": "tx_too_new");
            continue;
        }

        if chain_cfg.conflict_check.has_conflict(
            &snapshot.access_map,
            tx_block_height,
            tx.tx_reads(),
            tx.tx_writes(),
        ) {
            debug!("Received a tx with conflict");
            record_event!("discard_tx", "tx_id": tx_id, "reason": "tx_conflict");
            continue;
        }

        let tx_block = snapshot
            .get_block(tx_block_height)
            .context("Failed to get the block for tx")?;

        if tx.tx_state_root() != tx_block.state_root() {
            warn!("Received a tx with invalid state root.");
            record_event!("discard_tx", "tx_id": tx_id, "reason": "invalid_state_root");
            continue;
        }

        if let Err(e) = tx.verify_sig() {
            warn!("Received a tx with invalid sig. Error: {:?}", e);
            record_event!("discard_tx", "tx_id": tx_id, "reason": "invalid_sig", "detail": std::format!("{}", e));
            continue;
        }

        snapshot.access_map.add_read(tx.tx_reads());
        snapshot.access_map.add_write(tx.tx_writes());
        writes.merge(tx.tx_writes());

        txs.push(tx);
    }

    let db = db.clone();
    let state_update = tokio::task::spawn_blocking(move || -> Result<TxStateUpdate> {
        update_tx_state(&db, prev_state_root, &writes)
    })
    .await??;

    let tx_list: BlockTxList = txs.iter().collect();
    let last_block = snapshot
        .get_block(last_block_height)
        .context("Failed to get the last block.")?;
    let block_header = BlockHeader::new(
        next_block_height,
        last_block.to_digest(),
        Utc::now(),
        tx_list,
        state_update.root,
    );
    let new_blk = new_block_fn(block_header, last_block).await?;
    let blk_proposal = BlockProposal::new(new_blk, txs);

    snapshot.remove_oldest_block()?;
    snapshot.commit_block(blk_proposal.get_block().clone());

    let end = Instant::now();
    record_event!("propose_end", "height": blk_proposal.get_block_height().0);
    info!(time = ?(end - begin));
    Ok(Some((blk_proposal, state_update)))
}
