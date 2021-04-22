use slimchain_chain::db::{BLOCK_DB_COL, DB, LOG_DB_COL, META_DB_COL, STATE_DB_COL, TX_DB_COL};
use slimchain_common::{
    basic::BlockHeight,
    error::{bail, Context as _, Result},
};
use slimchain_utils::init_tracing_subscriber;
use std::{fs, path::PathBuf};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(version = git_version::git_version!(prefix = concat!(env!("CARGO_PKG_VERSION"), " ("), suffix = ")", fallback = "unknown"))]
struct Opts {
    /// Path to storage.db
    #[structopt(short, long, parse(from_os_str))]
    db_path: PathBuf,

    /// Path to output.json
    #[structopt(short, long, parse(from_os_str))]
    out: Option<PathBuf>,

    /// Set trace log level. Default: no tracing.
    #[structopt(long)]
    log_level: Option<String>,
}

fn main() -> Result<()> {
    color_backtrace::install();

    let opts = Opts::from_args();

    if let Some(log_level) = opts.log_level.as_deref() {
        init_tracing_subscriber(log_level)?;
    }

    if !opts.db_path.exists() {
        bail!("DB {:?} not existed.", opts.db_path);
    }
    let db = DB::open_or_create(&opts.db_path, false)?;

    let height: BlockHeight = db
        .get_existing_meta_object("height")
        .context("Failed to get block height from the database.")?;
    let meta_db_size = db.get_table_size(META_DB_COL);
    let log_db_size = db.get_table_size(LOG_DB_COL);
    let block_db_size = db.get_table_size(BLOCK_DB_COL);
    let tx_db_size = db.get_table_size(TX_DB_COL);
    let state_db_size = db.get_table_size(STATE_DB_COL);
    let chain_db_size = block_db_size + tx_db_size + state_db_size;

    println!("Database size breakdown:");
    println!(" Height = {}", height);
    println!(" META = {}", meta_db_size);
    println!(" RAFT_LOG = {}", log_db_size);
    println!(
        " BLOCK = {} ({} per block)",
        block_db_size,
        block_db_size as f64 / height.0 as f64
    );
    println!(
        " TX = {} ({} per block)",
        tx_db_size,
        tx_db_size as f64 / height.0 as f64
    );
    println!(
        " STATE = {} ({} per block)",
        state_db_size,
        state_db_size as f64 / height.0 as f64
    );
    println!(
        " BLOCK + TX + STATE = {} ({} per block)",
        chain_db_size,
        chain_db_size as f64 / height.0 as f64
    );

    if let Some(out_path) = opts.out.as_ref() {
        let out_data = serde_json::json!({
            "height": height.0,
            "meta_db_size": meta_db_size,
            "log_db_size": log_db_size,
            "block_db_size": block_db_size,
            "tx_db_size": tx_db_size,
            "state_db_size": state_db_size,
            "chain_db_size": chain_db_size,
        });

        fs::write(out_path, serde_json::to_string_pretty(&out_data)?)?;
    }

    Ok(())
}
