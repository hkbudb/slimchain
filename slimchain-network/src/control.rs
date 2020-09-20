use async_trait::async_trait;
use futures::{
    channel::{mpsc, oneshot},
    future::poll_fn,
    prelude::*,
};
use libp2p::{
    build_tcp_ws_noise_mplex_yamux,
    identity::Keypair,
    swarm::{
        protocols_handler::ProtocolsHandler, IntoProtocolsHandler, NetworkBehaviour, Swarm,
        SwarmEvent,
    },
    Multiaddr, PeerId,
};
use slimchain_common::error::{bail, Error, Result};
use std::{pin::Pin, task::Poll};
use tokio::task::JoinHandle;

#[async_trait]
pub trait Shutdown {
    async fn shutdown(&mut self) -> Result<()>;
}

pub struct Swarmer<Behaviour>
where
    Behaviour: NetworkBehaviour + Shutdown,
    <Behaviour as NetworkBehaviour>::ProtocolsHandler: IntoProtocolsHandler + Send + 'static,
    <<Behaviour as NetworkBehaviour>::ProtocolsHandler as IntoProtocolsHandler>::Handler: ProtocolsHandler,
    <<<Behaviour as NetworkBehaviour>::ProtocolsHandler as IntoProtocolsHandler>::Handler as ProtocolsHandler>::InEvent: Clone + Send + 'static,
    <<<Behaviour as NetworkBehaviour>::ProtocolsHandler as IntoProtocolsHandler>::Handler as ProtocolsHandler>::OutEvent: Send + 'static,
{
    peer_id: PeerId,
    key_pair: Keypair,
    swarm: Swarm<Behaviour>,
}

impl<Behaviour> Swarmer<Behaviour>
where
    Behaviour: NetworkBehaviour + Shutdown,
    <Behaviour as NetworkBehaviour>::ProtocolsHandler: IntoProtocolsHandler + Send + 'static,
    <<Behaviour as NetworkBehaviour>::ProtocolsHandler as IntoProtocolsHandler>::Handler: ProtocolsHandler,
    <<<Behaviour as NetworkBehaviour>::ProtocolsHandler as IntoProtocolsHandler>::Handler as ProtocolsHandler>::InEvent: Clone + Send + 'static,
    <<<Behaviour as NetworkBehaviour>::ProtocolsHandler as IntoProtocolsHandler>::Handler as ProtocolsHandler>::OutEvent: Send + 'static,
{
    pub fn new(key_pair: Keypair, behaviour: Behaviour) -> Result<Self> {
        let peer_id = key_pair.public().into_peer_id();
        let transport = build_tcp_ws_noise_mplex_yamux(key_pair.clone())?;
        let swarm = Swarm::new(transport, behaviour, peer_id.clone());

        Ok(Self {
            peer_id,
            key_pair,
            swarm,
        })
    }

    pub async fn listen_on(&mut self, address: Multiaddr) -> Result<Multiaddr> {
        Swarm::listen_on(&mut self.swarm, address).map_err(Error::msg)?;
        let address = loop {
            match self.swarm.next_event().await {
                SwarmEvent::NewListenAddr(address) => break address,
                SwarmEvent::ListenerError { error } => {
                    bail!("Error during listen. Error: {:?}", error);
                }
                SwarmEvent::ListenerClosed { reason, .. } => {
                    bail!("Listener closed. Reason: {:?}", reason);
                }
                _ => {}
            }
        };
        info!("Peer {} listening on {}", self.peer_id, address);
        Ok(address)
    }

    pub async fn listen_on_str(&mut self, address: &str) -> Result<Multiaddr> {
        self.listen_on(address.parse()?).await
    }

    pub fn peer_id(&self) -> &PeerId {
        &self.peer_id
    }

    pub fn key_pair(&self) -> &Keypair {
        &self.key_pair
    }

    pub fn swarm(&self) -> &Swarm<Behaviour> {
        &self.swarm
    }

    pub fn swarm_mut(&mut self) -> &mut Swarm<Behaviour> {
        &mut self.swarm
    }

    pub fn spawn(self) -> Control<Behaviour> {
        let (tx, mut rx) = mpsc::channel::<ControlMsg<Behaviour>>(16);
        let (mut swarm_tx, swarm_rx) = mpsc::channel::<Swarm<Behaviour>>(1);
        let mut swarm = Some(self.swarm);
        let handler = tokio::spawn(
            poll_fn(move |cx| {
                loop {
                    let msg = match Pin::new(&mut rx).poll_next(cx) {
                        Poll::Ready(Some(msg)) => msg,
                        Poll::Ready(None) => return Poll::Ready(()),
                        Poll::Pending => break,
                    };

                    match msg {
                        ControlMsg::Shutdown => {
                            swarm_tx.start_send(swarm.take().expect("Failed to get the swarm.")).ok();
                            return Poll::Ready(())
                        },
                        ControlMsg::Call(func) => func(swarm.as_mut().expect("Failed to get the swarm.")),
                    }
                }

                loop {
                    match Pin::new(swarm.as_mut().expect("Failed to get the swarm.")).poll_next(cx) {
                        Poll::Ready(Some(_)) => {},
                        Poll::Ready(None) => return Poll::Ready(()),
                        Poll::Pending => break,
                    }
                }

                Poll::Pending
            })
        );
        Control { tx, swarm_rx, handler }
    }

    pub async fn app_run(mut self, address: &str) -> Result<()> {
        let listen_addr = self.listen_on_str(address).await?;
        let peer_cfg = crate::config::PeerConfig::new(self.peer_id().clone(), listen_addr);
        peer_cfg.print_config_msg();
        let ctrl = self.spawn();
        info!("Press Ctrl-C to quit.");
        tokio::signal::ctrl_c().await?;
        info!("Quitting.");
        ctrl.shutdown().await?;
        Ok(())
    }
}

enum ControlMsg<Behaviour>
where
    Behaviour: NetworkBehaviour,
{
    Shutdown,
    Call(Box<dyn FnOnce(&mut Swarm<Behaviour>) + Send>),
}

pub struct Control<Behaviour>
where
    Behaviour: NetworkBehaviour + Shutdown,
{
    tx: mpsc::Sender<ControlMsg<Behaviour>>,
    swarm_rx: mpsc::Receiver<Swarm<Behaviour>>,
    handler: JoinHandle<()>,
}

impl<Behaviour> Control<Behaviour>
where
    Behaviour: NetworkBehaviour + Shutdown,
{
    pub async fn shutdown(mut self) -> Result<()> {
        self.tx.send(ControlMsg::Shutdown).await?;
        self.handler.await?;
        let mut swarm = self
            .swarm_rx
            .next()
            .await
            .expect("Failed to get the swarm.");
        swarm.shutdown().await
    }

    pub async fn call<T: Send + 'static>(
        &mut self,
        func: impl FnOnce(&mut Swarm<Behaviour>) -> T + Send + 'static,
    ) -> Result<T> {
        let (tx, rx) = oneshot::channel::<T>();
        self.tx
            .send(ControlMsg::Call(Box::new(
                move |swarm: &mut Swarm<Behaviour>| {
                    let result = func(swarm);
                    tx.send(result).ok();
                },
            )))
            .await?;
        Ok(rx.await?)
    }

    pub async fn call_with_sender<T: Send + 'static>(
        &mut self,
        func: impl FnOnce(&mut Swarm<Behaviour>, oneshot::Sender<T>) + Send + 'static,
    ) -> Result<T> {
        let (tx, rx) = oneshot::channel::<T>();
        self.tx
            .send(ControlMsg::Call(Box::new(
                move |swarm: &mut Swarm<Behaviour>| {
                    func(swarm, tx);
                },
            )))
            .await?;
        Ok(rx.await?)
    }
}

pub mod prelude {
    pub use super::{Control, Shutdown, Swarmer};
    pub use libp2p::swarm::{IntoProtocolsHandler, NetworkBehaviour, ProtocolsHandler};
}
