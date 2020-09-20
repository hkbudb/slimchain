use slimchain_common::error::Result;

use slimchain_common::tx::SignedTx as Tx;
use slimchain_chain::consensus::pow::Block;

#[tokio::main]
async fn main() -> Result<()> {
    slimchain::inspect::inspect_main::<Tx, Block>().await
}
