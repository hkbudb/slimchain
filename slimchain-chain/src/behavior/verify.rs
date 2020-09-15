use crate::{
    block::BlockTrait,
    block_proposal::{BlockProposal, BlockProposalTrie},
    config::ChainConfig,
    snapshot::Snapshot,
};
use slimchain_common::{
    error::{bail, ensure, Context as _, Result},
    rw_set::TxWriteData,
    tx::TxTrait,
};
use slimchain_tx_state::{TxStateUpdate, TxTrie, TxTrieTrait};
use slimchain_utils::record_time;
use std::time::Instant;

#[tracing::instrument(skip(chain_cfg, snapshot, blk_proposal, verify_consensus_fn), fields(height = blk_proposal.get_block().block_height().0), err)]
pub async fn verify_block<Tx, Block, VerifyConsensusFn>(
    chain_cfg: &ChainConfig,
    snapshot: &mut Snapshot<Block, TxTrie>,
    blk_proposal: &BlockProposal<Block, Tx>,
    verify_consensus_fn: VerifyConsensusFn,
) -> Result<TxStateUpdate>
where
    Tx: TxTrait,
    Block: BlockTrait,
    VerifyConsensusFn: Fn(&Block, &Block) -> Result<()>,
{
    let begin = Instant::now();
    let last_block = snapshot
        .get_latest_block()
        .context("Failed to get the last block")?;

    blk_proposal.get_block().verify_block_header(last_block)?;
    verify_consensus_fn(blk_proposal.get_block(), last_block)?;

    match blk_proposal.get_trie() {
        BlockProposalTrie::Trie(trie) => {
            trie.verify(last_block.state_root())?;
            snapshot.tx_trie.update_missing_branches(trie)?;
        }
        BlockProposalTrie::Diff(diff) => {
            snapshot.tx_trie.apply_diff(diff, true)?;
        }
    }

    snapshot.access_map.alloc_new_block();
    let mut writes = TxWriteData::default();

    for tx in blk_proposal.get_txs() {
        let tx_block_height = tx.tx_block_height();
        let tx_block = match snapshot.get_block(tx_block_height) {
            Some(blk) => blk,
            None => bail!("Outdated tx in the block proposal."),
        };

        ensure!(
            tx.tx_state_root() == tx_block.state_root(),
            "Tx with invalid state root."
        );

        tx.verify_sig().context("Tx with invalid sig.")?;

        ensure!(
            !chain_cfg.conflict_check.has_conflict(
                &snapshot.access_map,
                tx_block_height,
                tx.tx_reads(),
                tx.tx_writes(),
            ),
            "Tx with conflict."
        );

        snapshot.access_map.add_read(tx.tx_reads());
        snapshot.access_map.add_write(tx.tx_writes());
        writes.merge(tx.tx_writes());
    }

    let update = snapshot.tx_trie.apply_writes(&writes)?;
    ensure!(
        blk_proposal.get_block().state_root() == snapshot.tx_trie.root_hash(),
        "Invalid state root in the block proposal."
    );

    snapshot.commit_block(blk_proposal.get_block().clone());
    snapshot.remove_oldest_block()?;

    record_time!("verify-block", Instant::now() - begin, "height": blk_proposal.get_block().block_height().0);
    Ok(update)
}
