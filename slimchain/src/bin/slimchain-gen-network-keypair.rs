use slimchain_network::p2p::config::KeypairConfig;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(version = git_version::git_version!(prefix = concat!(env!("CARGO_PKG_VERSION"), " ("), suffix = ")", fallback = "unknown"))]
struct Opts {
    /// Output in toml format.
    #[structopt(short, long)]
    toml: bool,
}

fn main() {
    let opts = Opts::from_args();

    let keypair = KeypairConfig::generate();
    keypair.print_config_msg(opts.toml);
}
