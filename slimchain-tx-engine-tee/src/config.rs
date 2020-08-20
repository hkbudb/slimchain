use serde::Deserialize;

#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize)]
pub struct TEEConfig {
    /// Subscription Key that provides access to the Intel API
    pub api_key: String,
    /// Service Provider ID (SPID)
    #[serde(deserialize_with = "slimchain_utils::config::deserialize_from_hex")]
    pub spid: Vec<u8>,
    /// Whether to sign linkable quote
    pub linkable: bool,
}
