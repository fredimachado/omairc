//! The Windows installer update's file and process half. The release rules and
//! the state machine are in `model::update_check`. This module downloads the
//! setup into the updates directory and verifies it, probes whether the running
//! exe is the Inno-installed copy, and starts the installer. Go twin: none.
//! C++: the file and process parts of `omaircupdatecheck`
//! (`commitDownloadedInstaller`, `downloadedInstallerMatches`,
//! `launchInstaller`, `runningCopyIsInstalled`).

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use url::Url;

use crate::desktop::httpfetch::{get_following, FetchRequest, HttpFetch};
use crate::model::update_check::{
    install_location_matches, windows_setup_name, windows_setup_redirect_is_safe, WindowsSetup,
    INSTALLER_TRANSFER_TIMEOUT, MAXIMUM_REDIRECTS, WINDOWS_UNINSTALL_KEY,
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

/// The directory a download lands in. Qt keeps it under the state root. This
/// build uses [`crate::identity::APP_FOLDER`] so a default build never shares a
/// real install's files.
pub fn default_updates_dir() -> PathBuf {
    dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join(crate::identity::APP_FOLDER)
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

/// The three Uninstall roots Qt walks for [`WINDOWS_UNINSTALL_KEY`], in order.
/// Each is `Software\…\Uninstall\` under HKCU, HKLM, or HKLM WOW6432Node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum UninstallRoot {
    CurrentUser = 0,
    LocalMachine = 1,
    LocalMachineWow64 = 2,
}

/// Roots probed by [`running_copy_is_installed`], matching Qt's `QStringList`.
pub const UNINSTALL_ROOTS: [UninstallRoot; 3] = [
    UninstallRoot::CurrentUser,
    UninstallRoot::LocalMachine,
    UninstallRoot::LocalMachineWow64,
];

impl UninstallRoot {
    /// Registry path under the hive, including the Inno uninstall key.
    fn subkey(self) -> String {
        let prefix = match self {
            UninstallRoot::CurrentUser | UninstallRoot::LocalMachine => {
                r"Software\Microsoft\Windows\CurrentVersion\Uninstall"
            }
            UninstallRoot::LocalMachineWow64 => {
                r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"
            }
        };
        format!(r"{prefix}\{WINDOWS_UNINSTALL_KEY}")
    }
}

/// Reads `InstallLocation` for the Omairc uninstall key under one root. The
/// Windows build opens the real registry; tests supply a fake.
pub trait InstallLocationReader {
    fn install_location(&self, root: UninstallRoot) -> Option<String>;
}

/// Whether the running exe sits in an Inno install location. Qt
/// `runningCopyIsInstalled`: probe each uninstall root and compare
/// `InstallLocation` with the exe's folder. Non-Windows has no uninstall
/// registry, so the platform reader always misses.
pub fn running_copy_is_installed() -> bool {
    let Some(app_dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_string_lossy().into_owned()))
    else {
        return false;
    };
    running_copy_is_installed_with(&app_dir, &PlatformInstallLocationReader)
}

/// Same rules as [`running_copy_is_installed`], with an injected app directory
/// and registry reader so tests can cover the matcher without the real hive.
pub fn running_copy_is_installed_with(app_dir: &str, reader: &dyn InstallLocationReader) -> bool {
    UNINSTALL_ROOTS.iter().any(|&root| {
        reader
            .install_location(root)
            .is_some_and(|location| install_location_matches(&location, app_dir))
    })
}

/// Platform reader: real registry on Windows, always empty elsewhere.
struct PlatformInstallLocationReader;

impl InstallLocationReader for PlatformInstallLocationReader {
    fn install_location(&self, root: UninstallRoot) -> Option<String> {
        #[cfg(windows)]
        {
            read_install_location(root)
        }
        #[cfg(not(windows))]
        {
            let _ = root.subkey();
            None
        }
    }
}

#[cfg(windows)]
fn read_install_location(root: UninstallRoot) -> Option<String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegGetValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ,
    };

    let hive = match root {
        UninstallRoot::CurrentUser => HKEY_CURRENT_USER,
        UninstallRoot::LocalMachine | UninstallRoot::LocalMachineWow64 => HKEY_LOCAL_MACHINE,
    };
    let subkey: Vec<u16> = OsStr::new(&root.subkey())
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let value_name: Vec<u16> = OsStr::new("InstallLocation")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut data = vec![0u16; 1024];
    let mut bytes = (data.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            hive,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value_name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(data.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let len = data.iter().position(|&ch| ch == 0).unwrap_or(data.len());
    Some(String::from_utf16_lossy(&data[..len]))
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
    fn the_updates_directory_is_under_the_app_folder() {
        let dir = default_updates_dir();
        assert!(
            dir.ends_with(Path::new(crate::identity::APP_FOLDER).join("updates")),
            "{dir:?}"
        );
    }

    #[test]
    fn a_non_windows_build_is_never_an_installed_copy() {
        #[cfg(not(windows))]
        assert!(!running_copy_is_installed());
    }

    /// In-memory InstallLocation values for the three Uninstall roots.
    struct FakeInstallLocations {
        locations: [Option<&'static str>; 3],
    }

    impl FakeInstallLocations {
        fn at(root: UninstallRoot, location: &'static str) -> Self {
            let mut locations = [None, None, None];
            locations[root as usize] = Some(location);
            Self { locations }
        }

        fn empty() -> Self {
            Self {
                locations: [None, None, None],
            }
        }
    }

    impl InstallLocationReader for FakeInstallLocations {
        fn install_location(&self, root: UninstallRoot) -> Option<String> {
            self.locations[root as usize].map(str::to_string)
        }
    }

    #[test]
    fn uninstall_subkeys_name_the_shared_inno_key() {
        assert_eq!(
            WINDOWS_UNINSTALL_KEY,
            "{37400F83-8332-4043-8DFF-1F69E51F6555}_is1"
        );
        assert!(UninstallRoot::CurrentUser
            .subkey()
            .ends_with(WINDOWS_UNINSTALL_KEY));
        assert!(UninstallRoot::LocalMachineWow64
            .subkey()
            .contains(r"WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"));
    }

    #[test]
    fn a_matching_install_location_under_hkcu_marks_the_copy_installed() {
        // Mirrors tests/tst_omaircupdatecheck.cpp install-location cases.
        let reader = FakeInstallLocations::at(
            UninstallRoot::CurrentUser,
            "C:/Users/Fredi/AppData/Local/Programs/Omairc/",
        );
        assert!(running_copy_is_installed_with(
            "C:/Users/Fredi/AppData/Local/Programs/Omairc",
            &reader
        ));
    }

    #[test]
    fn a_matching_location_under_hklm_or_wow64_is_enough() {
        let hklm = FakeInstallLocations::at(
            UninstallRoot::LocalMachine,
            r"C:\Users\Fredi\AppData\Local\Programs\Omairc\",
        );
        assert!(running_copy_is_installed_with(
            "C:/Users/Fredi/AppData/Local/Programs/Omairc",
            &hklm
        ));
        let wow = FakeInstallLocations::at(
            UninstallRoot::LocalMachineWow64,
            "C:/Users/Fredi/AppData/Local/Programs/Omairc",
        );
        assert!(running_copy_is_installed_with(
            "C:/Users/Fredi/AppData/Local/Programs/Omairc/",
            &wow
        ));
    }

    #[test]
    fn a_mismatched_or_missing_install_location_is_not_installed() {
        let elsewhere = FakeInstallLocations::at(UninstallRoot::CurrentUser, "C:/elsewhere");
        assert!(!running_copy_is_installed_with("C:/Omairc", &elsewhere));
        assert!(!running_copy_is_installed_with(
            "C:/Omairc",
            &FakeInstallLocations::empty()
        ));
        let blank = FakeInstallLocations::at(UninstallRoot::CurrentUser, "   ");
        assert!(!running_copy_is_installed_with("C:/Omairc", &blank));
    }
}
