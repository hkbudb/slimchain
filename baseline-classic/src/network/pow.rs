pub mod client;
pub use client::*;

pub mod miner;
pub use miner::*;

use crate::{
    behavior::{commit_block, propose_block, verify_block},
    block::{
        pow::{create_new_block, verify_consensus, Block},
        BlockTrait,
    },
    db::DBPtr,
};
use futures::{
    channel::{mpsc, oneshot},
    prelude::*,
    stream::Fuse,
};
use slimchain_chain::{config::MinerConfig, latest::LatestTxCountPtr};
use slimchain_common::{
    basic::BlockHeight,
    error::{bail, Result},
    tx_req::SignedTxRequest,
};
use slimchain_utils::ordered_stream::OrderedStream;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::task::JoinHandle;

pub struct BlockImportWorker {
    handle: Option<JoinHandle<()>>,
    blk_tx: mpsc::UnboundedSender<Block>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl BlockImportWorker {
    pub fn new(db: DBPtr, height: BlockHeight, latest_tx_count: LatestTxCountPtr) -> Self {
        let (blk_tx, blk_rx) = mpsc::unbounded::<Block>();
        let mut blk_rx = OrderedStream::new(
            blk_rx.map(|blk| (blk.block_height(), blk)),
            height.next_height(),
            |height| height.next_height(),
        );
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        let handle: JoinHandle<()> = tokio::spawn(async move {
            let mut height = height;

            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    Some(blk) = blk_rx.next() => {
                        let state_update = match verify_block(&db, height, &blk, verify_consensus).await {
                            Ok(state_update) => state_update,
                            Err(e) => {
                                error!("Failed to import block. Error: {}", e);
                                continue;
                            }
                        };

                        commit_block(&db, &blk, &state_update, &latest_tx_count)
                            .await
                            .expect("Failed to commit the block.");

                        height = height.next_height();
                    }
                }
            }
        });

        Self {
            handle: Some(handle),
            blk_tx,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    pub fn add_block(&mut self, block: Block) {
        if let Err(e) = self.blk_tx.start_send(block) {
            error!("Failed to send block. Error: {}", e);
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

pub struct BlockProposalWorker {
    handle: Option<JoinHandle<()>>,
    tx_tx: mpsc::UnboundedSender<SignedTxRequest>,
    blk_rx: Fuse<mpsc::UnboundedReceiver<Block>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl BlockProposalWorker {
    pub fn new(
        miner_cfg: MinerConfig,
        db: DBPtr,
        height: BlockHeight,
        latest_tx_count: LatestTxCountPtr,
    ) -> Self {
        let (tx_tx, tx_rx) = mpsc::unbounded::<SignedTxRequest>();
        let mut tx_rx = tx_rx.fuse().peekable();

        let (mut blk_tx, blk_rx) = mpsc::unbounded::<Block>();
        let blk_rx = blk_rx.fuse();

        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        let handle: JoinHandle<()> = tokio::spawn(async move {
            let mut height = height;

            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    res = Pin::new(&mut tx_rx).peek() => {
                        if res.is_none() {
                            break;
                        }
                    }
                }

                let (block, state_update) =
                    match propose_block(&miner_cfg, &db, height, &mut tx_rx, create_new_block)
                        .await
                        .expect("Failed to build the new block.")
                    {
                        Some(res) => res,
                        None => break,
                    };

                commit_block(&db, &block, &state_update, &latest_tx_count)
                    .await
                    .expect("Failed to commit the block.");

                if let Err(e) = blk_tx.start_send(block) {
                    panic!("Failed to send the block. Error: {}", e);
                }

                height = height.next_height();
            }
        });

        Self {
            handle: Some(handle),
            tx_tx,
            blk_rx,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    pub fn add_tx(&mut self, tx: SignedTxRequest) {
        if let Err(e) = self.tx_tx.start_send(tx) {
            error!("Failed to send tx. Error: {}", e);
        }
    }

    pub fn poll_block(&mut self, cx: &mut Context<'_>) -> Poll<Block> {
        Pin::new(&mut self.blk_rx)
            .poll_next(cx)
            .map(|res| res.expect("Failed to get the block."))
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
