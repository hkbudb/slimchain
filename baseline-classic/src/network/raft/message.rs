use crate::block::{raft::Block, BlockTrait};
use async_raft::{AppData, AppDataResponse};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Serialize, Deserialize)]
pub struct NewBlockRequest(pub Block);

impl AppData for NewBlockRequest {}

impl fmt::Debug for NewBlockRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NewBlockRequest (height = {})", self.0.block_height())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NewBlockResponse {
    Ok,
    Err(String),
}

impl AppDataResponse for NewBlockResponse {}
