use crate::{
    access_map::AccessMap,
    block::{BlockHeader, BlockTrait, BlockTxList},
    block_proposal::{BlockProposal, BlockProposalTrie},
    config::{ChainConfig, MinerConfig},
    loader::BlockLoaderTrait,
};
use chrono::Utc;
use futures::prelude::*;
use itertools::Itertools;
use slimchain_common::{digest::Digestible, error::Result, tx::TxTrait};
use slimchain_tx_state::{merge_tx_trie_diff, TxProposal, TxTrie, TxTrieTrait, TxWriteSetTrie};
use std::time::{Duration, Instant};
use tokio::time::timeout_at;

pub async fn propose_block<Tx, Block, BlockLoader, TxStream, NewBlockFn>(
    chain_cfg: &ChainConfig,
    miner_cfg: &MinerConfig,
    access_map: &mut AccessMap,
    tx_trie: &mut TxTrie,
    block_loader: &BlockLoader,
    tx_proposals: &mut TxStream,
    new_block_fn: NewBlockFn,
) -> Result<(Option<BlockProposal<Block, Tx>>, Duration)>
where
    Tx: TxTrait,
    Block: BlockTrait,
    BlockLoader: BlockLoaderTrait<Block>,
    TxStream: Stream<Item = TxProposal<Tx>> + Unpin,
    NewBlockFn: Fn(BlockHeader, BlockTxList, &Block) -> Result<Block>,
{
    let begin = Instant::now();
    let deadline = begin + miner_cfg.max_block_interval;

    let mut txs: Vec<Tx> = Vec::with_capacity(miner_cfg.max_txs);
    let mut tx_write_tries: Vec<TxWriteSetTrie> = Vec::with_capacity(miner_cfg.max_txs);

    let last_block_height = block_loader.latest_block_height();
    debug_assert_eq!(last_block_height, access_map.latest_block_height());
    let last_block = block_loader.get_block(last_block_height)?;

    access_map.alloc_new_block();

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
                break;
            }
        };

        let tx_block_height = tx.tx_block_height();
        if tx_block_height < access_map.oldest_block_height() {
            debug!("Tx proposal is outdated.");
            continue;
        }
        if tx_block_height > last_block_height {
            warn!("Tx proposal is too new.");
            continue;
        }
        let tx_block = block_loader.get_block(tx_block_height)?;

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
            access_map,
            tx_block_height,
            tx.tx_reads(),
            tx.tx_writes(),
        ) {
            debug!("Received a tx with conflict");
            continue;
        }

        access_map.add_read(tx.tx_reads());
        access_map.add_write(tx.tx_writes());

        txs.push(tx);
        tx_write_tries.push(write_trie);
    }

    if txs.len() < miner_cfg.min_txs {
        return Ok((None, Instant::now() - begin));
    }

    let tx_trie_diff = tx_write_tries
        .iter()
        .map(|t| tx_trie.diff_missing_branches(t))
        .tree_fold1(|lhs, rhs| match (lhs, rhs) {
            (Ok(l), Ok(r)) => Ok(merge_tx_trie_diff(&l, &r)),
            (l @ Err(_), _) => l,
            (_, r @ Err(_)) => r,
        })
        .expect("Failed to compute the TxTrieDiff")?;

    tx_trie.apply_diff(&tx_trie_diff, false)?;
    for tx in &txs {
        tx_trie.apply_writes(tx.tx_writes())?;
    }

    let new_state_root = tx_trie.root_hash();
    let tx_list: BlockTxList = txs.iter().collect();
    let block_header = BlockHeader::new(
        last_block_height.next_height(),
        last_block.prev_blk_hash(),
        Utc::now(),
        tx_list.to_digest(),
        new_state_root,
    );
    let new_blk = new_block_fn(block_header, tx_list, &last_block)?;
    let blk_proposal = BlockProposal::new(new_blk, txs, BlockProposalTrie::Diff(tx_trie_diff));

    let prune_data = access_map.remove_oldest_block();
    prune_data.prune_tx_trie(tx_trie)?;

    Ok((Some(blk_proposal), Instant::now() - begin))
}
