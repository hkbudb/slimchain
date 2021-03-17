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
    basic::{BlockHeight, H256},
    error::{Context as _, Result},
    rw_set::TxWriteData,
    tx::TxTrait,
};
use slimchain_tx_state::{
    merge_tx_trie_diff, TxProposal, TxTrie, TxTrieDiff, TxTrieTrait, TxWriteSetTrie,
};
use slimchain_utils::record_event;
use std::time::Instant;
use tokio::time::timeout_at;

enum TxTries {
    Diff(Vec<TxTrieDiff>),
    UncompressedTries(Vec<(BlockHeight, TxWriteSetTrie)>),
}

#[tracing::instrument(level = "info", skip(chain_cfg, miner_cfg, snapshot, tx_proposals, new_block_fn), fields(height = snapshot.current_height().0 + 1), err)]
pub async fn propose_block<Tx, Block, TxStream, NewBlockFn, NewBlockFnOutput>(
    chain_cfg: &ChainConfig,
    miner_cfg: &MinerConfig,
    snapshot: &mut Snapshot<Block, TxTrie>,
    tx_proposals: &mut TxStream,
    new_block_fn: NewBlockFn,
) -> Result<Option<BlockProposal<Block, Tx>>>
where
    Tx: TxTrait,
    Block: BlockTrait + 'static,
    TxStream: Stream<Item = TxProposal<Tx>> + Unpin,
    NewBlockFn: Fn(BlockHeader, &Block) -> NewBlockFnOutput,
    NewBlockFnOutput: Future<Output = Result<Block>> + Send + 'static,
{
    let begin = Instant::now();
    let deadline = begin + miner_cfg.max_block_interval;

    let mut txs: Vec<Tx> = Vec::with_capacity(miner_cfg.max_txs);
    let mut tx_tries = if miner_cfg.compress_trie {
        TxTries::Diff(Vec::with_capacity(miner_cfg.max_txs))
    } else {
        TxTries::UncompressedTries(Vec::with_capacity(miner_cfg.max_txs))
    };

    let last_block_height = snapshot.current_height();
    let next_block_height = last_block_height.next_height();

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

        let TxProposal { tx, write_trie } = match tx_proposal {
            Some(tx_proposal) => tx_proposal,
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

        if let Err(e) = write_trie.verify(tx_block.state_root()) {
            warn!("Received a tx with invalid write trie. Error: {:?}", e);
            record_event!("discard_tx", "tx_id": tx_id, "reason": "invalid_write_trie", "detail": std::format!("{}", e));
            continue;
        }

        snapshot.access_map.add_read(tx.tx_reads());
        snapshot.access_map.add_write(tx.tx_writes());
        writes.merge(tx.tx_writes());

        txs.push(tx);
        match &mut tx_tries {
            TxTries::Diff(diffs) => {
                let diff = snapshot.tx_trie.diff_missing_branches(&write_trie);
                diffs.push(diff);
            }
            TxTries::UncompressedTries(tries) => {
                tries.push((tx_block_height, write_trie));
            }
        }
    }

    let blk_proposal_trie = match tx_tries {
        TxTries::Diff(diffs) => {
            let merged_diff = diffs
                .into_iter()
                .tree_fold1(|l, r| merge_tx_trie_diff(&l, &r))
                .unwrap_or_default();
            snapshot.tx_trie.apply_diff(&merged_diff, false)?;
            BlockProposalTrie::Diff(merged_diff)
        }
        TxTries::UncompressedTries(tries) => {
            for (_, trie) in &tries {
                snapshot.tx_trie.update_missing_branches(trie)?;
            }
            BlockProposalTrie::UncompressedTries(tries)
        }
    };

    let (updated_trie, new_state_root) = {
        let mut trie = snapshot.tx_trie.clone();
        tokio::task::spawn_blocking(move || -> Result<(TxTrie, H256)> {
            trie.apply_writes(&writes)?;
            let root = trie.root_hash();
            Ok((trie, root))
        })
        .await??
    };
    snapshot.tx_trie = updated_trie;

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
    let new_blk = new_block_fn(block_header, last_block).await?;
    let blk_proposal = BlockProposal::new(new_blk, txs, blk_proposal_trie);

    snapshot.remove_oldest_block()?;
    snapshot.commit_block(blk_proposal.get_block().clone());

    let end = Instant::now();
    record_event!("propose_end", "height": blk_proposal.get_block_height().0);
    info!(time = ?(end - begin));
    Ok(Some(blk_proposal))
}
