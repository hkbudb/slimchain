use crate::{db::DBPtr, latest::LatestBlockHeaderPtr};
use futures::{prelude::*, ready, stream::Fuse};
use pin_project::pin_project;
use slimchain_common::{
    basic::{BlockHeight, H256},
    tx::TxTrait,
    tx_req::SignedTxRequest,
};
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
    latest_block_header: LatestBlockHeaderPtr,
}

impl<Tx: TxTrait, Input: Stream<Item = SignedTxRequest>> TxExecuteStream<Tx, Input> {
    pub fn new(
        input: Input,
        engine: TxEngine<Tx>,
        db: &DBPtr,
        latest_block_header: &LatestBlockHeaderPtr,
    ) -> Self {
        let input = input.fuse();
        let db = db.clone();
        let latest_block_header = latest_block_header.clone();
        Self {
            input,
            engine,
            db,
            latest_block_header,
        }
    }
}

impl<Tx: TxTrait, Input: Stream<Item = SignedTxRequest>> Stream for TxExecuteStream<Tx, Input> {
    type Item = TxProposal<Tx>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        while let Poll::Ready(Some(req)) = this.input.as_mut().poll_next(cx) {
            let latest_block_header = this.latest_block_header.clone();
            let task = TxTask::new(this.db.clone(), req, move || -> (BlockHeight, H256) {
                latest_block_header.get_height_and_state_root()
            });
            this.engine.push_task(task);
        }

        if this.input.is_done() && this.engine.remaining_tasks() == 0 {
            return Poll::Ready(None);
        }

        if this.engine.is_shutdown() {
            return Poll::Ready(None);
        }

        let result = ready!(this.engine.poll_result(cx));
        Poll::Ready(Some(result.tx_proposal))
    }
}
