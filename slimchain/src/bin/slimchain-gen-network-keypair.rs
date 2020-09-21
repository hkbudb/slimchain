use slimchain_network::config::KeypairConfig;

fn main() {
    let keypair = KeypairConfig::generate();
    keypair.print_config_msg();
}
