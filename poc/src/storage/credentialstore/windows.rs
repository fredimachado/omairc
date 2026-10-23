//! The Windows Credential Manager, through `CredWriteW`, `CredReadW` and `CredDeleteW`. Port of
//! `tui/internal/storage/secretservice_windows.go`: generic credentials at the target
//! `<key name>@omairc`, the name QtKeychain 0.17+ writes. A read or delete also tries the bare
//! key name, for entries older QtKeychain builds wrote.

use std::ffi::c_void;
use std::io;
use std::iter::once;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::ERROR_NOT_FOUND;
use windows_sys::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_ENTERPRISE,
    CRED_TYPE_GENERIC,
};

use super::{CredentialKey, CredentialState, CredentialStore};

/// QtKeychain's service name, the suffix of every target.
const SERVICE: &str = "omairc";
/// An entry no client writes, for the availability probe.
const PROBE_TARGET: &str = "omairc/probe@omairc";

/// The Credential Manager store. The OS holds the entries, so the struct has no state.
pub struct WindowsCredentialStore;

/// The store, when the Credential Manager answers. A read of an absent probe entry is an
/// answer; any other failure means the manager is not usable.
pub fn open() -> Option<Box<dyn CredentialStore>> {
    match read_blob(PROBE_TARGET) {
        Ok(_) => Some(Box::new(WindowsCredentialStore)),
        Err(_) => None,
    }
}

/// The target for a key name: `<key name>@omairc`, as QtKeychain's `targetName` builds it.
fn target(key_name: &str) -> String {
    format!("{key_name}@{SERVICE}")
}

/// Both targets a key may be stored under, the current one first.
fn targets(key: &CredentialKey) -> [String; 2] {
    let name = key.key_name();
    [target(&name), name]
}

/// A NUL-terminated UTF-16 copy of `text`, as the wide Win32 entry points take it.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(once(0)).collect()
}

/// The blob of the generic credential at `target`. `Ok(None)` when there is no such entry.
fn read_blob(target: &str) -> io::Result<Option<Vec<u8>>> {
    let target = wide(target);
    let mut credential: *mut CREDENTIALW = null_mut();
    // SAFETY: `target` is NUL-terminated and outlives the call. On success `credential` points
    // at a buffer that CredFree releases once its blob has been copied out.
    let found = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) };
    if found == 0 {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(ERROR_NOT_FOUND as i32) {
            Ok(None)
        } else {
            Err(error)
        };
    }
    // SAFETY: CredReadW succeeded, so `credential` is a live CREDENTIALW. Its blob is copied
    // before CredFree, after which nothing points into the freed buffer.
    let blob = unsafe {
        let entry = &*credential;
        let blob = if entry.CredentialBlob.is_null() {
            Vec::new()
        } else {
            std::slice::from_raw_parts(entry.CredentialBlob, entry.CredentialBlobSize as usize)
                .to_vec()
        };
        CredFree(credential as *const c_void);
        blob
    };
    Ok(Some(blob))
}

/// Stores `password` as the generic credential at `target`, replacing any entry there.
fn write_blob(target: &str, password: &[u8]) -> io::Result<()> {
    let mut name = wide(target);
    let mut blob = password.to_vec();
    let entry = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: name.as_mut_ptr(),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_ENTERPRISE,
        ..Default::default()
    };
    // SAFETY: `entry` and the buffers it points at (`name`, `blob`) live across the call, and
    // CredWriteW only reads them.
    if unsafe { CredWriteW(&entry, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Deletes the generic credential at `target`. `Ok(false)` when there was no such entry.
fn delete_blob(target: &str) -> io::Result<bool> {
    let target = wide(target);
    // SAFETY: `target` is NUL-terminated and outlives the call.
    if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } != 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(ERROR_NOT_FOUND as i32) {
        Ok(false)
    } else {
        Err(error)
    }
}

impl CredentialStore for WindowsCredentialStore {
    fn durable(&self) -> bool {
        true
    }

    fn read(&self, key: &CredentialKey) -> Option<String> {
        targets(key)
            .iter()
            .find_map(|target| match read_blob(target) {
                Ok(Some(blob)) if !blob.is_empty() => {
                    Some(String::from_utf8_lossy(&blob).into_owned())
                }
                _ => None,
            })
    }

    fn write(&mut self, key: &CredentialKey, password: &str) -> CredentialState {
        match write_blob(&target(&key.key_name()), password.as_bytes()) {
            Ok(()) => CredentialState::Available,
            Err(error) => CredentialState::Error(error.to_string()),
        }
    }

    fn remove(&mut self, key: &CredentialKey) -> CredentialState {
        for target in targets(key) {
            if let Err(error) = delete_blob(&target) {
                return CredentialState::Error(error.to_string());
            }
        }
        CredentialState::Missing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::credentialstore::PURPOSE_PASSWORD;

    #[test]
    fn the_target_is_the_key_name_with_the_service_suffix() {
        let key = CredentialKey {
            network_id: "nid".to_owned(),
            username: "user".to_owned(),
            host: "irc.example".to_owned(),
            purpose: PURPOSE_PASSWORD,
        };
        assert_eq!(
            targets(&key)[0],
            "omairc/v1/3:nid/4:user/11:irc.example/8:password@omairc"
        );
        assert_eq!(
            targets(&key)[1],
            "omairc/v1/3:nid/4:user/11:irc.example/8:password"
        );
    }

    /// Writes, reads and removes a real Credential Manager entry, and the bare-name fallback.
    /// Run with `cargo test -- --ignored` on Windows.
    #[test]
    #[ignore = "writes to the Windows Credential Manager"]
    fn the_credential_manager_round_trips_a_secret() {
        let key = CredentialKey {
            network_id: format!("omairc-test-{}", std::process::id()),
            username: "test".to_owned(),
            host: "irc.example".to_owned(),
            purpose: PURPOSE_PASSWORD,
        };
        let mut store = open().expect("the Credential Manager answers");
        assert_eq!(store.read(&key), None);
        assert_eq!(store.write(&key, "hunter2"), CredentialState::Available);
        assert_eq!(store.read(&key).as_deref(), Some("hunter2"));
        assert_eq!(store.remove(&key), CredentialState::Missing);
        assert_eq!(store.read(&key), None);

        // An entry under the bare key name, as older QtKeychain builds wrote it, is read and
        // removed the same way.
        write_blob(&key.key_name(), b"legacy").expect("bare entry");
        assert_eq!(store.read(&key).as_deref(), Some("legacy"));
        assert_eq!(store.remove(&key), CredentialState::Missing);
        assert_eq!(store.read(&key), None);
    }
}
