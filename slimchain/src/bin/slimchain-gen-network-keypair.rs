use slimchain_network::p2p::config::KeypairConfig;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
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
