//! The clipboard key, wrapped with Windows DPAPI (ADR-0008).
//!
//! 32 bytes generated once and stored at `creds\clip.key.dpapi`. DPAPI is called
//! without `CRYPTPROTECT_LOCAL_MACHINE`, so unwrapping needs the user's logon
//! credentials: another account on the machine holding the file learns nothing.
//!
//! Additional entropy is passed too. Without it any process running as the user
//! can unwrap the blob by handing it back to DPAPI; with it, a reader also has to
//! know the string below. Obfuscation rather than a boundary — DPAPI's real one
//! is the account — but it costs nothing and removes the trivial case.

use std::path::{Path, PathBuf};

use windows::core::PCWSTR;
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
};
use windows::Win32::Foundation::{LocalFree, HLOCAL};

/// The key file, relative to the data directory.
pub const KEY_PATH: &[&str] = &["creds", "clip.key.dpapi"];

/// AES-256, so 32 bytes. Named rather than spelled inline: `store.rs` asserts it.
pub const KEY_LEN: usize = 32;

/// Entropy mixed into every wrap. Frozen — changing it orphans every stored clip,
/// which reads to the user as history that silently emptied itself.
const ENTROPY: &[u8] = b"com.v3sper.launcher/clip.key/v1";

/// 32 bytes of key material, zeroed when dropped.
///
/// Neither `Copy` nor `Clone`: one owner, so there is no second copy to forget.
pub struct ClipKey([u8; KEY_LEN]);

impl ClipKey {
    pub fn bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    /// Fresh key from the OS CSPRNG.
    pub fn generate() -> Self {
        use aes_gcm::aead::rand_core::RngCore;
        use aes_gcm::aead::OsRng;
        let mut bytes = [0u8; KEY_LEN];
        OsRng.fill_bytes(&mut bytes);
        ClipKey(bytes)
    }
}

impl Drop for ClipKey {
    /// Overwrite before the allocation is reused.
    ///
    /// `write_volatile`, so the optimiser cannot drop a write nothing reads back.
    /// Not a defence against a memory dump while running — shortens the window,
    /// does not close it.
    fn drop(&mut self) {
        for b in self.0.iter_mut() {
            unsafe { std::ptr::write_volatile(b, 0) };
        }
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    }
}

/// The key for `dir`, creating and wrapping one on first call.
///
/// A key file that exists but will not unwrap is an error, never a silently
/// regenerated key: regenerating makes every stored clip undecryptable, and the
/// user sees an empty history rather than a failure.
pub fn load_or_create(dir: &Path) -> std::io::Result<ClipKey> {
    let path = key_file(dir);
    if path.exists() {
        let wrapped = std::fs::read(&path)?;
        return into_key(unprotect(&wrapped)?);
    }

    let key = ClipKey::generate();
    let wrapped = protect(key.bytes())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, wrapped)?;
    Ok(key)
}

pub fn key_file(dir: &Path) -> PathBuf {
    let mut path = dir.to_path_buf();
    for part in KEY_PATH {
        path.push(part);
    }
    path
}

fn into_key(bytes: Vec<u8>) -> std::io::Result<ClipKey> {
    if bytes.len() != KEY_LEN {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("clipboard key is {} bytes, expected {KEY_LEN}", bytes.len()),
        ));
    }
    let mut out = [0u8; KEY_LEN];
    out.copy_from_slice(&bytes);
    Ok(ClipKey(out))
}

/// Wrap with DPAPI, bound to the current user account.
pub fn protect(plain: &[u8]) -> std::io::Result<Vec<u8>> {
    dpapi(plain, ENTROPY, true)
}

/// Unwrap a blob produced by [`protect`].
pub fn unprotect(wrapped: &[u8]) -> std::io::Result<Vec<u8>> {
    dpapi(wrapped, ENTROPY, false)
}

/// Wrap under a caller's own entropy, for a secret that is not this key.
///
/// Domain separation: the Brave key and the clipboard key are different secrets
/// with different lifetimes, and a blob wrapped for one must not unwrap in the
/// other's code path by accident.
pub fn protect_with(plain: &[u8], entropy: &[u8]) -> std::io::Result<Vec<u8>> {
    dpapi(plain, entropy, true)
}

/// Unwrap a blob produced by [`protect_with`] under the same entropy.
pub fn unprotect_with(wrapped: &[u8], entropy: &[u8]) -> std::io::Result<Vec<u8>> {
    dpapi(wrapped, entropy, false)
}

/// The two DPAPI calls, which differ only in direction.
///
/// Both hand back a `LocalAlloc`ed buffer that has to be freed here — the one
/// mistake in this API that leaks silently rather than failing.
fn dpapi(input: &[u8], entropy: &[u8], encrypt: bool) -> std::io::Result<Vec<u8>> {
    let mut entropy = entropy.to_vec();
    let mut input = input.to_vec();
    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: input.len() as u32,
        pbData: input.as_mut_ptr(),
    };
    let entropy_blob = CRYPT_INTEGER_BLOB {
        cbData: entropy.len() as u32,
        pbData: entropy.as_mut_ptr(),
    };
    let mut out = CRYPT_INTEGER_BLOB::default();

    let called = unsafe {
        if encrypt {
            CryptProtectData(
                &in_blob,
                PCWSTR::null(),
                Some(&entropy_blob),
                None,
                None,
                0,
                &mut out,
            )
        } else {
            CryptUnprotectData(&in_blob, None, Some(&entropy_blob), None, None, 0, &mut out)
        }
    };
    called.map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "DPAPI {} failed: {e}",
                if encrypt { "protect" } else { "unprotect" }
            ),
        )
    })?;

    let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    unsafe {
        let _ = LocalFree(Some(HLOCAL(out.pbData as *mut _)));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_5_a_key_is_thirty_two_bytes_and_not_all_zero() {
        let key = ClipKey::generate();
        assert_eq!(key.bytes().len(), KEY_LEN);
        assert!(key.bytes().iter().any(|b| *b != 0));
    }

    #[test]
    fn v0_5_two_generated_keys_differ() {
        assert_ne!(ClipKey::generate().bytes(), ClipKey::generate().bytes());
    }

    #[test]
    fn v0_5_a_wrapped_key_round_trips_through_dpapi() {
        let key = ClipKey::generate();
        let wrapped = protect(key.bytes()).expect("protect");
        assert_eq!(&unprotect(&wrapped).expect("unprotect"), key.bytes());
    }

    /// The point of the file: what lands on disk must not be the key.
    #[test]
    fn v0_5_the_wrapped_blob_does_not_contain_the_key() {
        let key = ClipKey::generate();
        let wrapped = protect(key.bytes()).expect("protect");
        assert!(
            !wrapped.windows(KEY_LEN).any(|w| w == key.bytes().as_slice()),
            "the plaintext key appears verbatim inside its own DPAPI blob"
        );
    }

    /// Two secrets, two entropies. A blob wrapped for the clipboard key must not
    /// unwrap in the Brave key's path, or the separation is decoration.
    #[test]
    fn v0_9_a_blob_does_not_cross_between_entropies() {
        let wrapped = protect(b"clip-key-material").expect("protect");
        assert!(unprotect_with(&wrapped, b"some.other.domain/v1").is_err());
        let other = protect_with(b"brave-key", b"some.other.domain/v1").expect("protect");
        assert!(unprotect(&other).is_err());
        assert_eq!(
            unprotect_with(&other, b"some.other.domain/v1").expect("unprotect"),
            b"brave-key"
        );
    }

    /// Entropy is part of the contract. A blob wrapped with it must not unwrap
    /// without it, or the constant is decoration.
    #[test]
    fn v0_5_unwrapping_without_the_entropy_fails() {
        let key = ClipKey::generate();
        let mut wrapped = protect(key.bytes()).expect("protect");
        let blob = CRYPT_INTEGER_BLOB {
            cbData: wrapped.len() as u32,
            pbData: wrapped.as_mut_ptr(),
        };
        let mut out = CRYPT_INTEGER_BLOB::default();
        let called = unsafe { CryptUnprotectData(&blob, None, None, None, None, 0, &mut out) };
        assert!(called.is_err(), "DPAPI unwrapped the blob without entropy");
    }

    #[test]
    fn v0_5_a_short_blob_is_rejected_rather_than_padded() {
        assert!(into_key(vec![0u8; 8]).is_err());
        assert!(into_key(vec![0u8; KEY_LEN]).is_ok());
    }
}
