use serde::{Deserialize, Serialize};
use slimchain_common::error::{Error, Result};

pub fn binary_encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let data = postcard::to_allocvec(value).map_err(Error::msg)?;
    Ok(data)
}

pub fn binary_decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T> {
    let value = postcard::from_bytes(bytes).map_err(Error::msg)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let value = String::from("hello world");
        let bin = binary_encode(&value).unwrap();
        assert_eq!(binary_decode::<String>(bin.as_ref()).unwrap(), value);
    }
}
