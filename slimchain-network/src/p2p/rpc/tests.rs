use super::*;
use crate::p2p::control::{Shutdown, Swarmer};
use libp2p::identity::Keypair;
use serial_test::serial;
use slimchain_utils::init_tracing_for_test;

#[derive(NetworkBehaviour)]
struct TestServer {
    request_response: RpcInstant<String, String>,
}

impl TestServer {
    fn new() -> Self {
        Self {
            request_response: create_request_response_server("/test/1"),
        }
    }
}

impl NetworkBehaviourEventProcess<RpcRequestResponseEvent<String, String>> for TestServer {
    fn inject_event(&mut self, event: RpcRequestResponseEvent<String, String>) {
        if let Some((req, channel)) = handle_request_response_server_event(event) {
            self.request_response
                .send_response(channel, format!("Hello {}!", req))
                .unwrap();
        }
    }
}

#[async_trait]
impl Shutdown for TestServer {
    async fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

#[tokio::test]
#[serial]
async fn test() {
    let _guard = init_tracing_for_test();

    let keypair1 = Keypair::generate_ed25519();
    let keypair2 = Keypair::generate_ed25519();

    let mut swarmer1 = Swarmer::new(keypair1, TestServer::new()).await.unwrap();
    let swarmer2 = Swarmer::new(keypair2, RpcClient::<String, String>::new("/test/1"))
        .await
        .unwrap();

    let address = swarmer1
        .listen_on_str("/ip4/127.0.0.1/tcp/0")
        .await
        .unwrap();

    let peer_id1 = *swarmer1.peer_id();
    let ctrl1 = swarmer1.spawn();
    let mut ctrl2 = swarmer2.spawn();

    ctrl2
        .call(move |swarm| swarm.add_address(&peer_id1, address))
        .await
        .unwrap();

    let resp = ctrl2
        .call_with_sender(move |swarm, ret| swarm.send_request(&peer_id1, "World".to_string(), ret))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(resp.as_str(), "Hello World!");

    ctrl1.shutdown().await.unwrap();
    ctrl2.shutdown().await.unwrap();
}
