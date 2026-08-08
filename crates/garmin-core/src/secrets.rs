//! The one place a secret is read from or written to, per platform.
//!
//! Desktop has an OS keyring and this is a thin wrapper over it. Android does
//! not: the `keyring` crate has no backend there, so `Entry::new` gives back
//! something that either refuses every call or forgets on exit, depending on
//! which features are compiled in. Neither is a place to keep a Garmin refresh
//! token.
//!
//! # What the Android side is actually worth
//!
//! Be clear about this, because "encrypted" invites more confidence than it has
//! earned here. The security boundary on Android is the app-private directory:
//! a per-app UID plus SELinux, enforced by the kernel, which is what stops
//! another installed app reading these bytes. That is the real protection, and
//! it is the same protection an unencrypted file would have had.
//!
//! The AES-GCM layer below buys exactly two things on top of it. It keeps
//! tokens out of anything that copies files without being the app — an `adb
//! backup`, a cloud restore, a recovery-mode image — and it means a plaintext
//! search of the device's storage doesn't turn up a bearer token. It does *not*
//! withstand root: the key sits in a sibling file, because there is nowhere
//! else to put it without going through the Android Keystore, and that needs
//! JNI and a Kotlin plugin this build doesn't have. Someone who can read
//! `secrets.bin` can read `secrets.key`.
//!
//! `allowBackup="false"` in the manifest covers the same ground from the other
//! direction and is arguably the stronger half of the pair. Both are set.
//!
//! Moving to the Keystore later changes this file and nothing above it — which
//! is the point of the module existing at all.

use anyhow::Result;

/// What to call the backing store when a failure has to be explained to
/// someone. The messages in `store` are written for a person who may need to go
/// and unlock something, so naming the wrong mechanism is worse than vague.
#[cfg(not(target_os = "android"))]
pub const STORE: &str = "the OS keyring";
#[cfg(target_os = "android")]
pub const STORE: &str = "the app's encrypted store";

/* ------------------------------------------------------------- desktop --- */

#[cfg(not(target_os = "android"))]
mod imp {
    use anyhow::{Context, Result};

    const SERVICE: &str = "com.omznc.garmincompanion";

    fn entry(account: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(SERVICE, account).context("could not open the OS keyring")
    }

    pub fn get(account: &str) -> Result<Option<String>> {
        match entry(account)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn set(account: &str, value: &str) -> Result<()> {
        entry(account)?.set_password(value).map_err(Into::into)
    }

    pub fn delete(account: &str) -> Result<()> {
        match entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

/* ------------------------------------------------------------- android --- */

#[cfg(target_os = "android")]
mod imp {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::Aes256Gcm;
    use anyhow::{anyhow, Context, Result};
    use std::collections::BTreeMap;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::PathBuf;

    /// Every account in one file rather than one file each. There are three of
    /// them and they are written together often enough that a single
    /// read-modify-write is simpler to keep consistent than three.
    type Bag = BTreeMap<String, String>;

    const NONCE_LEN: usize = 12;

    fn key_path() -> Result<PathBuf> {
        Ok(crate::paths::ensure_base_dir()?.join("secrets.key"))
    }

    fn bag_path() -> Result<PathBuf> {
        Ok(crate::paths::ensure_base_dir()?.join("secrets.bin"))
    }

    /// Write a file only this UID can open. The mode matters less than the
    /// directory it lands in, but a stray world-readable file inside a private
    /// directory is still a thing worth not creating.
    fn write_private(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("could not open {}", path.display()))?;
        f.write_all(bytes)
            .with_context(|| format!("could not write {}", path.display()))
    }

    /// The file key, generated on first use.
    fn key() -> Result<[u8; 32]> {
        let path = key_path()?;
        if let Ok(bytes) = std::fs::read(&path) {
            if bytes.len() == 32 {
                let mut k = [0u8; 32];
                k.copy_from_slice(&bytes);
                return Ok(k);
            }
            // A key of the wrong length can't decrypt anything, and keeping it
            // would mean failing every read forever. Replacing it drops the
            // stored secrets, which costs a fresh sign-in — recoverable, unlike
            // the alternative.
            let _ = std::fs::remove_file(bag_path()?);
        }

        let mut k = [0u8; 32];
        getrandom::fill(&mut k).map_err(|e| anyhow!("no system randomness available: {e}"))?;
        write_private(&path, &k)?;
        Ok(k)
    }

    fn cipher() -> Result<Aes256Gcm> {
        Aes256Gcm::new_from_slice(&key()?).map_err(|e| anyhow!("bad key length: {e}"))
    }

    fn load() -> Result<Bag> {
        let path = bag_path()?;
        let raw = match std::fs::read(&path) {
            Ok(raw) => raw,
            // Nothing stored yet is the normal first-run state.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Bag::new()),
            Err(e) => return Err(e).context("could not read the encrypted store"),
        };
        if raw.len() <= NONCE_LEN {
            return Ok(Bag::new());
        }

        let (nonce, body) = raw.split_at(NONCE_LEN);
        let nonce: [u8; NONCE_LEN] = nonce
            .try_into()
            .map_err(|_| anyhow!("the encrypted store has a malformed header"))?;
        let plain = cipher()?
            .decrypt(&nonce.into(), body)
            .map_err(|_| anyhow!("the encrypted store could not be decrypted"))?;
        serde_json::from_slice(&plain).context("the encrypted store is corrupt")
    }

    fn save(bag: &Bag) -> Result<()> {
        let plain = serde_json::to_vec(bag)?;
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::fill(&mut nonce).map_err(|e| anyhow!("no system randomness available: {e}"))?;

        let body = cipher()?
            .encrypt(&nonce.into(), plain.as_slice())
            .map_err(|_| anyhow!("could not encrypt the store"))?;

        let mut out = Vec::with_capacity(NONCE_LEN + body.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&body);
        write_private(&bag_path()?, &out)
    }

    pub fn get(account: &str) -> Result<Option<String>> {
        Ok(load()?.get(account).cloned())
    }

    pub fn set(account: &str, value: &str) -> Result<()> {
        let mut bag = load()?;
        bag.insert(account.to_string(), value.to_string());
        save(&bag)
    }

    pub fn delete(account: &str) -> Result<()> {
        let mut bag = load()?;
        if bag.remove(account).is_none() {
            return Ok(());
        }
        save(&bag)
    }
}

/// The stored value, or `None` if there isn't one. A missing entry is a normal
/// state on both platforms and never an error.
pub fn get(account: &str) -> Result<Option<String>> {
    imp::get(account)
}

pub fn set(account: &str, value: &str) -> Result<()> {
    imp::set(account, value)
}

/// Remove the entry. Removing one that was never there succeeds.
pub fn delete(account: &str) -> Result<()> {
    imp::delete(account)
}
