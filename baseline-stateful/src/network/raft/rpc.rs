use crate::block_proposal::BlockProposal;
use serde::Deserialize;
use slimchain_chain::consensus::raft::Block;
use slimchain_common::{basic::BlockHeight, error::Result, tx::TxTrait};
use slimchain_network::http::{common::*, node_rpc::NODE_RPC_ROUTE_PATH};

pub const NODE_BLOCK_PROPOSAL_ROUTE_PATH: &str = "block_proposal";

pub async fn get_block_proposal<Tx: TxTrait + for<'de> Deserialize<'de>>(
    endpoint: &str,
    height: BlockHeight,
) -> Result<BlockProposal<Block, Tx>> {
    send_post_request_using_binary(
        &format!(
            "http://{}/{}/{}",
            endpoint, NODE_RPC_ROUTE_PATH, NODE_BLOCK_PROPOSAL_ROUTE_PATH
        ),
        &height,
    )
    .await
}
