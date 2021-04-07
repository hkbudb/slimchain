use crate::{block_proposal::BlockProposal, snapshot::Snapshot};
use slimchain_chain::{block::BlockTrait, config::ChainConfig, db::DBPtr};
use slimchain_common::{
    error::{bail, ensure, Context as _, Result},
    rw_set::TxWriteData,
    tx::TxTrait,
};
use slimchain_tx_state::{update_tx_state, TxStateUpdate};
use slimchain_utils::record_time;
use std::time::Instant;

#[tracing::instrument(level = "info", skip(chain_cfg, db, snapshot, blk_proposal, verify_consensus_fn), fields(height = blk_proposal.get_block_height().0), err)]
pub async fn verify_block<Tx, Block, VerifyConsensusFn>(
    chain_cfg: &ChainConfig,
    db: &DBPtr,
    snapshot: &mut Snapshot<Block>,
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
    let prev_state_root = last_block.state_root();

    blk_proposal.get_block().verify_block_header(last_block)?;
    verify_consensus_fn(blk_proposal.get_block(), last_block)?;

    snapshot.access_map.alloc_new_block();
    let mut writes = TxWriteData::default();

    for tx in blk_proposal.get_txs() {
        let tx_block_height = tx.tx_block_height();
        let tx_block = match snapshot.get_block(tx_block_height) {
            Some(blk) => blk,
            None => bail!(
                "Outdated tx in the block proposal. blk_height={}, tx_height={}",
                blk_proposal.get_block_height(),
                tx_block_height
            ),
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

    let db = db.clone();
    let state_update = tokio::task::spawn_blocking(move || -> Result<TxStateUpdate> {
        update_tx_state(&db, prev_state_root, &writes)
    })
    .await??;
    let new_state_root = state_update.root;

    ensure!(
        blk_proposal.get_block().state_root() == new_state_root,
        "Invalid state root in the block proposal (expect: {}, actual: {}).",
        blk_proposal.get_block().state_root(),
        new_state_root,
    );

    snapshot.commit_block(blk_proposal.get_block().clone());
    snapshot.remove_oldest_block()?;

    let time = Instant::now() - begin;
    record_time!("verify_block", time, "height": blk_proposal.get_block_height().0);
    info!(?time);
    Ok(state_update)
}
