//! AES-256-GCM encryption for stored passwords.
//!
//! - The 32-byte key comes from `VAULT_KEY` (base64) and never touches the database.
//! - Every secret gets its own random 12-byte nonce. Reusing a nonce with the
//!   same key would break GCM, so we never derive or reuse one.
//! - The credential's id is bound in as "associated data": a ciphertext copied
//!   onto another row fails to decrypt instead of revealing the wrong password.
//! - GCM is authenticated, so a tampered ciphertext fails too.

use aes_gcm::{
    Aes256Gcm,
    aead::{Aead, Generate, Key, KeyInit, Nonce, Payload},
};
use base64::{Engine, engine::general_purpose::STANDARD};

#[derive(Clone)]
pub struct VaultKey {
    cipher: Aes256Gcm,
}

/// Never print the key, even in debug logs.
impl std::fmt::Debug for VaultKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VaultKey(..)")
    }
}

pub struct Sealed {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
}

impl VaultKey {
    /// A brand-new random key, base64-encoded, for `vault-init`.
    pub fn generate_base64() -> String {
        STANDARD.encode(Key::<Aes256Gcm>::generate())
    }

    pub fn from_base64(value: &str) -> anyhow::Result<Self> {
        let bytes = STANDARD
            .decode(value.trim())
            .map_err(|_| anyhow::anyhow!("VAULT_KEY is not valid base64"))?;
        if bytes.len() != 32 {
            anyhow::bail!("VAULT_KEY must be 32 bytes (got {})", bytes.len());
        }
        let cipher = Aes256Gcm::new_from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("VAULT_KEY has the wrong length"))?;
        Ok(Self { cipher })
    }

    pub fn encrypt(&self, plaintext: &str, aad: &[u8]) -> anyhow::Result<Sealed> {
        let nonce = Nonce::<Aes256Gcm>::generate();
        let ciphertext = self
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext.as_bytes(),
                    aad,
                },
            )
            .map_err(|_| anyhow::anyhow!("encryption failed"))?;
        Ok(Sealed {
            ciphertext,
            nonce: nonce.to_vec(),
        })
    }

    pub fn decrypt(&self, ciphertext: &[u8], nonce: &[u8], aad: &[u8]) -> anyhow::Result<String> {
        let nonce = Nonce::<Aes256Gcm>::try_from(nonce)
            .map_err(|_| anyhow::anyhow!("stored nonce has the wrong length"))?;
        let plaintext = self
            .cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: ciphertext,
                    aad,
                },
            )
            .map_err(|_| anyhow::anyhow!("could not decrypt: wrong VAULT_KEY or tampered data"))?;
        Ok(String::from_utf8(plaintext)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let key = VaultKey::from_base64(&VaultKey::generate_base64()).unwrap();
        let sealed = key.encrypt("hunter2", b"id-1").unwrap();
        assert_ne!(sealed.ciphertext, b"hunter2");
        assert_eq!(
            key.decrypt(&sealed.ciphertext, &sealed.nonce, b"id-1")
                .unwrap(),
            "hunter2"
        );
    }

    #[test]
    fn same_password_twice_gives_different_ciphertext() {
        let key = VaultKey::from_base64(&VaultKey::generate_base64()).unwrap();
        let a = key.encrypt("same", b"x").unwrap();
        let b = key.encrypt("same", b"x").unwrap();
        assert_ne!(a.nonce, b.nonce);
        assert_ne!(a.ciphertext, b.ciphertext);
    }

    #[test]
    fn wrong_key_wrong_row_or_tampering_fail() {
        let key = VaultKey::from_base64(&VaultKey::generate_base64()).unwrap();
        let other = VaultKey::from_base64(&VaultKey::generate_base64()).unwrap();
        let mut sealed = key.encrypt("secret", b"row-1").unwrap();

        assert!(
            other
                .decrypt(&sealed.ciphertext, &sealed.nonce, b"row-1")
                .is_err()
        );
        assert!(
            key.decrypt(&sealed.ciphertext, &sealed.nonce, b"row-2")
                .is_err()
        );
        sealed.ciphertext[0] ^= 1;
        assert!(
            key.decrypt(&sealed.ciphertext, &sealed.nonce, b"row-1")
                .is_err()
        );
    }

    #[test]
    fn rejects_bad_keys() {
        assert!(VaultKey::from_base64("not base64!").is_err());
        assert!(VaultKey::from_base64(&STANDARD.encode([0u8; 16])).is_err());
    }
}
