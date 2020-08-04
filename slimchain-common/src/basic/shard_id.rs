use crate::basic::Address;
use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct ShardId {
    pub id: u64,
    pub total: u64,
}

impl Default for ShardId {
    fn default() -> Self {
        Self { id: 0, total: 1 }
    }
}

impl ShardId {
    pub fn new(id: u64, total: u64) -> Self {
        Self { id, total }
    }

    pub fn contains(&self, addr: Address) -> bool {
        addr.to_low_u64_be() % self.total == self.id
    }

    pub fn is_full_shard(&self) -> bool {
        self.id == 0 && self.total == 1
    }

    pub fn find_remote_shard(
        addr: Address,
        total_values: impl Iterator<Item = u64>,
    ) -> impl Iterator<Item = ShardId> {
        let addr_low_u64 = addr.to_low_u64_be();
        total_values.map(move |total| {
            let id = addr_low_u64 % total;
            Self { id, total }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basic::H160;

    #[test]
    fn test_shard_id() {
        let shard_id = ShardId::new(1, 2);
        assert!(shard_id.contains(H160::repeat_byte(0xff).into()));
        assert!(!shard_id.contains(H160::repeat_byte(0x00).into()));
    }
}
