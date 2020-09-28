use futures::{channel::mpsc, future::BoxFuture, prelude::*, stream};
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
    tx_req::SignedTxRequest,
};
use std::{
    iter,
    net::SocketAddr,
    task::{Context, Poll},
};
use warp::Filter;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TxHttpRequest {
    pub req: SignedTxRequest,
    #[serde(default)]
    pub shard_id: ShardId,
}

const TX_REQ_ROUTE_PATH: &str = "tx_req";

pub async fn send_tx_request(endpoint: &str, req: SignedTxRequest) -> Result<()> {
    send_tx_requests(endpoint, iter::once(req)).await
}

pub async fn send_tx_requests(
    endpoint: &str,
    reqs: impl Iterator<Item = SignedTxRequest>,
) -> Result<()> {
    let reqs = reqs.into_iter().map(|req| (req, ShardId::default()));
    send_tx_requests_with_shard(endpoint, reqs).await
}

pub async fn send_tx_requests_with_shard(
    endpoint: &str,
    reqs: impl Iterator<Item = (SignedTxRequest, ShardId)>,
) -> Result<()> {
    let reqs: Vec<_> = reqs
        .into_iter()
        .map(|(req, shard_id)| TxHttpRequest { req, shard_id })
        .collect();

    let mut resp = surf::post(&format!("http://{}/{}", endpoint, TX_REQ_ROUTE_PATH))
        .body(surf::Body::from_json(&reqs).map_err(Error::msg)?)
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
    recv: mpsc::Receiver<TxHttpRequest>,
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
            .and_then(move |reqs: Vec<TxHttpRequest>| {
                let mut tx = tx.clone();
                async move {
                    let mut reqs = stream::iter(reqs).map(Ok);
                    match tx.send_all(&mut reqs).await {
                        Ok(_) => Ok(warp::reply::json(&())),
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
    type OutEvent = TxHttpRequest;

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
