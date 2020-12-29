use super::{common::*, config::PeerId};
use serde::Serialize;
use slimchain_common::error::Result;

pub const NODE_RPC_ROUTE_PATH: &str = "node_rpc";

pub const RAFT_APPEND_ENTRIES_ROUTE_PATH: &str = "raft_append_entries";
pub const RAFT_INSTALL_SNAPSHOT_ROUTE_PATH: &str = "raft_install_snapshot";
pub const RAFT_VOTE_ROUTE_PATH: &str = "raft_vote";

pub const STORAGE_BLOCK_IMPORT_ROUTE_PATH: &str = "storage_block_import";
pub const STORAGE_TX_REQ_ROUTE_PATH: &str = "storage_tx_req";

pub const CLIENT_LEADER_ID_ROUTE_PATH: &str = "leader_id";
pub const CLIENT_LEADER_REQ_ROUTE_PATH: &str = "leader_req";

pub async fn get_leader(endpoint: &str) -> Result<PeerId> {
    send_get_request_using_postcard(&format!(
        "http://{}/{}/{}",
        endpoint, NODE_RPC_ROUTE_PATH, CLIENT_LEADER_ID_ROUTE_PATH
    ))
    .await
}

#[allow(clippy::ptr_arg)]
pub async fn send_reqs_to_leader<Req: Serialize>(endpoint: &str, reqs: &Vec<Req>) -> Result<()> {
    send_post_request_using_postcard(
        &format!(
            "http://{}/{}/{}",
            endpoint, NODE_RPC_ROUTE_PATH, CLIENT_LEADER_REQ_ROUTE_PATH,
        ),
        reqs,
    )
    .await
}
