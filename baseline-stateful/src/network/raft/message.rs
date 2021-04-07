use crate::block_proposal::BlockProposal;
use async_raft::{AppData, AppDataResponse};
use serde::{Deserialize, Serialize};
use slimchain_chain::consensus::raft::Block;
use slimchain_common::tx::TxTrait;
use std::fmt;

#[derive(Clone, Serialize, Deserialize)]
pub struct NewBlockRequest<Tx: TxTrait>(pub BlockProposal<Block, Tx>);

impl<Tx: TxTrait> fmt::Debug for NewBlockRequest<Tx> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "NewBlockRequest<Tx> (height = {})",
            self.0.get_block_height()
        )
    }
}

impl<Tx: TxTrait + Serialize + for<'de> Deserialize<'de> + 'static> AppData
    for NewBlockRequest<Tx>
{
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NewBlockResponse {
    Ok,
    Err(String),
}

impl AppDataResponse for NewBlockResponse {}
