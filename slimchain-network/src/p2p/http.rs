use crate::http::client_rpc::client_rpc_server;
use futures::{channel::mpsc, future::BoxFuture, prelude::*, stream};
use libp2p::{
    core::connection::ConnectionId,
    swarm::{
        protocols_handler::DummyProtocolsHandler, NetworkBehaviour, NetworkBehaviourAction,
        PollParameters, ProtocolsHandler,
    },
    Multiaddr, PeerId,
};

use slimchain_common::{
    basic::BlockHeight,
    error::{Error, Result},
};
use std::{
    net::SocketAddr,
    task::{Context, Poll},
};

pub use crate::http::client_rpc::TxHttpRequest;

pub struct ClientHttpServer {
    srv: BoxFuture<'static, ()>,
    recv: mpsc::Receiver<TxHttpRequest>,
}

impl ClientHttpServer {
    pub fn new(
        endpoint: &str,
        tx_count_fn: impl Fn() -> usize + Send + Sync + 'static,
        block_height_fn: impl Fn() -> BlockHeight + Send + Sync + 'static,
    ) -> Result<Self> {
        info!("Create tx http server, listen on {}", endpoint);
        let listen_addr: SocketAddr = endpoint.parse()?;
        let (tx, rx) = mpsc::channel(1024);
        let tx_req_fn = move |reqs: Vec<TxHttpRequest>| {
            let mut tx = tx.clone();
            let mut reqs = stream::iter(reqs).map(Ok);
            async move { tx.send_all(&mut reqs).await.map_err(Error::msg) }
        };
        let route = client_rpc_server(tx_req_fn, tx_count_fn, block_height_fn);
        let srv = warp::serve(route).bind(listen_addr).boxed();
        Ok(Self { srv, recv: rx })
    }
}

impl NetworkBehaviour for ClientHttpServer {
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
