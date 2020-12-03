use serde::Deserialize;
use slimchain_chain::{
    block::BlockTrait,
    db::DB,
    loader::{BlockLoaderTrait, TxLoaderTrait},
};
use slimchain_common::{
    basic::BlockHeight,
    error::{bail, Context as _, Result},
    tx::TxTrait,
};
use slimchain_utils::init_tracing_subscriber;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
struct Opts {
    /// Path to storage.db
    #[structopt(short, long, parse(from_os_str))]
    db_path: PathBuf,

    /// Start Block
    #[structopt(short, long, default_value = "0")]
    start: BlockHeight,

    /// End block
    #[structopt(short, long)]
    end: Option<BlockHeight>,

    /// Show write set
    #[structopt(short, long)]
    write_set: bool,

    /// Set trace log level. Default: no tracing.
    #[structopt(long)]
    log_level: Option<String>,
}

pub async fn inspect_main<Tx, Block>() -> Result<()>
where
    Tx: TxTrait + for<'de> Deserialize<'de> + 'static,
    Block: BlockTrait + for<'de> Deserialize<'de> + 'static,
{
    color_backtrace::install();

    let opts = Opts::from_args();

    if let Some(log_level) = opts.log_level.as_deref() {
        init_tracing_subscriber(log_level)?;
    }

    if !opts.db_path.exists() {
        bail!("DB {:?} not existed.", opts.db_path);
    }
    let db = DB::open_or_create(&opts.db_path)?;

    let start = opts.start;
    let end = match opts.end {
        Some(end) => end,
        None => db
            .get_existing_meta_object("height")
            .context("Failed to get block height from the database.")?,
    };

    let mut height = start;
    while height <= end {
        if height.is_zero() {
            height = height.next_height();
            continue;
        }

        let block: Block = db.get_block(height)?;
        println!(
            "Block #{} [#tx={}, state_root={}]",
            height,
            block.tx_list().len(),
            block.state_root()
        );
        for &tx_hash in block.tx_list().iter() {
            let tx: Result<Tx> = db.get_tx(tx_hash);
            if let Ok(tx) = tx {
                println!(" TX {} exec_height = {}", tx_hash, tx.tx_block_height());
                if opts.write_set {
                    println!("   write_set = {:#?}", tx.tx_writes());
                }
            } else {
                println!(" TX {} (not available)", tx_hash);
            }
        }

        height = height.next_height();
    }

    Ok(())
}
