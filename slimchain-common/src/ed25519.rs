use crate::{
    basic::H256,
    digest::{blake2b_hash_to_h256, default_blake2, Digestible},
    error::{Error, Result},
};
use serde::{de::Error as DeError, Deserialize, Deserializer, Serialize, Serializer};

pub use ed25519_dalek;
pub use ed25519_dalek::{Keypair, PublicKey, SecretKey, Signature, Signer, Verifier};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PubSigPair {
    #[serde(with = "crate::ed25519::pk_serde_impl")]
    pub pk: PublicKey,
    pub sig: Signature,
}

impl PubSigPair {
    pub fn create(keypair: &Keypair, msg_hash: H256) -> Self {
        Self {
            pk: keypair.public,
            sig: keypair.sign(msg_hash.as_bytes()),
        }
    }

    pub fn verify(&self, msg_hash: H256) -> Result<()> {
        self.pk
            .verify(msg_hash.as_bytes(), &self.sig)
            .map_err(Error::msg)
    }

    pub fn public(&self) -> &PublicKey {
        &self.pk
    }

    pub fn signature(&self) -> &Signature {
        &self.sig
    }
}

impl Digestible for PubSigPair {
    fn to_digest(&self) -> H256 {
        let mut hash_state = default_blake2().to_state();
        hash_state.update(&self.pk.to_bytes()[..]);
        hash_state.update(&self.sig.to_bytes()[..]);
        let hash = hash_state.finalize();
        blake2b_hash_to_h256(hash)
    }
}

pub mod keypair_serde_impl {
    use super::*;

    #[derive(Serialize, Deserialize)]
    #[serde(remote = "Keypair")]
    struct KeypairDef {
        #[serde(with = "crate::ed25519::sk_serde_impl")]
        secret: SecretKey,
        #[serde(with = "crate::ed25519::pk_serde_impl")]
        public: PublicKey,
    }

    pub fn serialize<S>(value: &Keypair, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        KeypairDef::serialize(value, serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> core::result::Result<Keypair, D::Error>
    where
        D: Deserializer<'de>,
    {
        KeypairDef::deserialize(deserializer)
    }
}

pub mod pk_serde_impl {
    use super::*;
    use ed25519_dalek::PUBLIC_KEY_LENGTH;

    pub fn serialize<S>(value: &PublicKey, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let bytes: [u8; PUBLIC_KEY_LENGTH] = value.to_bytes();
        bytes.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> core::result::Result<PublicKey, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes = <[u8; PUBLIC_KEY_LENGTH]>::deserialize(deserializer)?;
        PublicKey::from_bytes(&bytes[..]).map_err(DeError::custom)
    }
}

pub mod sk_serde_impl {
    use super::*;
    use ed25519_dalek::SECRET_KEY_LENGTH;

    pub fn serialize<S>(value: &SecretKey, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let bytes: [u8; SECRET_KEY_LENGTH] = value.to_bytes();
        bytes.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> core::result::Result<SecretKey, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes = <[u8; SECRET_KEY_LENGTH]>::deserialize(deserializer)?;
        SecretKey::from_bytes(&bytes[..]).map_err(DeError::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serde_keypair() {
        #[derive(Serialize, Deserialize)]
        struct Key(#[serde(with = "crate::ed25519::keypair_serde_impl")] Keypair);

        let mut rng = rand::thread_rng();
        let keypair = Key(Keypair::generate(&mut rng));
        let bin = postcard::to_allocvec(&keypair).unwrap();
        let keypair2: Key = postcard::from_bytes(&bin[..]).unwrap();
        assert_eq!(&keypair.0.to_bytes()[..], &keypair2.0.to_bytes()[..]);
    }

    #[test]
    fn test_serde_pk() {
        #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
        struct PubKey(#[serde(with = "crate::ed25519::pk_serde_impl")] PublicKey);

        let mut rng = rand::thread_rng();
        let keypair = Keypair::generate(&mut rng);
        let pk = PubKey(keypair.public);
        let bin = postcard::to_allocvec(&pk).unwrap();
        assert_eq!(postcard::from_bytes::<PubKey>(&bin[..]).unwrap(), pk);
    }

    #[test]
    fn test_serde_sk() {
        #[derive(Serialize, Deserialize)]
        struct SecKey(#[serde(with = "crate::ed25519::sk_serde_impl")] SecretKey);

        let mut rng = rand::thread_rng();
        let keypair = Keypair::generate(&mut rng);
        let sk = SecKey(keypair.secret);
        let bin = postcard::to_allocvec(&sk).unwrap();
        let sk2: SecKey = postcard::from_bytes(&bin[..]).unwrap();
        assert_eq!(&sk.0.to_bytes()[..], &sk2.0.to_bytes()[..]);
    }

    #[test]
    fn test_serde_sig() {
        let mut rng = rand::thread_rng();
        let keypair = Keypair::generate(&mut rng);
        let sig = keypair.sign(b"hello");
        let bin = postcard::to_allocvec(&sig).unwrap();
        assert_eq!(postcard::from_bytes::<Signature>(&bin[..]).unwrap(), sig);
    }

    #[test]
    fn test_serde_pk_sig() {
        let mut rng = rand::thread_rng();
        let keypair = Keypair::generate(&mut rng);
        let pk_sig = PubSigPair::create(&keypair, H256::zero());
        let bin = postcard::to_allocvec(&pk_sig).unwrap();
        assert_eq!(
            postcard::from_bytes::<PubSigPair>(&bin[..]).unwrap(),
            pk_sig
        );
    }

    #[test]
    fn test_sign_and_verify() {
        let mut rng = rand::thread_rng();
        let hash = H256::repeat_byte(0x12);
        let keypair = Keypair::generate(&mut rng);
        let pk_sig = PubSigPair::create(&keypair, hash);
        pk_sig.verify(hash).unwrap();
    }
}
