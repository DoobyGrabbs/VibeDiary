//! Password-based encryption for the diary file.
//!
//! The password is stretched into a 256-bit key with Argon2id and the data is sealed with
//! XChaCha20-Poly1305 (authenticated, so a wrong password or a damaged file is detected).
//! The result is stored as a small JSON "envelope" holding the KDF settings, salt, nonce and
//! ciphertext. Nothing in it reveals the diary contents.

use argon2::{Algorithm, Argon2, Params, Version};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use chacha20poly1305::{
    Key, KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Generate},
};
use serde::{Deserialize, Serialize};

const FORMAT: &str = "diary-encrypted";
const M_KIB: u32 = 64 * 1024; // 64 MiB
const T_COST: u32 = 3;
const P_COST: u32 = 1;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;

pub const WRONG_PASSWORD: &str = "Incorrect password, or the file is damaged.";

#[derive(Serialize, Deserialize)]
#[derive(Clone)]
pub struct Envelope {
    format: String,
    version: u32,
    kdf: String,
    m_kib: u32,
    t: u32,
    p: u32,
    salt: String,
    nonce: String,
    ciphertext: String,
}

impl Envelope {
    /// `Some` if `raw` is an encrypted diary file.
    pub fn parse(raw: &str) -> Option<Envelope> {
        serde_json::from_str::<Envelope>(raw).ok().filter(|e| e.format == FORMAT)
    }
}

/// An unlocked key plus the settings needed to write the file again.
pub struct Vault {
    key: [u8; 32],
    salt: [u8; SALT_LEN],
    m_kib: u32,
    t: u32,
    p: u32,
}

fn derive(password: &str, salt: &[u8], m_kib: u32, t: u32, p: u32) -> Result<[u8; 32], String> {
    let params = Params::new(m_kib, t, p, Some(32)).map_err(|e| e.to_string())?;
    let mut key = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| e.to_string())?;
    Ok(key)
}

impl Vault {
    /// Start a new vault for `password` with a fresh random salt.
    pub fn create(password: &str) -> Result<Vault, String> {
        let mut salt = [0u8; SALT_LEN];
        getrandom::fill(&mut salt).map_err(|e| e.to_string())?;
        let key = derive(password, &salt, M_KIB, T_COST, P_COST)?;
        Ok(Vault { key, salt, m_kib: M_KIB, t: T_COST, p: P_COST })
    }

    /// Unlock an existing file. Returns the vault and the decrypted bytes.
    pub fn unlock(env: &Envelope, password: &str) -> Result<(Vault, Vec<u8>), String> {
        if env.kdf != "argon2id" || env.version != 1 {
            return Err("This file was made by a newer version of the app.".into());
        }
        // Guard against absurd settings in a tampered file.
        if env.m_kib > 1024 * 1024 || env.t > 20 || env.p > 16 {
            return Err("The encryption settings in the file are invalid.".into());
        }
        let salt: [u8; SALT_LEN] = B64
            .decode(&env.salt)
            .ok()
            .and_then(|v| v.try_into().ok())
            .ok_or("The file's salt is damaged.")?;
        let key = derive(password, &salt, env.m_kib, env.t, env.p)?;
        let vault = Vault { key, salt, m_kib: env.m_kib, t: env.t, p: env.p };
        let plain = vault.open(env)?;
        Ok((vault, plain))
    }

    fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new(&Key::from(self.key))
    }

    /// Decrypt an envelope with this vault's key.
    pub fn open(&self, env: &Envelope) -> Result<Vec<u8>, String> {
        let nonce: [u8; NONCE_LEN] = B64
            .decode(&env.nonce)
            .ok()
            .and_then(|v| v.try_into().ok())
            .ok_or("The file's nonce is damaged.")?;
        let ciphertext = B64.decode(&env.ciphertext).map_err(|_| "The file's data is damaged.")?;
        self.cipher()
            .decrypt(&XNonce::from(nonce), ciphertext.as_slice())
            .map_err(|_| WRONG_PASSWORD.to_string())
    }

    /// Encrypt `plain` with a fresh nonce and return the envelope as JSON text.
    pub fn seal(&self, plain: &[u8]) -> Result<String, String> {
        let nonce = XNonce::generate();
        let ciphertext = self.cipher().encrypt(&nonce, plain).map_err(|e| e.to_string())?;
        let env = Envelope {
            format: FORMAT.into(),
            version: 1,
            kdf: "argon2id".into(),
            m_kib: self.m_kib,
            t: self.t,
            p: self.p,
            salt: B64.encode(self.salt),
            nonce: B64.encode(nonce),
            ciphertext: B64.encode(ciphertext),
        };
        serde_json::to_string_pretty(&env).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_wrong_password_and_tampering() {
        let vault = Vault::create("correct horse").unwrap();
        let sealed = vault.seal(b"secret diary").unwrap();
        assert!(!sealed.contains("secret diary"));

        let env = Envelope::parse(&sealed).unwrap();
        let (_, plain) = Vault::unlock(&env, "correct horse").unwrap();
        assert_eq!(plain, b"secret diary");

        assert_eq!(Vault::unlock(&env, "wrong").err().as_deref(), Some(WRONG_PASSWORD));

        // A fresh nonce is used each time, and flipping any ciphertext bit is detected.
        assert_ne!(sealed, vault.seal(b"secret diary").unwrap());
        let mut bad = env.clone();
        let mut ct = B64.decode(&bad.ciphertext).unwrap();
        ct[0] ^= 1;
        bad.ciphertext = B64.encode(ct);
        assert!(Vault::unlock(&bad, "correct horse").is_err());

        // Plain JSON is not mistaken for an encrypted file.
        assert!(Envelope::parse("{\"categories\":[],\"entries\":[]}").is_none());
    }
}
