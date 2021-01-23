use crate::block::raft::Block;
use slimchain_common::{basic::BlockHeight, error::Result};
use slimchain_network::http::{common::*, node_rpc::NODE_RPC_ROUTE_PATH};

pub const NODE_BLOCK_ROUTE_PATH: &str = "block";

pub async fn get_block(endpoint: &str, height: BlockHeight) -> Result<Block> {
    send_post_request_using_binary(
        &format!(
            "http://{}/{}/{}",
            endpoint, NODE_RPC_ROUTE_PATH, NODE_BLOCK_ROUTE_PATH
        ),
        &height,
    )
    .await
}
