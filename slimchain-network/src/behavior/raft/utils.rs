use crate::http::config::PeerId;
use async_raft::{AppData, AppDataResponse, Raft, RaftNetwork, RaftStorage};
use slimchain_common::error::{anyhow, Result};

pub async fn get_current_leader<D, R, N, S>(raft: &Raft<D, R, N, S>) -> Result<PeerId>
where
    D: AppData,
    R: AppDataResponse,
    N: RaftNetwork<D>,
    S: RaftStorage<D, R>,
{
    raft.metrics()
        .recv()
        .await
        .and_then(|m| m.current_leader)
        .map(PeerId::from)
        .ok_or_else(|| anyhow!("Leader unknown"))
}
