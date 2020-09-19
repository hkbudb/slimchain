use serde::{de::Error as SerdeError, Deserialize, Deserializer};
use slimchain_common::{
    basic::ShardId,
    error::{Context as _, Result},
};
use std::fmt;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum Role {
    Client,
    Miner,
    Storage(ShardId),
}

impl<'de> Deserialize<'de> for Role {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Copy, Clone, Deserialize)]
        #[serde(rename_all = "lowercase")]
        enum RoleType {
            Client,
            Miner,
            Storage,
        }

        #[derive(Deserialize)]
        struct RoleData {
            role: RoleType,
            #[serde(default)]
            shard_id: Option<u64>,
            #[serde(default)]
            shard_total: Option<u64>,
        }

        let data = RoleData::deserialize(deserializer)?;
        match data.role {
            RoleType::Client => {
                if data.shard_id.is_some() {
                    return Err(SerdeError::custom(
                        "Field shard_id is only valid for storage node.",
                    ));
                }
                if data.shard_total.is_some() {
                    return Err(SerdeError::custom(
                        "Field shard_total is only valid for storage node.",
                    ));
                }

                Ok(Self::Client)
            }
            RoleType::Miner => {
                if data.shard_id.is_some() {
                    return Err(SerdeError::custom(
                        "Field shard_id is only valid for storage node.",
                    ));
                }
                if data.shard_total.is_some() {
                    return Err(SerdeError::custom(
                        "Field shard_total is only valid for storage node.",
                    ));
                }

                Ok(Self::Miner)
            }
            RoleType::Storage => match (data.shard_id, data.shard_total) {
                (Some(id), Some(total)) => Ok(Self::Storage(ShardId::new(id, total))),
                (None, None) => Ok(Self::Storage(ShardId::default())),
                (Some(_), None) => Err(SerdeError::missing_field("shard_total")),
                (None, Some(_)) => Err(SerdeError::missing_field("shard_id")),
            },
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client => write!(f, "Client"),
            Self::Miner => write!(f, "Miner"),
            Self::Storage(ShardId { id, total }) => write!(f, "Storage-{}-{}", id, total),
        }
    }
}

impl Role {
    pub fn to_user_agent(self) -> String {
        format!("{}", self)
    }

    pub fn from_user_agent(input: &str) -> Result<Self> {
        match input {
            "Client" => return Ok(Self::Client),
            "Miner" => return Ok(Self::Miner),
            _ => {}
        }

        let rest = input
            .strip_prefix("Storage-")
            .context("Unknown User Agent.")?;
        let mut shard_info = rest.splitn(2, '-');
        let id = shard_info.next().context("Unknown User Agent.")?.parse()?;
        let total = shard_info.next().context("Unknown User Agent.")?.parse()?;
        Ok(Self::Storage(ShardId::new(id, total)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize() {
        use slimchain_utils::{config::Config, toml};

        let input = toml::toml! {
            [role]
            role = "client"
        };
        assert_eq!(Role::Client, Config::from_toml(input).get("role").unwrap());

        let input = toml::toml! {
            [role]
            role = "miner"
        };
        assert_eq!(Role::Miner, Config::from_toml(input).get("role").unwrap());

        let input = toml::toml! {
            [role]
            role = "storage"
        };
        assert_eq!(
            Role::Storage(ShardId::default()),
            Config::from_toml(input).get("role").unwrap()
        );

        let input = toml::toml! {
            [role]
            role = "storage"
            shard_id = 1
            shard_total = 2
        };
        assert_eq!(
            Role::Storage(ShardId::new(1, 2)),
            Config::from_toml(input).get("role").unwrap()
        );

        let input = toml::toml! {
            [role]
            role = "client"
            shard_id = 1
            shard_total = 2
        };
        assert!(Config::from_toml(input).get::<Role>("role").is_err());

        let input = toml::toml! {
            [role]
            role = "storage"
            shard_id = 1
        };
        assert!(Config::from_toml(input).get::<Role>("role").is_err());

        let input = toml::toml! {
            [role]
            role = "storage"
            shard_total = 2
        };
        assert!(Config::from_toml(input).get::<Role>("role").is_err());
    }

    #[test]
    fn test_user_agent() {
        let role = Role::Client;
        assert_eq!(role, Role::from_user_agent(&role.to_user_agent()).unwrap());
        let role = Role::Miner;
        assert_eq!(role, Role::from_user_agent(&role.to_user_agent()).unwrap());
        let role = Role::Storage(ShardId::default());
        assert_eq!(role, Role::from_user_agent(&role.to_user_agent()).unwrap());
        assert!(Role::from_user_agent("").is_err());
        assert!(Role::from_user_agent("foo").is_err());
        assert!(Role::from_user_agent("Storage").is_err());
        assert!(Role::from_user_agent("Storage-").is_err());
        assert!(Role::from_user_agent("Storage-1").is_err());
        assert!(Role::from_user_agent("Storage-1-").is_err());
        assert!(Role::from_user_agent("Storage-a-b").is_err());
        assert!(Role::from_user_agent("Storage-1-2-3").is_err());
    }
}
