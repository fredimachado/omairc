//! The Windows installer update's file and process half. The release rules and
//! the state machine are in `model::update_check`. This module downloads the
//! setup into the updates directory and verifies it, checks that an existing
//! copy still matches, starts the installer, and probes whether the running exe
//! is the Inno-installed copy. Go twin: none. C++: the file and process parts of
//! `src/omaircupdatecheck.cpp` (`commitDownloadedInstaller`,
//! `downloadedInstallerMatches`, `launchInstaller`, `runningCopyIsInstalled`).

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use url::Url;

use crate::desktop::httpfetch::{get_following, FetchRequest, HttpFetch};
#[cfg(windows)]
use crate::model::update_check::WINDOWS_UNINSTALL_KEY;
use crate::model::update_check::{
    install_location_matches, windows_setup_name, windows_setup_redirect_is_safe, WindowsSetup,
    INSTALLER_TRANSFER_TIMEOUT, MAXIMUM_REDIRECTS,
};

/// Starts the setup. A seam so the state machine can run without a process.
pub trait InstallerLauncher {
    /// Starts `installer` detached. False means it did not start.
    fn start(&self, installer: &Path) -> bool;
}

/// The real launcher. On Windows it starts the Inno wizard in its own directory
/// and does not wait for it. Qt `QProcess::startDetached`. Elsewhere the installer
/// does not exist, so the launch is a no-op that reports success, as in Qt.
pub struct ProcessLauncher;

impl InstallerLauncher for ProcessLauncher {
    fn start(&self, installer: &Path) -> bool {
        #[cfg(windows)]
        {
            let directory = installer.parent().map(Path::to_path_buf).unwrap_or_default();
            // Dropping the child does not stop it: the wizard outlives this process.
            std::process::Command::new(installer)
                .current_dir(directory)
                .spawn()
                .is_ok()
        }
        #[cfg(not(windows))]
        {
            let _ = installer;
            true
        }
    }
}

/// The directory a download lands in. Qt keeps it under the state root. The PoC
/// uses `omairc-poc` so it never shares a real install's files.
pub fn default_updates_dir() -> PathBuf {
    dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join("omairc-poc")
        .join("updates")
}

/// Downloads the setup for `version` into `directory` and verifies it. The bytes
/// go to `<name>.partial` and are checked against the digest and size as they
/// arrive. A verified file is renamed into place, and anything else is removed.
/// `on_progress` gets the byte count after each chunk. The error is a reason for
/// the log. Qt `startNetworkDownload` and `commitDownloadedInstaller`.
pub fn download_setup(
    fetch: &dyn HttpFetch,
    version: &str,
    setup: &WindowsSetup,
    directory: &Path,
    on_progress: &mut dyn FnMut(u64),
) -> Result<PathBuf, String> {
    fs::create_dir_all(directory).map_err(|err| err.to_string())?;
    let name = windows_setup_name(version);
    let partial = directory.join(format!("{name}.partial"));
    let final_path = directory.join(&name);
    let _ = fs::remove_file(&partial);
    let mut file = File::create(&partial).map_err(|err| err.to_string())?;

    let mut hasher = Sha256::new();
    let mut written: u64 = 0;
    let mut write_failed = false;
    let request = FetchRequest::get(&setup.url, INSTALLER_TRANSFER_TIMEOUT, setup.size);
    let outcome = get_following(
        fetch,
        request,
        MAXIMUM_REDIRECTS,
        &|url: &Url| windows_setup_redirect_is_safe(url),
        &mut |bytes| {
            // A body longer than the declared size is refused before any byte lands.
            if written + bytes.len() as u64 > setup.size {
                return false;
            }
            if file.write_all(bytes).is_err() {
                write_failed = true;
                return false;
            }
            hasher.update(bytes);
            written += bytes.len() as u64;
            on_progress(written);
            true
        },
    );
    // Close the file before any rename or removal, which Windows requires.
    drop(file);

    let verdict = (|| {
        if write_failed {
            return Err("could not write the installer".to_string());
        }
        let fetched = outcome.map_err(|err| err.to_string())?;
        if fetched.status != 200 {
            return Err(format!("HTTP {}", fetched.status));
        }
        if written != setup.size {
            return Err(format!("expected {} bytes, got {written}", setup.size));
        }
        if hex_lower(&hasher.finalize()) != setup.sha256 {
            return Err("the installer digest does not match the release".to_string());
        }
        let _ = fs::remove_file(&final_path);
        fs::rename(&partial, &final_path).map_err(|err| err.to_string())?;
        set_owner_executable(&final_path);
        Ok(final_path.clone())
    })();
    if verdict.is_err() {
        let _ = fs::remove_file(&partial);
    }
    verdict
}

/// Whether the installer on disk still matches the release: its size and its
/// SHA-256 digest. Qt `downloadedInstallerMatches`, checked before launch.
pub fn installer_matches(path: &Path, setup: &WindowsSetup) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if metadata.len() != setup.size {
        return false;
    }
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 16 * 1024];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => hasher.update(&buffer[..read]),
            Err(_) => return false,
        }
    }
    hex_lower(&hasher.finalize()) == setup.sha256
}

/// Removes a file that a stale download or a failed verification left behind.
pub fn remove_installer(path: &Path) {
    let _ = fs::remove_file(path);
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(unix)]
fn set_owner_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
fn set_owner_executable(_path: &Path) {}

/// Whether the running exe sits in the Inno install location. The uninstall
/// entry is read from HKCU, then HKLM, then the WOW6432Node view of HKLM. Qt
/// `runningCopyIsInstalled`. Off Windows there is no installed copy.
pub fn running_copy_is_installed() -> bool {
    let Some(app_dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_string_lossy().into_owned()))
    else {
        return false;
    };
    install_locations()
        .iter()
        .any(|location| install_location_matches(location, &app_dir))
}

#[cfg(windows)]
fn install_locations() -> Vec<String> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::RegKey;
    let entry = format!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{WINDOWS_UNINSTALL_KEY}");
    let wow_entry = format!(
        r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\{WINDOWS_UNINSTALL_KEY}"
    );
    [
        (HKEY_CURRENT_USER, entry.as_str()),
        (HKEY_LOCAL_MACHINE, entry.as_str()),
        (HKEY_LOCAL_MACHINE, wow_entry.as_str()),
    ]
    .into_iter()
    .filter_map(|(hive, path)| {
        RegKey::predef(hive)
            .open_subkey_with_flags(path, KEY_READ)
            .ok()
    })
    .filter_map(|key| key.get_value::<String, _>("InstallLocation").ok())
    .collect()
}

#[cfg(not(windows))]
fn install_locations() -> Vec<String> {
    Vec::new()
}

/// Records each start and reports the result it was told to return, so tests
/// can check a launch without spawning a process.
#[cfg(test)]
pub struct RecordingLauncher {
    result: bool,
    started: std::sync::Mutex<Vec<PathBuf>>,
}

#[cfg(test)]
impl RecordingLauncher {
    pub fn new(result: bool) -> Self {
        Self {
            result,
            started: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn started(&self) -> Vec<PathBuf> {
        self.started.lock().unwrap().clone()
    }
}

#[cfg(test)]
impl InstallerLauncher for RecordingLauncher {
    fn start(&self, installer: &Path) -> bool {
        self.started.lock().unwrap().push(installer.to_path_buf());
        self.result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::httpfetch::FakeFetch;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    const VERSION: &str = "9.9.9";
    const URL: &str =
        "https://github.com/fredimachado/omairc/releases/download/v9.9.9/omairc-9.9.9-windows-x64-setup.exe";
    const BODY: &[u8] = b"omairc-setup";

    /// A fresh scratch directory, removed when dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "omairc-installer-test-{}-{nanos}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("scratch dir");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// The setup the release declares for `body`, digest and size included.
    fn setup_for(body: &[u8]) -> WindowsSetup {
        WindowsSetup {
            url: URL.to_string(),
            sha256: hex_lower(&Sha256::digest(body)),
            size: body.len() as u64,
        }
    }

    fn download(fake: &FakeFetch, setup: &WindowsSetup, dir: &Path) -> Result<PathBuf, String> {
        download_setup(fake, VERSION, setup, dir, &mut |_| {})
    }

    #[test]
    fn a_verified_download_is_renamed_into_place() {
        let scratch = Scratch::new();
        let fake = FakeFetch::new();
        fake.serve(URL, 200, BODY);
        let setup = setup_for(BODY);
        let mut progress = Vec::new();
        let path = download_setup(&fake, VERSION, &setup, scratch.path(), &mut |n| {
            progress.push(n)
        })
        .expect("verified");
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some("omairc-9.9.9-windows-x64-setup.exe")
        );
        assert_eq!(fs::read(&path).expect("read"), BODY);
        assert_eq!(progress.last(), Some(&(BODY.len() as u64)));
        assert!(!scratch.path().join("omairc-9.9.9-windows-x64-setup.exe.partial").exists());
        assert!(installer_matches(&path, &setup));
    }

    #[test]
    fn a_bad_digest_removes_the_partial_and_leaves_no_installer() {
        let scratch = Scratch::new();
        let fake = FakeFetch::new();
        // Same length as BODY, so only the digest can reject it.
        fake.serve(URL, 200, b"OMAIRC-SETUP");
        let setup = setup_for(BODY);
        let err = download(&fake, &setup, scratch.path()).expect_err("digest mismatch");
        assert!(err.contains("digest"), "{err}");
        assert!(!scratch.path().join("omairc-9.9.9-windows-x64-setup.exe").exists());
        assert!(!scratch.path().join("omairc-9.9.9-windows-x64-setup.exe.partial").exists());
    }

    #[test]
    fn a_body_over_the_declared_size_is_refused() {
        let scratch = Scratch::new();
        let fake = FakeFetch::new();
        fake.serve(URL, 200, b"omairc-setup-but-longer");
        let mut setup = setup_for(BODY);
        setup.size = 4;
        assert!(download(&fake, &setup, scratch.path()).is_err());
        assert!(!scratch.path().join("omairc-9.9.9-windows-x64-setup.exe").exists());
    }

    #[test]
    fn a_short_body_is_refused() {
        let scratch = Scratch::new();
        let fake = FakeFetch::new();
        fake.serve(URL, 200, b"omairc");
        let setup = setup_for(BODY);
        let err = download(&fake, &setup, scratch.path()).expect_err("short body");
        assert!(err.contains("expected"), "{err}");
    }

    #[test]
    fn an_unsafe_redirect_is_never_requested() {
        let scratch = Scratch::new();
        let fake = FakeFetch::new();
        fake.redirect(URL, 302, "https://evil.example/setup.exe");
        fake.serve("https://evil.example/setup.exe", 200, BODY);
        let err = download(&fake, &setup_for(BODY), scratch.path()).expect_err("refused");
        assert!(err.contains("refused"), "{err}");
        assert_eq!(fake.requests().len(), 1, "the evil host was never requested");
        assert!(!scratch.path().join("omairc-9.9.9-windows-x64-setup.exe").exists());
    }

    #[test]
    fn a_github_redirect_to_its_object_host_is_followed() {
        let scratch = Scratch::new();
        let fake = FakeFetch::new();
        let object = "https://objects.githubusercontent.com/releases/setup.exe";
        fake.redirect(URL, 302, object);
        fake.serve(object, 200, BODY);
        let setup = setup_for(BODY);
        let path = download(&fake, &setup, scratch.path()).expect("followed");
        assert!(installer_matches(&path, &setup));
    }

    #[test]
    fn a_non_200_success_is_refused_like_qt() {
        let scratch = Scratch::new();
        let fake = FakeFetch::new();
        fake.serve(URL, 204, BODY);
        let err = download(&fake, &setup_for(BODY), scratch.path()).expect_err("204");
        assert!(err.contains("204"), "{err}");
    }

    #[test]
    fn a_tampered_or_resized_installer_no_longer_matches() {
        let scratch = Scratch::new();
        let fake = FakeFetch::new();
        fake.serve(URL, 200, BODY);
        let setup = setup_for(BODY);
        let path = download(&fake, &setup, scratch.path()).expect("verified");
        fs::write(&path, b"omairc-SETUP").expect("tamper");
        assert!(!installer_matches(&path, &setup), "same size, different bytes");
        fs::write(&path, b"short").expect("resize");
        assert!(!installer_matches(&path, &setup), "different size");
        assert!(!installer_matches(&scratch.path().join("missing.exe"), &setup));
    }

    #[test]
    fn remove_installer_ignores_a_missing_file() {
        let scratch = Scratch::new();
        remove_installer(&scratch.path().join("missing.exe"));
    }

    #[test]
    fn the_recording_launcher_reports_what_it_was_asked() {
        let launcher = RecordingLauncher::new(false);
        assert!(!launcher.start(Path::new("/tmp/setup.exe")));
        assert_eq!(launcher.started(), vec![PathBuf::from("/tmp/setup.exe")]);
    }

    #[test]
    fn the_updates_directory_is_under_omairc_poc() {
        let dir = default_updates_dir();
        assert!(dir.ends_with(Path::new("omairc-poc").join("updates")), "{dir:?}");
    }
}
