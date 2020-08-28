use serde::Deserialize;

pub mod pow;
pub mod raft;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Consensus {
    PoW,
    Raft,
}
