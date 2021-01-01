use crate::http::config::PeerId;
use async_raft::{
    error::ClientReadError, AppData, AppDataResponse, Raft, RaftNetwork, RaftStorage,
};
use slimchain_common::error::{bail, Error, Result};
use tokio::sync::RwLock;

pub async fn get_current_leader<D, R, N, S>(
    node_peer_id: PeerId,
    raft: &Raft<D, R, N, S>,
    lock: &RwLock<()>,
) -> Result<PeerId>
where
    D: AppData,
    R: AppDataResponse,
    N: RaftNetwork<D>,
    S: RaftStorage<D, R>,
{
    let _guard = lock.read().await;
    match raft.client_read().await {
        Ok(()) => Ok(node_peer_id),
        Err(ClientReadError::ForwardToLeader(Some(id))) => Ok(PeerId::from(id)),
        Err(ClientReadError::ForwardToLeader(None)) => bail!("Leader unknown"),
        Err(ClientReadError::RaftError(e)) => Err(Error::msg(e)),
    }
}
