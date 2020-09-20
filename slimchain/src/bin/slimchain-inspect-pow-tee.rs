use slimchain_common::error::Result;

use slimchain_tee_sig::TEESignedTx as Tx;
use slimchain_chain::consensus::pow::Block;

#[tokio::main]
async fn main() -> Result<()> {
    slimchain::inspect::inspect_main::<Tx, Block>().await
}
