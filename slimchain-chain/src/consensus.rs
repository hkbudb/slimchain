use serde::Deserialize;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Consensus {
    PoW,
    Raft,
}
