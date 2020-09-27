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
    config::MinerConfig,
    db::DBPtr,
};
use futures::{channel::mpsc, prelude::*, stream::Fuse};
use pin_project::pin_project;
use slimchain_common::{
    basic::BlockHeight,
    collections::HashMap,
    error::{bail, Result},
    tx_req::SignedTxRequest,
};
use std::{
    cmp::Ordering,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::task::JoinHandle;

#[pin_project]
pub struct OrderedBlockStream<Input: Stream<Item = Block>> {
    #[pin]
    input: Fuse<Input>,
    height: BlockHeight,
    cache: HashMap<BlockHeight, Block>,
}

impl<Input: Stream<Item = Block>> OrderedBlockStream<Input> {
    pub fn new(input: Input, height: BlockHeight) -> Self {
        Self {
            input: input.fuse(),
            height,
            cache: HashMap::new(),
        }
    }
}

impl<Input: Stream<Item = Block>> Stream for OrderedBlockStream<Input> {
    type Item = Block;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        if let Some(item) = this.cache.remove(&this.height) {
            *this.height = this.height.next_height();
            return Poll::Ready(Some(item));
        }

        if let Poll::Ready(Some(item)) = this.input.as_mut().poll_next(cx) {
            let item_height = item.block_height();
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

pub struct BlockImportWorker {
    handle: Option<JoinHandle<()>>,
    blk_tx: mpsc::UnboundedSender<Block>,
}

impl BlockImportWorker {
    pub fn new(db: DBPtr, height: BlockHeight) -> Self {
        let (blk_tx, blk_rx) = mpsc::unbounded::<Block>();
        let mut blk_rx = OrderedBlockStream::new(blk_rx, height.next_height());

        let handle: JoinHandle<()> = tokio::spawn(async move {
            let mut height = height;
            while let Some(blk) = blk_rx.next().await {
                let state_update = match verify_block(&db, height, &blk, verify_consensus).await {
                    Ok(state_update) => state_update,
                    Err(e) => {
                        error!("Failed to import block. Error: {}", e);
                        continue;
                    }
                };

                commit_block(&db, &blk, &state_update)
                    .await
                    .expect("Failed to commit the block.");

                height = height.next_height();
            }
        });

        Self {
            handle: Some(handle),
            blk_tx,
        }
    }

    pub fn add_block(&mut self, block: Block) {
        if let Err(e) = self.blk_tx.start_send(block) {
            error!("Failed to send block. Error: {}", e);
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

pub struct BlockProposalWorker {
    handle: Option<JoinHandle<()>>,
    tx_tx: mpsc::UnboundedSender<SignedTxRequest>,
    blk_rx: Fuse<mpsc::UnboundedReceiver<Block>>,
}

impl BlockProposalWorker {
    pub fn new(miner_cfg: MinerConfig, db: DBPtr, height: BlockHeight) -> Self {
        let (tx_tx, tx_rx) = mpsc::unbounded::<SignedTxRequest>();
        let mut tx_rx = tx_rx.fuse();

        let (mut blk_tx, blk_rx) = mpsc::unbounded::<Block>();
        let blk_rx = blk_rx.fuse();

        let handle: JoinHandle<()> = tokio::spawn(async move {
            let mut height = height;
            while let Some((block, state_update)) =
                propose_block(&miner_cfg, &db, height, &mut tx_rx, create_new_block)
                    .await
                    .expect("Failed to build the new block.")
            {
                commit_block(&db, &block, &state_update)
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
        if let Some(handler) = self.handle.take() {
            handler.await?;
        } else {
            bail!("Already shutdown.");
        }
        Ok(())
    }
}
