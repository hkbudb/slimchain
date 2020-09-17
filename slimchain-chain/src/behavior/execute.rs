use crate::{db::DBPtr, latest::get_latest_block_height_and_state_root};
use futures::{prelude::*, stream::Fuse};
use pin_project::pin_project;
use slimchain_common::{tx::TxTrait, tx_req::SignedTxRequest};
use slimchain_tx_engine::{TxEngine, TxTask};
use slimchain_tx_state::TxProposal;
use std::{
    pin::Pin,
    task::{Context, Poll},
};

#[pin_project]
pub struct TxExecuteStream<Tx: TxTrait + 'static, Input: Stream<Item = SignedTxRequest>> {
    #[pin]
    input: Fuse<Input>,
    engine: TxEngine<Tx>,
    db: DBPtr,
}

impl<Tx: TxTrait, Input: Stream<Item = SignedTxRequest>> TxExecuteStream<Tx, Input> {
    pub fn new(input: Input, engine: TxEngine<Tx>, db: DBPtr) -> Self {
        let input = input.fuse();
        Self { input, engine, db }
    }
}

impl<Tx: TxTrait, Input: Stream<Item = SignedTxRequest>> Stream for TxExecuteStream<Tx, Input> {
    type Item = TxProposal<Tx>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        let mut input_exhausted = false;
        while let Poll::Ready(req) = this.input.as_mut().poll_next(cx) {
            match req {
                Some(req) => {
                    let (block_height, state_root) = get_latest_block_height_and_state_root()
                        .expect("Failed to get the latest block info.");
                    let task = TxTask::new(block_height, this.db.clone(), state_root, req);
                    this.engine.push_task(task);
                }
                None => {
                    input_exhausted = true;
                }
            }
        }

        match this.engine.pop_result() {
            Some(result) => Poll::Ready(Some(result.tx_proposal)),
            None => {
                if input_exhausted && this.engine.remaining_tasks() == 0 {
                    Poll::Ready(None)
                } else {
                    Poll::Pending
                }
            }
        }
    }
}
