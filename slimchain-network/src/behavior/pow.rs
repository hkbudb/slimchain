pub mod client;
pub use client::*;

pub mod miner;
pub use miner::*;

pub mod storage;
pub use storage::*;

use futures::{
    channel::{mpsc, oneshot},
    prelude::*,
    stream::Fuse,
};
use serde::Serialize;
use slimchain_chain::{
    behavior::{commit_block, commit_block_storage_node, propose_block, verify_block},
    block_proposal::BlockProposal,
    config::{ChainConfig, MinerConfig},
    consensus::pow::{create_new_block, verify_consensus, Block},
    db::{DBPtr, Transaction as DBTx},
    latest::{LatestBlockHeaderPtr, LatestTxCountPtr},
    snapshot::Snapshot,
};
use slimchain_common::{
    error::{bail, Result},
    tx::TxTrait,
};
use slimchain_tx_state::{TxProposal, TxTrie, TxTrieTrait};
use slimchain_utils::ordered_stream::OrderedStream;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::task::JoinHandle;

pub struct BlockImportWorker<Tx: TxTrait + 'static> {
    handle: Option<JoinHandle<()>>,
    blk_tx: mpsc::UnboundedSender<BlockProposal<Block, Tx>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl<Tx: TxTrait + Serialize> BlockImportWorker<Tx> {
    pub fn new<TxTrie: TxTrieTrait + 'static>(
        storage_node: bool,
        chain_cfg: ChainConfig,
        mut snapshot: Snapshot<Block, TxTrie>,
        latest_block_header: LatestBlockHeaderPtr,
        latest_tx_count: LatestTxCountPtr,
        db: DBPtr,
        snapshot_to_db_tx: impl Fn(&Snapshot<Block, TxTrie>) -> Result<DBTx> + Send + Sync + 'static,
    ) -> Self {
        let (blk_tx, blk_rx) = mpsc::unbounded::<BlockProposal<Block, Tx>>();
        let mut blk_rx = OrderedStream::new(
            blk_rx.map(|blk| (blk.get_block_height(), blk)),
            latest_block_header.get_height().next_height(),
            |height| height.next_height(),
        );
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        let handle: JoinHandle<()> = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    Some(blk_proposal) = blk_rx.next() => {
                        let snapshot_backup = snapshot.clone();
                        let state_update = match verify_block(
                            &chain_cfg,
                            &mut snapshot,
                            &blk_proposal,
                            verify_consensus,
                        ).await
                        {
                            Ok(state_update) => state_update,
                            Err(e) => {
                                error!("Failed to import block. Error: {}", e);
                                snapshot = snapshot_backup;
                                continue;
                            }
                        };

                        let commit_res = if storage_node {
                            commit_block_storage_node(
                                &blk_proposal,
                                &state_update,
                                &db,
                                &latest_block_header,
                                &latest_tx_count,
                            )
                            .await
                        } else {
                            commit_block(&blk_proposal, &db, &latest_block_header, &latest_tx_count)
                                .await
                        };

                        if let Err(e) = commit_res {
                            if let Ok(db_tx) = snapshot_to_db_tx(&snapshot_backup) {
                                db.write_async(db_tx).await.ok();
                            }
                            panic!("Failed to commit the block. Error: {}", e);
                        }
                    }
                }
            }

            db.write_async(snapshot_to_db_tx(&snapshot).expect("Failed to save the snapshot."))
                .await
                .expect("Failed to save the snapshot.");
        });

        Self {
            handle: Some(handle),
            blk_tx,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    pub fn add_block_proposal(&mut self, block_proposal: BlockProposal<Block, Tx>) {
        if let Err(e) = self.blk_tx.start_send(block_proposal) {
            error!("Failed to send block proposal. Error: {}", e);
        }
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        self.blk_tx.close_channel();
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            shutdown_tx.send(()).ok();
        } else {
            bail!("Already shutdown.");
        }
        if let Some(handler) = self.handle.take() {
            handler.await?;
        } else {
            bail!("Already shutdown.");
        }
        Ok(())
    }
}

pub struct BlockProposalWorker<Tx: TxTrait + 'static> {
    handle: Option<JoinHandle<()>>,
    tx_tx: mpsc::UnboundedSender<TxProposal<Tx>>,
    blk_rx: Fuse<mpsc::UnboundedReceiver<BlockProposal<Block, Tx>>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl<Tx: TxTrait + Serialize> BlockProposalWorker<Tx> {
    pub fn new(
        chain_cfg: ChainConfig,
        miner_cfg: MinerConfig,
        mut snapshot: Snapshot<Block, TxTrie>,
        latest_block_header: LatestBlockHeaderPtr,
        latest_tx_count: LatestTxCountPtr,
        db: DBPtr,
    ) -> Self {
        let (tx_tx, tx_rx) = mpsc::unbounded::<TxProposal<Tx>>();
        let mut tx_rx = tx_rx.fuse().peekable();

        let (mut blk_tx, blk_rx) = mpsc::unbounded::<BlockProposal<Block, Tx>>();
        let blk_rx = blk_rx.fuse();

        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        let handle: JoinHandle<()> = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    res = Pin::new(&mut tx_rx).peek() => {
                        if res.is_none() {
                            break;
                        }
                    }
                }

                let snapshot_backup = snapshot.clone();
                let blk_proposal = match propose_block(
                    &chain_cfg,
                    &miner_cfg,
                    &mut snapshot,
                    &mut tx_rx,
                    create_new_block,
                )
                .await
                {
                    Ok(blk_proposal) => blk_proposal,
                    Err(e) => {
                        snapshot_backup.write_async(&db).await.ok();
                        panic!("Failed to build the new block. Error: {}", e);
                    }
                };

                match blk_proposal {
                    Some(blk_proposal) => {
                        if let Err(e) =
                            commit_block(&blk_proposal, &db, &latest_block_header, &latest_tx_count)
                                .await
                        {
                            snapshot_backup.write_async(&db).await.ok();
                            panic!("Failed to commit the new block. Error: {}", e);
                        }
                        if let Err(e) = blk_tx.start_send(blk_proposal) {
                            snapshot_backup.write_async(&db).await.ok();
                            panic!("Failed to send the block proposal. Error: {}", e);
                        }
                    }
                    None => {
                        snapshot = snapshot_backup;
                        break;
                    }
                }
            }

            snapshot
                .write_async(&db)
                .await
                .expect("Failed to save the snapshot.");
        });

        Self {
            handle: Some(handle),
            tx_tx,
            blk_rx,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    pub fn add_tx_proposal(&mut self, tx_proposal: TxProposal<Tx>) {
        if let Err(e) = self.tx_tx.start_send(tx_proposal) {
            error!("Failed to send tx proposal. Error: {}", e);
        }
    }

    pub fn poll_block_proposal(&mut self, cx: &mut Context<'_>) -> Poll<BlockProposal<Block, Tx>> {
        Pin::new(&mut self.blk_rx)
            .poll_next(cx)
            .map(|res| res.expect("Failed to get the block proposal."))
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        self.tx_tx.close_channel();
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            shutdown_tx.send(()).ok();
        } else {
            bail!("Already shutdown.");
        }
        if let Some(handler) = self.handle.take() {
            handler.await?;
        } else {
            bail!("Already shutdown.");
        }
        Ok(())
    }
}
