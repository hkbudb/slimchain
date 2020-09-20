pub mod client;
pub use client::*;

pub mod miner;
pub use miner::*;

pub mod storage;
pub use storage::*;

use futures::{channel::mpsc, prelude::*, stream::Fuse};
use pin_project::pin_project;
use serde::Serialize;
use slimchain_chain::{
    behavior::{commit_block, commit_block_storage_node, propose_block, verify_block},
    block_proposal::BlockProposal,
    config::{ChainConfig, MinerConfig},
    consensus::pow::{create_new_block, verify_consensus, Block},
    db::{DBPtr, Transaction as DBTx},
    latest::LatestBlockHeaderPtr,
    snapshot::Snapshot,
};
use slimchain_common::{
    basic::BlockHeight,
    collections::HashMap,
    error::{bail, Result},
    tx::TxTrait,
};
use slimchain_tx_state::{TxProposal, TxTrie, TxTrieTrait};
use std::{
    cmp::Ordering,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::task::JoinHandle;

#[pin_project]
pub struct OrderedBlockProposalStream<
    Tx: TxTrait + 'static,
    Input: Stream<Item = BlockProposal<Block, Tx>>,
> {
    #[pin]
    input: Fuse<Input>,
    height: BlockHeight,
    cache: HashMap<BlockHeight, BlockProposal<Block, Tx>>,
}

impl<Tx: TxTrait, Input: Stream<Item = BlockProposal<Block, Tx>>>
    OrderedBlockProposalStream<Tx, Input>
{
    pub fn new(input: Input, height: BlockHeight) -> Self {
        Self {
            input: input.fuse(),
            height,
            cache: HashMap::new(),
        }
    }
}

impl<Tx: TxTrait, Input: Stream<Item = BlockProposal<Block, Tx>>> Stream
    for OrderedBlockProposalStream<Tx, Input>
{
    type Item = BlockProposal<Block, Tx>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        if let Some(item) = this.cache.remove(&this.height) {
            *this.height = this.height.next_height();
            return Poll::Ready(Some(item));
        }

        if let Poll::Ready(Some(item)) = this.input.as_mut().poll_next(cx) {
            let item_height = item.get_block_height();
            match item_height.cmp(this.height) {
                Ordering::Equal => {
                    *this.height = this.height.next_height();
                    return Poll::Ready(Some(item));
                }
                Ordering::Greater => {
                    this.cache.insert(item_height, item);
                }
                Ordering::Less => warn!(
                    height = item_height.0,
                    cur_height = this.height.0,
                    "Received outdated block."
                ),
            }
        }

        if this.input.is_done() && !this.cache.contains_key(&this.height) {
            return Poll::Ready(None);
        }

        Poll::Pending
    }
}

pub struct BlockImportWorker<Tx: TxTrait + 'static> {
    handle: Option<JoinHandle<()>>,
    blk_tx: mpsc::UnboundedSender<BlockProposal<Block, Tx>>,
}

impl<Tx: TxTrait + Serialize> BlockImportWorker<Tx> {
    pub fn new<TxTrie: TxTrieTrait + 'static>(
        storage_node: bool,
        chain_cfg: ChainConfig,
        mut snapshot: Snapshot<Block, TxTrie>,
        latest_block_header: LatestBlockHeaderPtr,
        db: DBPtr,
        snapshot_to_db_tx: impl Fn(&Snapshot<Block, TxTrie>) -> Result<DBTx> + Send + Sync + 'static,
    ) -> Self {
        let (blk_tx, blk_rx) = mpsc::unbounded::<BlockProposal<Block, Tx>>();
        let mut blk_rx =
            OrderedBlockProposalStream::new(blk_rx, latest_block_header.get_height().next_height());

        let handle: JoinHandle<()> = tokio::spawn(async move {
            while let Some(blk_proposal) = blk_rx.next().await {
                let snapshot_backup = snapshot.clone();
                let state_update =
                    match verify_block(&chain_cfg, &mut snapshot, &blk_proposal, verify_consensus)
                        .await
                    {
                        Ok(state_update) => state_update,
                        Err(e) => {
                            snapshot = snapshot_backup;
                            error!("Failed to import block. Error: {}", e);
                            continue;
                        }
                    };

                let commit_res = if storage_node {
                    commit_block_storage_node(
                        &blk_proposal,
                        &state_update,
                        &db,
                        &latest_block_header,
                    )
                    .await
                } else {
                    commit_block(&blk_proposal, &db, &latest_block_header).await
                };

                if let Err(e) = commit_res {
                    if let Ok(db_tx) = snapshot_to_db_tx(&snapshot) {
                        db.write_async(db_tx).await.ok();
                    }
                    panic!("Failed to commit the block. Error: {}", e);
                }
            }

            db.write_async(snapshot_to_db_tx(&snapshot).expect("Failed to save the snapshot."))
                .await
                .expect("Failed to save the snapshot.");
        });

        Self {
            handle: Some(handle),
            blk_tx,
        }
    }

    pub fn add_block_proposal(&mut self, block_proposal: BlockProposal<Block, Tx>) {
        if let Err(e) = self.blk_tx.start_send(block_proposal) {
            error!("Failed to send block proposal. Error: {}", e);
        }
    }

    pub async fn shutdown(&mut self) -> Result<()> {
        self.blk_tx.close_channel();
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
}

impl<Tx: TxTrait + Serialize> BlockProposalWorker<Tx> {
    pub fn new(
        chain_cfg: ChainConfig,
        miner_cfg: MinerConfig,
        mut snapshot: Snapshot<Block, TxTrie>,
        latest_block_header: LatestBlockHeaderPtr,
        db: DBPtr,
    ) -> Self {
        let (tx_tx, tx_rx) = mpsc::unbounded::<TxProposal<Tx>>();
        let mut tx_rx = tx_rx.fuse();

        let (mut blk_tx, blk_rx) = mpsc::unbounded::<BlockProposal<Block, Tx>>();
        let blk_rx = blk_rx.fuse();

        let handle: JoinHandle<()> = tokio::spawn(async move {
            loop {
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
                        if let Err(e) = commit_block(&blk_proposal, &db, &latest_block_header).await
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
                        snapshot
                            .write_async(&db)
                            .await
                            .expect("Failed to save the snapshot.");
                        return;
                    }
                }
            }
        });

        Self {
            handle: Some(handle),
            tx_tx,
            blk_rx,
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
        if let Some(handler) = self.handle.take() {
            handler.await?;
        } else {
            bail!("Already shutdown.");
        }
        Ok(())
    }
}
