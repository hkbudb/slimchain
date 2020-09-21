use libp2p::{multiaddr::Multiaddr, PeerId};
use serde::{de::Error as DeError, Deserialize, Deserializer, Serialize, Serializer};
use slimchain_common::error::{Error, Result};
use std::fmt;

#[derive(Debug, Clone, Deserialize)]
pub struct NetworkConfig {
    /// Listen address for node
    #[serde(default = "default_listen")]
    pub listen: String,
    /// Listen address for HTTP server (Client only)
    #[serde(default = "default_http_listen")]
    pub http_listen: String,
    /// Ed25519 key
    #[serde(default = "default_keypair")]
    pub keypair: KeypairConfig,
    /// Whether to enable mDNS
    #[serde(default = "default_mdns")]
    pub mdns: bool,
    /// Known peers
    #[serde(default = "Vec::new")]
    pub peers: Vec<PeerConfig>,
}

fn default_listen() -> String {
    "/ip4/0.0.0.0/tcp/6000".into()
}

fn default_http_listen() -> String {
    "127.0.0.1:8000".into()
}

fn default_keypair() -> KeypairConfig {
    let keypair = KeypairConfig::generate();
    println!("A keypair is generated.");
    keypair.print_config_msg(false);
    keypair
}

fn default_mdns() -> bool {
    true
}

#[derive(Clone)]
pub struct KeypairConfig(pub libp2p::identity::ed25519::Keypair);

impl KeypairConfig {
    pub fn generate() -> Self {
        Self(libp2p::identity::ed25519::Keypair::generate())
    }

    pub fn to_base58(&self) -> String {
        bs58::encode(&self.0.encode()[..]).into_string()
    }

    pub fn from_base58(input: &str) -> Result<Self> {
        let mut bin = bs58::decode(input).into_vec().map_err(Error::msg)?;
        let key =
            libp2p::identity::ed25519::Keypair::decode(bin.as_mut_slice()).map_err(Error::msg)?;
        Ok(Self(key))
    }

    pub fn to_libp2p_keypair(&self) -> libp2p::identity::Keypair {
        libp2p::identity::Keypair::Ed25519(self.0.clone())
    }

    pub fn print_config_msg(&self, toml: bool) {
        if toml {
            println!("keypair = \"{}\"", self.to_base58());
        } else {
            println!("To add the keypair in the config.toml.");
            println!();
            println!("  [network]");
            println!("  keypair = \"{}\"", self.to_base58());
            println!();
        }
    }
}

impl fmt::Debug for KeypairConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("KeypairConfig")
            .field(&self.to_base58())
            .finish()
    }
}

impl PartialEq for KeypairConfig {
    fn eq(&self, other: &Self) -> bool {
        self.0.encode() == other.0.encode()
    }
}

impl Eq for KeypairConfig {}

impl Serialize for KeypairConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.to_base58().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for KeypairConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = <String>::deserialize(deserializer)?;
        Self::from_base58(&value).map_err(DeError::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerConfig {
    #[serde(with = "peer_id_serde_impl")]
    pub peer_id: PeerId,
    #[serde(with = "multi_addr_serde_impl")]
    pub address: Multiaddr,
}

impl PeerConfig {
    pub fn new(peer_id: PeerId, address: Multiaddr) -> Self {
        Self { peer_id, address }
    }

    pub fn print_config_msg(&self) {
        println!(
            "To add the current peer in the other nodes. Add the followings to the config file."
        );
        println!();
        println!("  [[network.peers]]");
        println!("  peer_id = \"{}\"", self.peer_id.to_base58());
        println!("  address = \"{}\"", self.address.to_string());
        println!();
    }
}

mod peer_id_serde_impl {
    use super::*;

    pub fn serialize<S>(value: &PeerId, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        value.to_base58().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<PeerId, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = <String>::deserialize(deserializer)?;
        value.parse().map_err(DeError::custom)
    }
}

mod multi_addr_serde_impl {
    use super::*;

    pub fn serialize<S>(value: &Multiaddr, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        value.to_string().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Multiaddr, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = <String>::deserialize(deserializer)?;
        value.parse().map_err(DeError::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slimchain_utils::toml;

    #[test]
    fn test_keypair_config() {
        #[derive(Serialize, Deserialize)]
        struct Test {
            keypair: KeypairConfig,
        }

        let keypair = Test {
            keypair: KeypairConfig::generate(),
        };
        let toml_value = toml::to_string_pretty(&keypair).unwrap();
        let keypair2 = toml::from_str::<Test>(&toml_value).unwrap();
        assert_eq!(keypair.keypair, keypair2.keypair);
    }

    #[test]
    fn test_peer_config() {
        use libp2p::identity::Keypair;

        let peer_id = Keypair::generate_ed25519().public().into_peer_id();
        let address = "/ip4/127.0.0.1/tcp/6000".parse().unwrap();
        let peer_config = PeerConfig::new(peer_id, address);
        let toml_value = toml::to_string_pretty(&peer_config).unwrap();
        let peer_config2 = toml::from_str::<PeerConfig>(&toml_value).unwrap();
        assert_eq!(peer_config, peer_config2);
    }
}
