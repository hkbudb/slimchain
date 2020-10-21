use super::*;
use crate::control::{Control, Shutdown, Swarmer};
use futures::channel::oneshot;
use libp2p::identity::Keypair;
use serial_test::serial;
use slimchain_common::basic::ShardId;
use slimchain_utils::init_tracing_for_test;
use std::ops::{Deref, DerefMut};

#[derive(NetworkBehaviour)]
struct DiscoveryTest {
    discv: Discovery,
}

impl DiscoveryTest {
    fn new(pk: PublicKey, role: Role, enable_mdns: bool) -> Result<Self> {
        let discv = Discovery::new(pk, role, enable_mdns)?;
        Ok(Self { discv })
    }

    fn try_find_peer(
        &mut self,
        role: Role,
        timeout: Duration,
        ret: oneshot::Sender<Result<PeerId>>,
    ) {
        self.discv.find_random_peer_with_ret(role, timeout, ret);
    }
}

impl NetworkBehaviourEventProcess<DiscoveryEvent> for DiscoveryTest {
    fn inject_event(&mut self, _: DiscoveryEvent) {}
}

#[async_trait::async_trait]
impl Shutdown for DiscoveryTest {
    async fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

impl Deref for DiscoveryTest {
    type Target = Discovery;

    fn deref(&self) -> &Self::Target {
        &self.discv
    }
}

impl DerefMut for DiscoveryTest {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.discv
    }
}

async fn create_node(mdns: bool, role: Role) -> (PeerId, Multiaddr, Control<DiscoveryTest>) {
    let keypair = Keypair::generate_ed25519();
    let mut swarmer = Swarmer::new(
        keypair.clone(),
        DiscoveryTest::new(keypair.public(), role, mdns).unwrap(),
    )
    .unwrap();
    let address = swarmer.listen_on_str("/ip4/127.0.0.1/tcp/0").await.unwrap();
    let ctrl = swarmer.spawn();
    (keypair.public().into_peer_id(), address, ctrl)
}

#[tokio::test]
#[serial]
async fn test_with_mdns() {
    let _guard = init_tracing_for_test();

    let (_peer1, _addr1, mut ctrl1) = create_node(true, Role::Client).await;
    let (peer2, _addr2, ctrl2) = create_node(true, Role::Miner).await;
    let (peer3, _addr3, ctrl3) = create_node(true, Role::Storage(ShardId::new(0, 1))).await;
    let (peer4, _addr4, ctrl4) = create_node(true, Role::Storage(ShardId::new(0, 1))).await;

    let res = ctrl1
        .call_with_sender(|swarm, ret| {
            swarm.try_find_peer(Role::Client, Duration::from_millis(100), ret)
        })
        .await
        .unwrap();
    assert!(res.is_err());

    let res = ctrl1
        .call_with_sender(|swarm, ret| {
            swarm.try_find_peer(Role::Miner, Duration::from_secs(5), ret)
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(peer2, res);
    assert_eq!(
        Some(peer2),
        ctrl1
            .call(|swarm| swarm.random_known_peer(&Role::Miner))
            .await
            .unwrap()
    );

    let res = ctrl1
        .call_with_sender(|swarm, ret| {
            swarm.try_find_peer(
                Role::Storage(ShardId::new(0, 1)),
                Duration::from_secs(5),
                ret,
            )
        })
        .await
        .unwrap()
        .unwrap();
    assert!(res == peer3 || res == peer4);

    let res = ctrl1
        .call(|swarm| swarm.random_known_peers(&Role::Storage(ShardId::new(0, 1)), 1))
        .await
        .unwrap();
    assert!(res[0] == peer3 || res[0] == peer4);

    ctrl1.shutdown().await.unwrap();
    ctrl2.shutdown().await.unwrap();
    ctrl3.shutdown().await.unwrap();
    ctrl4.shutdown().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_without_mdns() {
    let _guard = init_tracing_for_test();

    let (peer0, addr0, ctrl0) = create_node(false, Role::Client).await;
    let (_peer1, _addr1, mut ctrl1) = create_node(false, Role::Client).await;
    let (peer2, _addr2, mut ctrl2) = create_node(false, Role::Miner).await;
    let (peer3, _addr3, mut ctrl3) = create_node(false, Role::Storage(ShardId::new(0, 1))).await;
    let (peer4, _addr4, mut ctrl4) = create_node(false, Role::Storage(ShardId::new(0, 1))).await;

    let peer = peer0.clone();
    let addr = addr0.clone();
    ctrl1
        .call(move |swarm| swarm.add_address(&peer, addr))
        .await
        .unwrap();
    let peer = peer0.clone();
    let addr = addr0.clone();
    ctrl2
        .call(move |swarm| swarm.add_address(&peer, addr))
        .await
        .unwrap();
    let peer = peer0.clone();
    let addr = addr0.clone();
    ctrl3
        .call(move |swarm| swarm.add_address(&peer, addr))
        .await
        .unwrap();
    let peer = peer0.clone();
    let addr = addr0.clone();
    ctrl4
        .call(move |swarm| swarm.add_address(&peer, addr))
        .await
        .unwrap();

    let res = ctrl1
        .call_with_sender(|swarm, ret| {
            swarm.try_find_peer(Role::Miner, Duration::from_secs(5), ret)
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(peer2, res);
    assert_eq!(
        Some(peer2),
        ctrl1
            .call(|swarm| swarm.random_known_peer(&Role::Miner))
            .await
            .unwrap()
    );

    let res = ctrl1
        .call_with_sender(|swarm, ret| {
            swarm.try_find_peer(
                Role::Storage(ShardId::new(0, 1)),
                Duration::from_secs(5),
                ret,
            )
        })
        .await
        .unwrap()
        .unwrap();
    assert!(res == peer3 || res == peer4);

    let res = ctrl1
        .call_with_sender(|swarm, ret| {
            swarm.try_find_peer(
                Role::Storage(ShardId::new(0, 1)),
                Duration::from_secs(5),
                ret,
            )
        })
        .await
        .unwrap()
        .unwrap();
    assert!(res == peer3 || res == peer4);

    let res = ctrl1
        .call(|swarm| swarm.random_known_peers(&Role::Storage(ShardId::new(0, 1)), 1))
        .await
        .unwrap();
    assert!(res[0] == peer3 || res[0] == peer4);

    ctrl0.shutdown().await.unwrap();
    ctrl1.shutdown().await.unwrap();
    ctrl2.shutdown().await.unwrap();
    ctrl3.shutdown().await.unwrap();
    ctrl4.shutdown().await.unwrap();
}
