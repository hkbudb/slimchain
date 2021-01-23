use serde::{Deserialize, Serialize};
use slimchain_common::error::{Error, Result};
use snap::{read::FrameDecoder, write::FrameEncoder};
use std::io::Cursor;

pub fn binary_encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut encoder = FrameEncoder::new(Cursor::new(Vec::new()));
    bincode::serialize_into(&mut encoder, value).map_err(Error::msg)?;
    Ok(encoder.into_inner()?.into_inner())
}

pub fn binary_decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T> {
    let decoder = FrameDecoder::new(Cursor::new(bytes));
    bincode::deserialize_from(decoder).map_err(Error::msg)
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
