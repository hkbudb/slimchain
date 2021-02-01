use async_trait::async_trait;
use futures::channel::oneshot;
use libp2p::{
    request_response::{
        ProtocolName, ProtocolSupport, RequestResponseConfig, RequestResponseMessage,
    },
    swarm::NetworkBehaviourEventProcess,
    Multiaddr, NetworkBehaviour, PeerId,
};
use serde::{Deserialize, Serialize};
use slimchain_common::{
    collections::HashMap,
    error::{anyhow, Result},
};
use std::{fmt, iter, time::Duration};

pub use libp2p::request_response::{
    RequestId as RpcRequestId, RequestResponse as RpcRequestResponse,
    RequestResponseEvent as RpcRequestResponseEvent, ResponseChannel as RpcResponseChannel,
};

pub mod codec;
pub use codec::RpcCodec;

const CONN_KEEP_ALIVE: Duration = Duration::from_secs(300);
const CONN_TIMEOUT: Duration = Duration::from_secs(300);

pub type RpcInstant<Req, Resp> = RpcRequestResponse<RpcCodec<Req, Resp>>;

#[derive(Debug, Clone)]
pub struct RpcProtocol {
    protocol_name: String,
}

impl RpcProtocol {
    pub fn new(protocol_name: &str) -> Self {
        Self {
            protocol_name: format!("/slimchain/rpc/1{}", protocol_name),
        }
    }
}

impl Default for RpcProtocol {
    fn default() -> Self {
        Self::new("")
    }
}

impl ProtocolName for RpcProtocol {
    fn protocol_name(&self) -> &[u8] {
        self.protocol_name.as_bytes()
    }
}

#[inline]
fn create_request_response<Req, Resp>(
    protocol_name: &str,
    protocol: ProtocolSupport,
) -> RpcInstant<Req, Resp>
where
    Req: Serialize + for<'de> Deserialize<'de> + Send + 'static,
    Resp: Serialize + for<'de> Deserialize<'de> + Send + 'static,
{
    let protocols = iter::once((RpcProtocol::new(protocol_name), protocol));
    let mut cfg = RequestResponseConfig::default();
    cfg.set_connection_keep_alive(CONN_KEEP_ALIVE);
    cfg.set_request_timeout(CONN_TIMEOUT);
    RpcRequestResponse::new(RpcCodec::default(), protocols, cfg)
}

pub fn create_request_response_server<Req, Resp>(protocol_name: &str) -> RpcInstant<Req, Resp>
where
    Req: Serialize + for<'de> Deserialize<'de> + Send + 'static,
    Resp: Serialize + for<'de> Deserialize<'de> + Send + 'static,
{
    create_request_response(protocol_name, ProtocolSupport::Inbound)
}

pub fn create_request_response_client<Req, Resp>(protocol_name: &str) -> RpcInstant<Req, Resp>
where
    Req: Serialize + for<'de> Deserialize<'de> + Send + 'static,
    Resp: Serialize + for<'de> Deserialize<'de> + Send + 'static,
{
    create_request_response(protocol_name, ProtocolSupport::Outbound)
}

pub fn handle_request_response_server_event<Req, Resp>(
    event: RpcRequestResponseEvent<Req, Resp>,
) -> Option<(Req, RpcResponseChannel<Resp>)>
where
    Req: Serialize + for<'de> Deserialize<'de> + fmt::Debug + Send + 'static,
    Resp: Serialize + for<'de> Deserialize<'de> + fmt::Debug + Send + 'static,
{
    match event {
        RpcRequestResponseEvent::Message {
            message:
                RequestResponseMessage::Request {
                    request, channel, ..
                },
            ..
        } => Some((request, channel)),
        RpcRequestResponseEvent::InboundFailure { error, .. } => {
            warn!("RpcServer inbound error: {:?}", error);
            None
        }
        RpcRequestResponseEvent::ResponseSent { request_id, .. } => {
            trace!("RpcServer response sent: request_id={:?}", request_id);
            None
        }
        event => {
            error!("RpcServer unknown event: {:?}", event);
            None
        }
    }
}

pub fn handle_request_response_client_event<Req, Resp>(
    event: RpcRequestResponseEvent<Req, Resp>,
) -> Option<(RpcRequestId, Result<Resp>)>
where
    Req: Serialize + for<'de> Deserialize<'de> + fmt::Debug + Send + 'static,
    Resp: Serialize + for<'de> Deserialize<'de> + fmt::Debug + Send + 'static,
{
    match event {
        RpcRequestResponseEvent::Message {
            message:
                RequestResponseMessage::Response {
                    request_id,
                    response,
                },
            ..
        } => Some((request_id, Ok(response))),
        RpcRequestResponseEvent::OutboundFailure {
            request_id, error, ..
        } => {
            let e = Err(anyhow!(
                "Failed to get the rpc response. Error: {:?}.",
                error
            ));
            Some((request_id, e))
        }
        event => {
            error!("RpcClient unknown event: {:?}", event);
            None
        }
    }
}

#[derive(NetworkBehaviour)]
pub struct RpcClient<Req, Resp>
where
    Req: Serialize + for<'de> Deserialize<'de> + fmt::Debug + Send + 'static,
    Resp: Serialize + for<'de> Deserialize<'de> + fmt::Debug + Send + 'static,
{
    request_response: RpcInstant<Req, Resp>,
    #[behaviour(ignore)]
    pending_responses: HashMap<RpcRequestId, oneshot::Sender<Result<Resp>>>,
}

impl<Req, Resp> RpcClient<Req, Resp>
where
    Req: Serialize + for<'de> Deserialize<'de> + fmt::Debug + Send + 'static,
    Resp: Serialize + for<'de> Deserialize<'de> + fmt::Debug + Send + 'static,
{
    pub fn new(protocol_name: &str) -> Self {
        Self {
            request_response: create_request_response_client(protocol_name),
            pending_responses: HashMap::new(),
        }
    }

    pub fn add_address(&mut self, peer: &PeerId, address: Multiaddr) {
        self.request_response.add_address(peer, address);
    }

    pub fn remove_address(&mut self, peer: &PeerId, address: &Multiaddr) {
        self.request_response.remove_address(peer, address);
    }

    pub fn send_request(&mut self, peer: &PeerId, input: Req, ret: oneshot::Sender<Result<Resp>>) {
        let id = self.request_response.send_request(peer, input);
        self.pending_responses.insert(id, ret);
    }
}

impl<Req, Resp> NetworkBehaviourEventProcess<RpcRequestResponseEvent<Req, Resp>>
    for RpcClient<Req, Resp>
where
    Req: Serialize + for<'de> Deserialize<'de> + fmt::Debug + Send + 'static,
    Resp: Serialize + for<'de> Deserialize<'de> + fmt::Debug + Send + 'static,
{
    fn inject_event(&mut self, event: RpcRequestResponseEvent<Req, Resp>) {
        let (request_id, resp) = match handle_request_response_client_event(event) {
            Some(res) => res,
            None => return,
        };
        let tx = match self.pending_responses.remove(&request_id) {
            Some(tx) => tx,
            None => return,
        };
        tx.send(resp).ok();
    }
}

#[async_trait]
impl<Req, Resp> crate::p2p::control::Shutdown for RpcClient<Req, Resp>
where
    Req: Serialize + for<'de> Deserialize<'de> + fmt::Debug + Send + 'static,
    Resp: Serialize + for<'de> Deserialize<'de> + fmt::Debug + Send + 'static,
{
    async fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
