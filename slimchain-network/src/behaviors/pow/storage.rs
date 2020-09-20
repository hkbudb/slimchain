use super::BlockImportWorker;
use crate::{
    config::NetworkConfig,
    control::Shutdown,
    discovery::{Discovery, DiscoveryEvent},
    pubsub::{PubSub, PubSubEvent, PubSubTopic},
    rpc::{
        create_request_response_server, handle_request_response_server_event, RpcInstant,
        RpcRequestResponseEvent,
    },
};
use async_trait::async_trait;
use futures::{channel::mpsc, prelude::*};
use libp2p::{
    swarm::{NetworkBehaviourAction, NetworkBehaviourEventProcess, PollParameters},
    NetworkBehaviour,
};
use serde::Serialize;
use slimchain_chain::{
    behavior::TxExecuteStream, block_proposal::BlockProposal, config::ChainConfig,
    consensus::pow::Block, db::DBPtr, role::Role, snapshot::Snapshot,
};
use slimchain_common::{basic::ShardId, error::Result, tx::TxTrait, tx_req::SignedTxRequest};
use slimchain_tx_engine::TxEngine;
use slimchain_tx_state::{StorageTxTrie, TxProposal};
use std::{
    pin::Pin,
    task::{Context, Poll},
};

#[derive(NetworkBehaviour)]
#[behaviour(poll_method = "poll_inner")]
pub struct StorageBehavior<Tx: TxTrait + Serialize + 'static> {
    discv: Discovery,
    pubsub: PubSub<TxProposal<Tx>, BlockProposal<Block, Tx>>,
    rpc_server: RpcInstant<SignedTxRequest, ()>,
    #[behaviour(ignore)]
    import_worker: BlockImportWorker<Tx>,
    #[behaviour(ignore)]
    tx_req_tx: mpsc::UnboundedSender<SignedTxRequest>,
    #[behaviour(ignore)]
    tx_exec_stream: TxExecuteStream<Tx, mpsc::UnboundedReceiver<SignedTxRequest>>,
}

impl<Tx: TxTrait + Serialize> StorageBehavior<Tx> {
    pub fn new(
        db: DBPtr,
        engine: TxEngine<Tx>,
        shard_id: ShardId,
        chain_cfg: &ChainConfig,
        net_cfg: &NetworkConfig,
    ) -> Result<Self> {
        let keypair = net_cfg.keypair.to_libp2p_keypair();
        let discv = Discovery::new(keypair.public(), Role::Storage(shard_id), net_cfg.mdns)?;
        let pubsub = PubSub::new(keypair, &[PubSubTopic::BlockProposal]);
        let rpc_server = create_request_response_server("/tx_req/1");
        let snapshot =
            Snapshot::<Block, StorageTxTrie>::load_from_db(&db, chain_cfg.state_len, shard_id)?;
        let latest_block_header = snapshot.to_latest_block_header();

        let (tx_req_tx, tx_req_rx) = mpsc::unbounded::<SignedTxRequest>();
        let tx_exec_stream = TxExecuteStream::new(tx_req_rx, engine, &db, &latest_block_header);

        let import_worker = BlockImportWorker::new(
            false,
            chain_cfg.clone(),
            snapshot,
            latest_block_header,
            db,
            |snapshot| snapshot.write_db_tx(),
        );

        Ok(Self {
            discv,
            pubsub,
            rpc_server,
            import_worker,
            tx_req_tx,
            tx_exec_stream,
        })
    }

    fn poll_inner<T>(
        &mut self,
        cx: &mut Context,
        _: &mut impl PollParameters,
    ) -> Poll<NetworkBehaviourAction<T, ()>> {
        if let Poll::Ready(Some(tx_proposal)) = Pin::new(&mut self.tx_exec_stream).poll_next(cx) {
            self.pubsub
                .publish_tx_proposal(&tx_proposal)
                .expect("Failed to publish tx proposal.");
        }

        Poll::Pending
    }
}

impl<Tx: TxTrait + Serialize> NetworkBehaviourEventProcess<DiscoveryEvent> for StorageBehavior<Tx> {
    fn inject_event(&mut self, _: DiscoveryEvent) {}
}

impl<Tx: TxTrait + Serialize>
    NetworkBehaviourEventProcess<RpcRequestResponseEvent<SignedTxRequest, ()>>
    for StorageBehavior<Tx>
{
    fn inject_event(&mut self, event: RpcRequestResponseEvent<SignedTxRequest, ()>) {
        if let Some((tx_req, channel)) = handle_request_response_server_event(event) {
            debug!(tx_req_id = %tx_req.id(), "Recv TxReq");
            self.tx_req_tx
                .start_send(tx_req)
                .expect("Failed to send tx_req to TxEngine.");
            self.rpc_server.send_response(channel, ());
        }
    }
}

impl<Tx: TxTrait + Serialize>
    NetworkBehaviourEventProcess<PubSubEvent<TxProposal<Tx>, BlockProposal<Block, Tx>>>
    for StorageBehavior<Tx>
{
    fn inject_event(&mut self, event: PubSubEvent<TxProposal<Tx>, BlockProposal<Block, Tx>>) {
        match event {
            PubSubEvent::BlockProposal(input) => {
                debug!(
                    height = input.get_block_height().0,
                    txs = input.get_txs().len(),
                    "Recv block proposal."
                );
                self.import_worker.add_block_proposal(input);
            }
            _ => {
                unreachable!();
            }
        }
    }
}

#[async_trait]
impl<Tx: TxTrait + Serialize> Shutdown for StorageBehavior<Tx> {
    async fn shutdown(&mut self) -> Result<()> {
        self.import_worker.shutdown().await
    }
}