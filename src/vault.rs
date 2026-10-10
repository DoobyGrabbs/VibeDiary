//! Password-based encryption for the diary file, with an optional recovery key.
//!
//! The diary text is sealed with XChaCha20-Poly1305 (authenticated, so a wrong password or a
//! damaged file is detected) under a random 256-bit **data key**. That data key is never stored
//! as it is: it is "wrapped" (encrypted) separately by
//!
//! * a key stretched from the **password** with Argon2id, and
//! * optionally a key stretched from a random **recovery key** shown once when it is created.
//!
//! Either one can unlock the diary. Changing the password only re-wraps the data key, so the
//! recovery key keeps working; using the recovery key lets you choose a new password.
//!
//! The result is stored as a small JSON "envelope" holding the key-derivation settings, the
//! wrapped keys, a nonce and the ciphertext. Nothing in it reveals the diary contents.
//!
//! Version 1 files (before recovery keys) used the password-derived key directly. They still
//! open, and are upgraded to version 2 the first time they are unlocked.

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
pub const WRONG_RECOVERY: &str = "That recovery key doesn't match this diary.";
pub const BAD_RECOVERY_FORMAT: &str = "That doesn't look like a recovery key. It has 32 letters and numbers, usually shown in groups of four.";

/// Letters and digits for recovery keys: no I, L, O or U, so they are hard to misread.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// A data key encrypted by a key derived from a password or recovery key.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Wrap {
    salt: String,
    nonce: String,
    wrapped: String,
    /// When a recovery key was made (YYYY-MM-DD); empty for password wraps.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    created: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Envelope {
    format: String,
    version: u32,
    kdf: String,
    m_kib: u32,
    t: u32,
    p: u32,
    /// Version 1 only: the salt of the password-derived data key.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    salt: String,
    nonce: String,
    ciphertext: String,
    /// Version 2: the data key wrapped by the password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    password: Option<Wrap>,
    /// Version 2: the data key wrapped by the recovery key, if one was made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recovery: Option<Wrap>,
}

impl Envelope {
    /// `Some` if `raw` is an encrypted diary file.
    pub fn parse(raw: &str) -> Option<Envelope> {
        serde_json::from_str::<Envelope>(raw).ok().filter(|e| e.format == FORMAT)
    }

    /// Was this file written by the first versions of the app (no recovery key support)?
    pub fn is_legacy(&self) -> bool {
        self.version == 1
    }

    /// Can this diary be unlocked with a recovery key?
    pub fn has_recovery(&self) -> bool {
        self.recovery.is_some()
    }
}

/// An unlocked data key plus everything needed to write the file again.
pub struct Vault {
    dek: [u8; 32],
    password: Wrap,
    recovery: Option<Wrap>,
    m_kib: u32,
    t: u32,
    p: u32,
}

fn derive(secret: &str, salt: &[u8], m_kib: u32, t: u32, p: u32) -> Result<[u8; 32], String> {
    let params = Params::new(m_kib, t, p, Some(32)).map_err(|e| e.to_string())?;
    let mut key = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(secret.as_bytes(), salt, &mut key)
        .map_err(|e| e.to_string())?;
    Ok(key)
}

fn random<const N: usize>() -> Result<[u8; N], String> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

fn decode_array<const N: usize>(text: &str, what: &str) -> Result<[u8; N], String> {
    B64.decode(text).ok().and_then(|v| v.try_into().ok()).ok_or_else(|| format!("The file's {what} is damaged."))
}

/// Encrypt the data key under a key derived from `secret`.
fn make_wrap(secret: &str, dek: &[u8; 32], m_kib: u32, t: u32, p: u32, created: &str) -> Result<Wrap, String> {
    let salt = random::<SALT_LEN>()?;
    let kek = derive(secret, &salt, m_kib, t, p)?;
    let nonce = XNonce::generate();
    let wrapped = XChaCha20Poly1305::new(&Key::from(kek)).encrypt(&nonce, dek.as_slice()).map_err(|e| e.to_string())?;
    Ok(Wrap { salt: B64.encode(salt), nonce: B64.encode(nonce), wrapped: B64.encode(wrapped), created: created.to_string() })
}

/// Recover the data key from a wrap, or `None` if `secret` is wrong.
fn open_wrap(wrap: &Wrap, secret: &str, m_kib: u32, t: u32, p: u32) -> Result<Option<[u8; 32]>, String> {
    let salt: [u8; SALT_LEN] = decode_array(&wrap.salt, "salt")?;
    let nonce: [u8; NONCE_LEN] = decode_array(&wrap.nonce, "nonce")?;
    let wrapped = B64.decode(&wrap.wrapped).map_err(|_| "The file's key data is damaged.".to_string())?;
    let kek = derive(secret, &salt, m_kib, t, p)?;
    let opened = XChaCha20Poly1305::new(&Key::from(kek)).decrypt(&XNonce::from(nonce), wrapped.as_slice()).ok();
    Ok(opened.and_then(|v| <[u8; 32]>::try_from(v).ok()))
}

fn check_settings(env: &Envelope) -> Result<(), String> {
    if env.kdf != "argon2id" || !(1..=2).contains(&env.version) {
        return Err("This file was made by a newer version of the app.".into());
    }
    // Guard against absurd settings in a tampered file.
    if env.m_kib > 1024 * 1024 || env.t > 20 || env.p > 16 {
        return Err("The encryption settings in the file are invalid.".into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Recovery keys
// ---------------------------------------------------------------------------------------------

/// Show a recovery key in groups of four: `ABCD-EFGH-…`.
fn format_recovery(raw: &str) -> String {
    raw.as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).into_owned()).collect::<Vec<_>>().join("-")
}

/// 160 random bits as 32 characters.
fn new_recovery_key() -> Result<String, String> {
    let bytes = random::<20>()?;
    let (mut bits, mut have, mut out) = (0u32, 0u32, String::new());
    for b in bytes {
        bits = (bits << 8) | u32::from(b);
        have += 8;
        while have >= 5 {
            out.push(ALPHABET[((bits >> (have - 5)) & 31) as usize] as char);
            have -= 5;
        }
    }
    Ok(out)
}

/// What a person typed, as the 32-character secret: case, spaces and dashes don't matter, and
/// O, I and L are read as 0, 1 and 1.
pub fn normalize_recovery(text: &str) -> Option<String> {
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        })
        .collect();
    (cleaned.len() == 32 && cleaned.bytes().all(|b| ALPHABET.contains(&b))).then_some(cleaned)
}

// ---------------------------------------------------------------------------------------------
// The vault
// ---------------------------------------------------------------------------------------------

impl Vault {
    /// Start a new vault for `password`, with a fresh random data key and no recovery key.
    pub fn create(password: &str) -> Result<Vault, String> {
        let dek = random::<32>()?;
        let wrap = make_wrap(password, &dek, M_KIB, T_COST, P_COST, "")?;
        Ok(Vault { dek, password: wrap, recovery: None, m_kib: M_KIB, t: T_COST, p: P_COST })
    }

    /// Unlock an existing file with the password. Returns the vault and the decrypted bytes.
    /// A version 1 file comes back as a version 2 vault, ready to be saved in the new format.
    pub fn unlock(env: &Envelope, password: &str) -> Result<(Vault, Vec<u8>), String> {
        check_settings(env)?;
        if env.version == 1 {
            let salt: [u8; SALT_LEN] = decode_array(&env.salt, "salt")?;
            let key = derive(password, &salt, env.m_kib, env.t, env.p)?;
            let plain = open_with(&key, env)?;
            return Ok((Vault::create(password)?, plain));
        }
        let wrap = env.password.as_ref().ok_or("The file's password key is missing.")?;
        let dek = open_wrap(wrap, password, env.m_kib, env.t, env.p)?.ok_or(WRONG_PASSWORD)?;
        let vault = Vault { dek, password: wrap.clone(), recovery: env.recovery.clone(), m_kib: env.m_kib, t: env.t, p: env.p };
        let plain = vault.open(env)?;
        Ok((vault, plain))
    }

    /// Unlock with the recovery key instead of the password. The caller should then call
    /// [`Vault::set_password`] so the diary has a password again.
    pub fn unlock_with_recovery(env: &Envelope, key_text: &str) -> Result<(Vault, Vec<u8>), String> {
        check_settings(env)?;
        let wrap = env.recovery.as_ref().ok_or("This diary has no recovery key.")?;
        let secret = normalize_recovery(key_text).ok_or(BAD_RECOVERY_FORMAT)?;
        let dek = open_wrap(wrap, &secret, env.m_kib, env.t, env.p)?.ok_or(WRONG_RECOVERY)?;
        let password = env.password.clone().ok_or("The file's password key is missing.")?;
        let vault = Vault { dek, password, recovery: Some(wrap.clone()), m_kib: env.m_kib, t: env.t, p: env.p };
        let plain = vault.open(env).map_err(|_| WRONG_RECOVERY.to_string())?;
        Ok((vault, plain))
    }

    /// Choose a new password. The recovery key, if any, is unaffected.
    pub fn set_password(&mut self, password: &str) -> Result<(), String> {
        self.password = make_wrap(password, &self.dek, self.m_kib, self.t, self.p, "")?;
        Ok(())
    }

    /// Make a new recovery key, replacing any earlier one, and return it for display (once).
    pub fn set_recovery(&mut self) -> Result<String, String> {
        let key = new_recovery_key()?;
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        self.recovery = Some(make_wrap(&key, &self.dek, self.m_kib, self.t, self.p, &today)?);
        Ok(format_recovery(&key))
    }

    pub fn remove_recovery(&mut self) {
        self.recovery = None;
    }

    pub fn has_recovery(&self) -> bool {
        self.recovery.is_some()
    }

    /// The day the current recovery key was made.
    pub fn recovery_created(&self) -> Option<&str> {
        self.recovery.as_ref().map(|w| w.created.as_str()).filter(|c| !c.is_empty())
    }

    fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new(&Key::from(self.dek))
    }

    /// Decrypt an envelope's data with this vault's data key.
    pub fn open(&self, env: &Envelope) -> Result<Vec<u8>, String> {
        open_with(&self.dek, env)
    }

    /// Encrypt `plain` with a fresh nonce and return the envelope as JSON text.
    pub fn seal(&self, plain: &[u8]) -> Result<String, String> {
        let nonce = XNonce::generate();
        let ciphertext = self.cipher().encrypt(&nonce, plain).map_err(|e| e.to_string())?;
        let env = Envelope {
            format: FORMAT.into(),
            version: 2,
            kdf: "argon2id".into(),
            m_kib: self.m_kib,
            t: self.t,
            p: self.p,
            salt: String::new(),
            nonce: B64.encode(nonce),
            ciphertext: B64.encode(ciphertext),
            password: Some(self.password.clone()),
            recovery: self.recovery.clone(),
        };
        serde_json::to_string_pretty(&env).map_err(|e| e.to_string())
    }
}

fn open_with(key: &[u8; 32], env: &Envelope) -> Result<Vec<u8>, String> {
    let nonce: [u8; NONCE_LEN] = decode_array(&env.nonce, "nonce")?;
    let ciphertext = B64.decode(&env.ciphertext).map_err(|_| "The file's data is damaged.")?;
    XChaCha20Poly1305::new(&Key::from(*key))
        .decrypt(&XNonce::from(nonce), ciphertext.as_slice())
        .map_err(|_| WRONG_PASSWORD.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(sealed: &str) -> Envelope {
        Envelope::parse(sealed).unwrap()
    }

    #[test]
    fn round_trip_wrong_password_and_tampering() {
        let vault = Vault::create("correct horse").unwrap();
        let sealed = vault.seal(b"secret diary").unwrap();
        assert!(!sealed.contains("secret diary"));

        let env = parse(&sealed);
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

    #[test]
    fn recovery_key_unlocks_and_lets_you_choose_a_new_password() {
        let mut vault = Vault::create("old password").unwrap();
        assert!(!vault.has_recovery());
        let key = vault.set_recovery().unwrap();
        assert_eq!(key.len(), 32 + 7, "eight groups of four, with dashes: {key}");
        assert!(vault.has_recovery() && vault.recovery_created().is_some());
        let sealed = vault.seal(b"my entries").unwrap();
        assert!(!sealed.contains(&key.replace('-', "")), "the recovery key itself is never stored");

        let env = parse(&sealed);
        assert!(env.has_recovery());
        // How a person might type it: lower case, spaces instead of dashes.
        let typed = key.to_lowercase().replace('-', " ");
        let (mut recovered, plain) = Vault::unlock_with_recovery(&env, &typed).unwrap();
        assert_eq!(plain, b"my entries");

        // Choose a new password: it works, the old one no longer does, the recovery key still does.
        recovered.set_password("new password").unwrap();
        let env2 = parse(&recovered.seal(b"my entries").unwrap());
        assert_eq!(Vault::unlock(&env2, "new password").unwrap().1, b"my entries");
        assert_eq!(Vault::unlock(&env2, "old password").err().as_deref(), Some(WRONG_PASSWORD));
        assert!(Vault::unlock_with_recovery(&env2, &key).is_ok(), "the recovery key survives a password change");
    }

    #[test]
    fn wrong_or_missing_recovery_keys_are_rejected() {
        let mut vault = Vault::create("pw").unwrap();
        let key = vault.set_recovery().unwrap();
        let env = parse(&vault.seal(b"x").unwrap());

        let other = {
            let mut v = Vault::create("pw").unwrap();
            v.set_recovery().unwrap()
        };
        assert_ne!(key, other);
        assert_eq!(Vault::unlock_with_recovery(&env, &other).err().as_deref(), Some(WRONG_RECOVERY));
        assert_eq!(Vault::unlock_with_recovery(&env, "too short").err().as_deref(), Some(BAD_RECOVERY_FORMAT));
        assert_eq!(Vault::unlock_with_recovery(&env, &"U".repeat(32)).err().as_deref(), Some(BAD_RECOVERY_FORMAT));

        // Replacing the key makes the old one useless; removing it removes recovery altogether.
        vault.set_recovery().unwrap();
        let env_new = parse(&vault.seal(b"x").unwrap());
        assert_eq!(Vault::unlock_with_recovery(&env_new, &key).err().as_deref(), Some(WRONG_RECOVERY));
        vault.remove_recovery();
        let env_none = parse(&vault.seal(b"x").unwrap());
        assert!(!env_none.has_recovery());
        assert!(Vault::unlock_with_recovery(&env_none, &key).is_err());
        assert!(Vault::unlock(&env_none, "pw").is_ok());
    }

    #[test]
    fn recovery_keys_are_typed_forgivingly() {
        assert_eq!(normalize_recovery("abcd efgh-JKMN pqrs-tvwx-yz01-2345-6789").as_deref(), Some("ABCDEFGHJKMNPQRSTVWXYZ0123456789"));
        // O, I and L are read as 0, 1, 1.
        assert_eq!(normalize_recovery(&format!("{}OIL", "A".repeat(29))).as_deref(), Some(&*format!("{}011", "A".repeat(29))));
        assert_eq!(normalize_recovery("short"), None);
        assert_eq!(normalize_recovery(&"!".repeat(32)), None);
        for _ in 0..20 {
            let k = new_recovery_key().unwrap();
            assert_eq!(k.len(), 32);
            assert_eq!(normalize_recovery(&format_recovery(&k)).as_deref(), Some(k.as_str()));
        }
    }

    #[test]
    fn old_version_1_files_open_and_upgrade() {
        // Build a version 1 file the way the first versions of the app did.
        let salt = random::<SALT_LEN>().unwrap();
        let key = derive("legacy pw", &salt, M_KIB, T_COST, P_COST).unwrap();
        let nonce = XNonce::generate();
        let ciphertext = XChaCha20Poly1305::new(&Key::from(key)).encrypt(&nonce, b"old diary".as_slice()).unwrap();
        let legacy = Envelope {
            format: FORMAT.into(),
            version: 1,
            kdf: "argon2id".into(),
            m_kib: M_KIB,
            t: T_COST,
            p: P_COST,
            salt: B64.encode(salt),
            nonce: B64.encode(nonce),
            ciphertext: B64.encode(ciphertext),
            password: None,
            recovery: None,
        };
        let raw = serde_json::to_string(&legacy).unwrap();
        let env = parse(&raw);
        assert_eq!(Vault::unlock(&env, "nope").err().as_deref(), Some(WRONG_PASSWORD));
        let (vault, plain) = Vault::unlock(&env, "legacy pw").unwrap();
        assert_eq!(plain, b"old diary");

        // Saving now writes the new format, which opens with the same password.
        let upgraded = parse(&vault.seal(&plain).unwrap());
        assert!(upgraded.password.is_some() && upgraded.salt.is_empty());
        assert_eq!(Vault::unlock(&upgraded, "legacy pw").unwrap().1, b"old diary");
        assert!(Vault::unlock_with_recovery(&env, "AAAA").is_err(), "version 1 files have no recovery key");
    }

    #[test]
    fn files_from_a_newer_version_are_refused() {
        let vault = Vault::create("pw").unwrap();
        let mut env = parse(&vault.seal(b"x").unwrap());
        env.version = 99;
        assert!(Vault::unlock(&env, "pw").err().unwrap().contains("newer version"));
        env.version = 2;
        env.m_kib = u32::MAX;
        assert!(Vault::unlock(&env, "pw").err().unwrap().contains("invalid"));
    }
}
