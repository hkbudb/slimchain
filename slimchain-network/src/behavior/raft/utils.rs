use crate::http::config::PeerId;
use async_raft::{
    error::ClientReadError, AppData, AppDataResponse, Raft, RaftNetwork, RaftStorage,
};
use slimchain_common::error::{bail, Error, Result};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[derive(Debug, Clone, Copy)]
pub struct LeaderPeerIdCache {
    inner: Option<(PeerId, Instant)>,
}

impl LeaderPeerIdCache {
    pub fn new() -> Self {
        Self { inner: None }
    }

    pub fn get(&mut self) -> Option<PeerId> {
        match self.inner {
            Some((id, deadline)) => {
                if Instant::now() > deadline {
                    self.inner = None;
                    None
                } else {
                    Some(id)
                }
            }
            None => None,
        }
    }

    pub fn set(&mut self, id: PeerId, ttl: Duration) {
        self.inner = Some((id, Instant::now() + ttl));
    }

    pub fn reset(&mut self) {
        self.inner = None;
    }
}

pub async fn get_current_leader<D, R, N, S>(
    node_peer_id: PeerId,
    raft: &Raft<D, R, N, S>,
    lock: &Mutex<()>,
) -> Result<PeerId>
where
    D: AppData,
    R: AppDataResponse,
    N: RaftNetwork<D>,
    S: RaftStorage<D, R>,
{
    let _guard = lock.lock().await;
    match raft.client_read().await {
        Ok(()) => Ok(node_peer_id),
        Err(ClientReadError::ForwardToLeader(Some(id))) => Ok(PeerId::from(id)),
        Err(ClientReadError::ForwardToLeader(None)) => bail!("Leader unknown"),
        Err(ClientReadError::RaftError(e)) => Err(Error::msg(e)),
    }
}
