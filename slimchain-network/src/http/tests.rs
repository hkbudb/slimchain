use super::*;
use rand::SeedableRng;
use slimchain_common::{ed25519::Keypair, tx_req::TxRequest};
use slimchain_utils::init_tracing_for_test;

#[tokio::test]
async fn test() {
    let _guard = init_tracing_for_test();

    let mut swarm = {
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let peer_id = keypair.public().into_peer_id();
        let transport = libp2p::build_development_transport(keypair).unwrap();
        libp2p::swarm::Swarm::new(
            transport,
            TxHttpServer::new("127.0.0.1:8000").unwrap(),
            peer_id,
        )
    };

    let signed_tx_req = {
        let mut rng = rand::rngs::StdRng::seed_from_u64(1u64);
        let keypair = Keypair::generate(&mut rng);
        let tx_req = TxRequest::Create {
            nonce: Default::default(),
            code: Default::default(),
        };
        tx_req.sign(&keypair)
    };

    let handler = tokio::spawn(async move { swarm.next().await });
    tokio::time::delay_for(tokio::time::Duration::from_millis(300)).await;
    send_tx_request("127.0.0.1:8000", signed_tx_req.clone())
        .await
        .unwrap();
    let req = handler.await.unwrap();
    assert_eq!(req.req, signed_tx_req);
}
