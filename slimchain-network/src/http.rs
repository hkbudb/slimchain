use futures::{channel::mpsc, future::BoxFuture, prelude::*};
use libp2p::{
    core::connection::ConnectionId,
    swarm::{
        protocols_handler::DummyProtocolsHandler, NetworkBehaviour, NetworkBehaviourAction,
        PollParameters, ProtocolsHandler,
    },
    Multiaddr, PeerId,
};
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::ShardId,
    error::{ensure, Error, Result},
    tx_req::{SignedTxRequest, TxReqId},
};
use std::net::SocketAddr;
use std::task::{Context, Poll};
use warp::Filter;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TxHttpRequest {
    pub req: SignedTxRequest,
    #[serde(default)]
    pub shard_id: ShardId,
}

const TX_REQ_ROUTE_PATH: &str = "tx_req";

pub async fn send_tx_request(endpoint: &str, req: SignedTxRequest) -> Result<TxReqId> {
    send_tx_request_with_shard(endpoint, req, ShardId::default()).await
}

pub async fn send_tx_request_with_shard(
    endpoint: &str,
    req: SignedTxRequest,
    shard_id: ShardId,
) -> Result<TxReqId> {
    let tx_req = TxHttpRequest { req, shard_id };
    let mut resp = surf::post(&format!("http://{}/{}", endpoint, TX_REQ_ROUTE_PATH))
        .body_json(&tx_req)?
        .await
        .map_err(Error::msg)?;
    ensure!(
        resp.status().is_success(),
        "Failed to send Tx http req (status code: {})",
        resp.status()
    );
    resp.body_json().await.map_err(Error::msg)
}

pub struct TxHttpServer {
    srv: BoxFuture<'static, ()>,
    recv: mpsc::Receiver<(TxReqId, TxHttpRequest)>,
}

#[derive(Debug)]
pub struct TxHttpServerErr(Error);

impl warp::reject::Reject for TxHttpServerErr {}

impl TxHttpServer {
    pub fn new(endpoint: &str) -> Result<Self> {
        info!("Create tx http server, listen on {}", endpoint);
        let listen_addr: SocketAddr = endpoint.parse()?;
        let (tx, rx) = mpsc::channel(1024);
        let route = warp::post()
            .and(warp::path(TX_REQ_ROUTE_PATH))
            .and(warp::body::json())
            .and_then(move |req: TxHttpRequest| {
                let req_id = TxReqId::next_id();

                let tx = tx.clone();
                async move {
                    match tx.clone().send((req_id, req)).await {
                        Ok(_) => Ok(warp::reply::json(&req_id)),
                        Err(e) => Err(warp::reject::custom(TxHttpServerErr(Error::msg(e)))),
                    }
                }
            });
        let srv = warp::serve(route).bind(listen_addr).boxed();
        Ok(Self { srv, recv: rx })
    }
}

impl NetworkBehaviour for TxHttpServer {
    type ProtocolsHandler = DummyProtocolsHandler;
    type OutEvent = (TxReqId, TxHttpRequest);

    fn new_handler(&mut self) -> Self::ProtocolsHandler {
        DummyProtocolsHandler::default()
    }

    fn addresses_of_peer(&mut self, _: &PeerId) -> Vec<Multiaddr> {
        vec![]
    }

    fn inject_connected(&mut self, _: &PeerId) {}

    fn inject_disconnected(&mut self, _: &PeerId) {}

    fn inject_event(
        &mut self,
        _: PeerId,
        _: ConnectionId,
        _: <Self::ProtocolsHandler as ProtocolsHandler>::OutEvent,
    ) {
    }

    fn poll(
        &mut self,
        cx: &mut Context,
        _: &mut impl PollParameters,
    ) -> Poll<
        NetworkBehaviourAction<
            <Self::ProtocolsHandler as ProtocolsHandler>::InEvent,
            Self::OutEvent,
        >,
    > {
        if let Poll::Ready(Some(req)) = self.recv.poll_next_unpin(cx) {
            return Poll::Ready(NetworkBehaviourAction::GenerateEvent(req));
        }

        match self.srv.poll_unpin(cx) {
            Poll::Ready(_) => {
                unreachable!();
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests;