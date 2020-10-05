use super::*;
use rand::SeedableRng;
use slimchain_common::{ed25519::Keypair, tx_req::TxRequest};
use slimchain_utils::init_tracing_for_test;

#[tokio::test]
async fn test() {
    let _guard = init_tracing_for_test();

    let endpoint = "127.0.0.1:8000";
    let mut swarm = {
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let peer_id = keypair.public().into_peer_id();
        let transport = libp2p::build_development_transport(keypair).unwrap();
        libp2p::swarm::Swarm::new(
            transport,
            TxHttpServer::new(endpoint, || 1, || 1.into()).unwrap(),
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
    send_tx_request(endpoint, signed_tx_req.clone())
        .await
        .unwrap();
    let req = handler.await.unwrap();
    assert_eq!(req.req, signed_tx_req);

    send_record_event(endpoint, "test_event").await.unwrap();
    send_record_event_with_data(endpoint, "test_event", 42)
        .await
        .unwrap();
    send_record_event_with_data(endpoint, "test_event", &vec![1, 2, 3])
        .await
        .unwrap();
    assert_eq!(get_block_height(endpoint).await.unwrap(), 1.into());
    assert_eq!(get_tx_count(endpoint).await.unwrap(), 1);
}
