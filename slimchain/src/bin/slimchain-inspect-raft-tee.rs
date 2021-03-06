use slimchain_common::error::Result;

use slimchain_chain::consensus::raft::Block;
use slimchain_tee_sig::TEESignedTx as Tx;

#[tokio::main]
async fn main() -> Result<()> {
    slimchain::inspect::inspect_main::<Tx, Block>().await
}
