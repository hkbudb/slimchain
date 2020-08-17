use hex::{FromHex, FromHexError};
use serde::{de::Error as SerdeError, Deserialize, Deserializer};
use slimchain_common::error::{anyhow, Error, Result};
use std::{fs, path::Path};
use toml::Value as TomlValue;

pub const CONFIG_FILE_NAME: &str = "config.toml";

#[derive(Debug, Clone)]
pub struct Config(TomlValue);

impl Config {
    pub fn load(file: &Path) -> Result<Self> {
        let cfg = toml::from_str(
            &fs::read_to_string(file)
                .map_err(|e| anyhow!("Failed to open {:?}. Reason: {}.", file, e))?,
        )
        .map_err(|e| anyhow!("Failed to load {:?}. Reason: {}.", file, e))?;
        Ok(Self(cfg))
    }

    #[cfg(test)]
    pub fn load_test() -> Result<Self> {
        Self::load(&crate::path::project_root_directory()?.join(CONFIG_FILE_NAME))
    }

    pub fn load_in_the_same_dir() -> Result<Self> {
        Self::load(&crate::path::current_directory()?.join(CONFIG_FILE_NAME))
    }

    pub fn from_toml(value: TomlValue) -> Self {
        Self(value)
    }

    pub fn get<'de, T: Deserialize<'de>>(&self, key: &str) -> Result<T> {
        self.0
            .get(key)
            .ok_or_else(|| anyhow!("Failed to read `{}` in the config.", key))?
            .clone()
            .try_into()
            .map_err(Error::msg)
    }
}

pub fn deserialize_from_hex<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: FromHex<Error = FromHexError>,
{
    let encoded_hex = String::deserialize(deserializer)?;
    T::from_hex(encoded_hex).map_err(SerdeError::custom)
}
