use slimchain_common::error::Result;

use slimchain_chain::consensus::pow::Block;
use slimchain_common::tx::SignedTx as Tx;

#[tokio::main]
async fn main() -> Result<()> {
    slimchain::inspect::inspect_main::<Tx, Block>().await
}
