use crate::{
    block::{BlockHeader, BlockTrait, BlockTxList},
    block_proposal::{BlockProposal, BlockProposalTrie},
    config::{ChainConfig, MinerConfig},
    snapshot::Snapshot,
};
use chrono::Utc;
use futures::prelude::*;
use itertools::Itertools;
use slimchain_common::{
    error::{Context as _, Result},
    rw_set::TxWriteData,
    tx::TxTrait,
};
use slimchain_tx_state::{merge_tx_trie_diff, TxProposal, TxTrie, TxTrieTrait, TxWriteSetTrie};
use slimchain_utils::record_event;
use std::time::Instant;
use tokio::time::timeout_at;

#[tracing::instrument(level = "info", skip(chain_cfg, miner_cfg, snapshot, tx_proposals, new_block_fn), fields(height = snapshot.current_height().0 + 1), err)]
pub async fn propose_block<Tx, Block, TxStream, NewBlockFn>(
    chain_cfg: &ChainConfig,
    miner_cfg: &MinerConfig,
    snapshot: &mut Snapshot<Block, TxTrie>,
    tx_proposals: &mut TxStream,
    new_block_fn: NewBlockFn,
) -> Result<Option<BlockProposal<Block, Tx>>>
where
    Tx: TxTrait,
    Block: BlockTrait,
    TxStream: Stream<Item = TxProposal<Tx>> + Unpin,
    NewBlockFn: Fn(BlockHeader, &Block) -> Block,
{
    let begin = Instant::now();
    let deadline = begin + miner_cfg.max_block_interval;

    let mut txs: Vec<Tx> = Vec::with_capacity(miner_cfg.max_txs);
    let mut tx_write_tries: Vec<TxWriteSetTrie> = Vec::with_capacity(miner_cfg.max_txs);

    let last_block_height = snapshot.current_height();
    let next_block_height = last_block_height.next_height();

    snapshot.access_map.alloc_new_block();
    let mut writes = TxWriteData::default();

    while txs.len() < miner_cfg.max_txs {
        let tx_proposal = if txs.len() < miner_cfg.min_txs {
            tx_proposals.next().await
        } else {
            match timeout_at(deadline.into(), tx_proposals.next()).await {
                Ok(tx_proposal) => tx_proposal,
                Err(_) => {
                    debug!("Wait tx proposal timeout.");
                    break;
                }
            }
        };

        let TxProposal { tx, write_trie } = match tx_proposal {
            Some(tx_proposal) => tx_proposal,
            None => {
                debug!("No tx proposal is available.");
                return Ok(None);
            }
        };

        record_event!("blk_recv_tx", "tx_id": tx.id(), "height": next_block_height.0);

        let tx_block_height = tx.tx_block_height();
        if tx_block_height < snapshot.access_map.oldest_block_height() {
            debug!("Tx proposal is outdated.");
            continue;
        }
        if tx_block_height > last_block_height {
            warn!("Tx proposal is too new.");
            continue;
        }
        let tx_block = snapshot
            .get_block(tx_block_height)
            .context("Failed to get the block for tx")?;

        if tx.tx_state_root() != tx_block.state_root() {
            warn!("Received a tx with invalid state root.");
            continue;
        }

        if let Err(e) = tx.verify_sig() {
            warn!("Received a tx with invalid sig. Error: {}", e);
            continue;
        }

        if let Err(e) = write_trie.verify(tx_block.state_root()) {
            warn!("Received a tx with invalid write trie. Error: {}", e);
            continue;
        }

        if chain_cfg.conflict_check.has_conflict(
            &snapshot.access_map,
            tx_block_height,
            tx.tx_reads(),
            tx.tx_writes(),
        ) {
            debug!("Received a tx with conflict");
            continue;
        }

        snapshot.access_map.add_read(tx.tx_reads());
        snapshot.access_map.add_write(tx.tx_writes());
        writes.merge(tx.tx_writes());

        txs.push(tx);
        tx_write_tries.push(write_trie);
    }

    let tx_trie_diff = tx_write_tries
        .iter()
        .map(|t| snapshot.tx_trie.diff_missing_branches(t))
        .tree_fold1(|lhs, rhs| match (lhs, rhs) {
            (Ok(l), Ok(r)) => Ok(merge_tx_trie_diff(&l, &r)),
            (l @ Err(_), _) => l,
            (_, r @ Err(_)) => r,
        })
        .transpose()?
        .unwrap_or_default();

    snapshot.tx_trie.apply_diff(&tx_trie_diff, false)?;
    snapshot.tx_trie.apply_writes(&writes)?;

    let new_state_root = snapshot.tx_trie.root_hash();
    let tx_list: BlockTxList = txs.iter().collect();
    let last_block = snapshot
        .get_block(last_block_height)
        .context("Failed to get the last block.")?;
    let block_header = BlockHeader::new(
        next_block_height,
        last_block.to_digest(),
        Utc::now(),
        tx_list,
        new_state_root,
    );
    let new_blk = new_block_fn(block_header, last_block);
    let blk_proposal = BlockProposal::new(new_blk, txs, BlockProposalTrie::Diff(tx_trie_diff));

    snapshot.remove_oldest_block()?;
    snapshot.commit_block(blk_proposal.get_block().clone());

    info!(time = ?(Instant::now() - begin));
    Ok(Some(blk_proposal))
}
