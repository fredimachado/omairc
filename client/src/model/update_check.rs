//! Check for Updates and the Windows installer update.
//! Update check: reads the latest GitHub release, orders its
//! version against the running one, and words the status line as Qt does. A
//! portable copy opens the release page. An installed copy with a verified
//! setup asset downloads it in the background and offers `Restart to update`.
//! This module is state and policy only. The network goes through the
//! `HttpFetch` seam and the installer file and process go through
//! `desktop::installer`, so every transition here is testable without a socket.
//! Callers keep the same order as Qt: `begin_*`, run the background work, then
//! `finish_*`. A ticket from an earlier generation is ignored.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use url::Url;

use crate::desktop::httpfetch::{get_following, FetchRequest, HttpFetch};
use crate::desktop::installer::InstallerLauncher;

/// Qt `omaircGithubRepoUrl`.
pub const REPO_URL: &str = "https://github.com/fredimachado/omairc";
/// Qt `omaircGithubLatestReleaseApiUrl`.
pub const LATEST_RELEASE_API_URL: &str =
    "https://api.github.com/repos/fredimachado/omairc/releases/latest";
/// The page used when the reply has no `html_url`, as Qt's parser falls back.
const LATEST_RELEASE_PAGE_URL: &str = "https://github.com/fredimachado/omairc/releases/latest";
/// The only path a release page may sit under.
const REPO_PATH: &str = "/fredimachado/omairc";
/// Qt `kUpdateCheckTimeoutMs`.
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(10);
/// Qt `kInstallerTransferTimeoutMs`.
pub const INSTALLER_TRANSFER_TIMEOUT: Duration = Duration::from_secs(60);
/// Qt `kMaximumReleaseBytes`. A longer reply is a failed check.
pub const MAXIMUM_RELEASE_BYTES: u64 = 256 * 1024;
/// Qt `kMaximumSetupBytes`. An asset that declares more is not offered.
pub const MAXIMUM_SETUP_BYTES: u64 = 80 * 1024 * 1024;
/// Redirects a download may follow, one hop at a time.
pub const MAXIMUM_REDIRECTS: u32 = 5;
/// Inno uninstall key shared with the Windows setup (`AppId`). Qt
/// `kWindowsUninstallKey`. Probed by `desktop::installer::running_copy_is_installed`.
pub const WINDOWS_UNINSTALL_KEY: &str = "{37400F83-8332-4043-8DFF-1F69E51F6555}_is1";

/// The latest release, reduced to what the sheet and the installer need.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// The tag without its leading `v`.
    pub version: String,
    pub tag: String,
    /// The release page. Always on the repository.
    pub html_url: String,
    /// The Windows setup asset, when exactly one verified asset matches.
    pub setup: Option<WindowsSetup>,
}

/// The `omairc-<v>-windows-x64-setup.exe` asset GitHub publishes, with the
/// digest and size it declares. A download must match both.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowsSetup {
    pub url: String,
    /// The lowercase hex SHA-256 from the asset's `sha256:` digest.
    pub sha256: String,
    pub size: u64,
}

/// The update state the status line and the footer read. Qt
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateStatus {
    /// No check has run, so the status line is hidden.
    Idle,
    Checking,
    UpToDate,
    /// A newer release exists. Its page or setup is on `UpdateCheck::latest`.
    Available,
    /// The setup is downloading. `written` of `total` bytes have arrived.
    Downloading { written: u64, total: u64 },
    /// The setup is verified and waiting to run.
    ReadyToRestart,
    DownloadFailed,
    Failed,
}

/// What clicking the status line does. Qt `activateUpdateStatus`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StatusAction {
    None,
    /// Open the release page in the browser.
    OpenPage(String),
    /// Start the background download of the verified setup.
    Download,
    /// Start the installer.
    Restart,
}

/// What a `launch_installer` call did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchOutcome {
    /// The installer process was started, or the test latch counted it.
    Started,
    /// The installer already started in this session, so nothing ran again.
    AlreadyStarted,
    /// No installer is ready to run.
    NotReady,
    /// The downloaded file no longer matches the release. The download is
    /// marked failed, and the caller removes the file.
    VerificationFailed,
    /// The installer process could not be started.
    SpawnFailed,
}

/// A check in flight. A reply from an older check is ignored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckTicket {
    generation: u64,
}

/// A download in flight. Progress and the result are accepted only while
/// this ticket is the current download.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadTicket {
    generation: u64,
    pub version: String,
    pub setup: WindowsSetup,
}

/// The state behind the About sheet and the footer label. Qt
/// `OmaircUpdateCheck`, without the Qt properties and the file I/O.
#[derive(Clone, Debug)]
pub struct UpdateCheck {
    current: String,
    installed_copy: bool,
    status: UpdateStatus,
    latest: Option<Release>,
    installer_path: Option<PathBuf>,
    launch_attempts: u32,
    launched: bool,
    launch_process: bool,
    generation: u64,
}

impl UpdateCheck {
    /// `current` is the running version. `installed_copy` says the running exe
    /// sits in the Inno install location, so only that copy may self-update.
    pub fn new(current: &str, installed_copy: bool) -> Self {
        Self {
            current: current.to_string(),
            installed_copy,
            status: UpdateStatus::Idle,
            latest: None,
            installer_path: None,
            launch_attempts: 0,
            launched: false,
            launch_process: true,
            generation: 0,
        }
    }

    pub fn current(&self) -> &str {
        &self.current
    }

    #[cfg(test)]
    pub fn status(&self) -> UpdateStatus {
        self.status
    }

    #[cfg(test)]
    pub fn latest(&self) -> Option<&Release> {
        self.latest.as_ref()
    }

    /// The verified installer on disk, once a download has committed.
    pub fn installer_path(&self) -> Option<&Path> {
        self.installer_path.as_deref()
    }

    /// Installer launches attempted this session. Qt `launchAttempts`.
    #[cfg(test)]
    pub fn launch_attempts(&self) -> u32 {
        self.launch_attempts
    }

    /// Whether the footer offers `Restart to update`.
    pub fn restart_ready(&self) -> bool {
        self.status == UpdateStatus::ReadyToRestart
    }

    /// An installed copy with a setup asset can download and restart. Qt
    pub fn installer_update_available(&self) -> bool {
        self.installed_copy && self.latest.as_ref().is_some_and(|r| r.setup.is_some())
    }

    /// The status line text, or `None` when the line is hidden. Qt
    pub fn message(&self) -> Option<String> {
        Some(match self.status {
            UpdateStatus::Idle => return None,
            UpdateStatus::Checking => "Checking…".to_string(),
            UpdateStatus::UpToDate => "Omairc is up to date.".to_string(),
            UpdateStatus::Available => match self.latest.as_ref() {
                Some(release) => format!("Version {} is available.", release.version),
                None => "A newer version is available.".to_string(),
            },
            UpdateStatus::Downloading { written, total } => {
                if total == 0 || written == 0 {
                    "Downloading update…".to_string()
                } else {
                    let percent = (written.saturating_mul(100) / total).min(99);
                    format!("Downloading update… {percent}%")
                }
            }
            UpdateStatus::ReadyToRestart => "Restart to update".to_string(),
            UpdateStatus::DownloadFailed => "Could not download the update.".to_string(),
            UpdateStatus::Failed => "Could not check for updates.".to_string(),
        })
    }

    /// What clicking the status line does. A portable copy, or a release with no
    /// verified setup, keeps the release page link. Qt `activateUpdateStatus`.
    pub fn action(&self) -> StatusAction {
        match self.status {
            UpdateStatus::Available | UpdateStatus::DownloadFailed => {
                if self.installer_update_available() {
                    StatusAction::Download
                } else {
                    match self.latest.as_ref() {
                        Some(release) => StatusAction::OpenPage(release.html_url.clone()),
                        None => StatusAction::None,
                    }
                }
            }
            UpdateStatus::ReadyToRestart => StatusAction::Restart,
            _ => StatusAction::None,
        }
    }

    /// Starts a check. Returns `None` while a check or a download is running, since
    /// the About sheet disables Check for Updates then. Qt `check`.
    /// A check forgets the downloaded installer, as Qt does, and the caller
    /// removes that file. The path is read before this call.
    pub fn begin_check(&mut self) -> Option<CheckTicket> {
        if matches!(
            self.status,
            UpdateStatus::Checking | UpdateStatus::Downloading { .. }
        ) {
            return None;
        }
        self.generation += 1;
        self.latest = None;
        self.installer_path = None;
        self.status = UpdateStatus::Checking;
        Some(CheckTicket {
            generation: self.generation,
        })
    }

    /// Applies a check reply. `None` is a failed check. The reply is ignored
    /// unless it is for the running check. Qt `finishWithBody` and
    pub fn finish_check(&mut self, ticket: CheckTicket, release: Option<Release>) -> bool {
        if !self.current_check(ticket) {
            return false;
        }
        match release {
            None => {
                self.latest = None;
                self.status = UpdateStatus::Failed;
            }
            Some(release) => {
                self.status = match compare_versions(&self.current, &release.version) {
                    Ordering::Less => UpdateStatus::Available,
                    Ordering::Equal | Ordering::Greater => UpdateStatus::UpToDate,
                };
                self.latest = Some(release);
            }
        }
        true
    }

    /// Cancels a running check, as closing the About sheet does. Qt `cancel`.
    pub fn cancel_check(&mut self) {
        if self.status != UpdateStatus::Checking {
            return;
        }
        self.generation += 1;
        self.status = UpdateStatus::Idle;
    }

    /// Starts the setup download. Only an installed copy with a verified asset
    /// may start one, and only from `Available` or `DownloadFailed`. Qt `download`.
    pub fn start_download(&mut self) -> Option<DownloadTicket> {
        if !self.installer_update_available() {
            return None;
        }
        if !matches!(
            self.status,
            UpdateStatus::Available | UpdateStatus::DownloadFailed
        ) {
            return None;
        }
        let release = self.latest.as_ref()?;
        let setup = release.setup.clone()?;
        self.generation += 1;
        self.status = UpdateStatus::Downloading {
            written: 0,
            total: setup.size,
        };
        Some(DownloadTicket {
            generation: self.generation,
            version: release.version.clone(),
            setup,
        })
    }

    /// Records bytes that have arrived. Returns false for a stale ticket.
    pub fn report_progress(&mut self, ticket: &DownloadTicket, written: u64) -> bool {
        if !self.current_download(ticket) {
            return false;
        }
        if let UpdateStatus::Downloading { total, .. } = self.status {
            self.status = UpdateStatus::Downloading {
                written: written.min(total),
                total,
            };
        }
        true
    }

    /// Records the download result. `Some(path)` is a verified installer and
    /// `None` is a failure. Returns false for a stale ticket, and the caller then
    /// removes the file it wrote. Qt `commitDownloadedInstaller`.
    pub fn commit_download(&mut self, ticket: &DownloadTicket, installer: Option<PathBuf>) -> bool {
        if !self.current_download(ticket) {
            return false;
        }
        match installer {
            Some(path) => {
                self.installer_path = Some(path);
                self.status = UpdateStatus::ReadyToRestart;
            }
            None => {
                self.installer_path = None;
                self.status = UpdateStatus::DownloadFailed;
            }
        }
        true
    }

    /// Makes launches count without starting a process, so tests never run an
    /// installer. Qt `suppressInstallerLaunch`.
    #[cfg(test)]
    pub fn suppress_installer_launch(&mut self) {
        self.launch_process = false;
    }

    /// Starts the verified installer once per session. `verify` checks the
    /// downloaded file against the release digest and size. A successful spawn
    /// latches, so a later click or window close does nothing. Qt
    pub fn launch_installer(
        &mut self,
        launcher: &dyn InstallerLauncher,
        verify: &dyn Fn(&Path, &WindowsSetup) -> bool,
    ) -> LaunchOutcome {
        if self.status != UpdateStatus::ReadyToRestart {
            return LaunchOutcome::NotReady;
        }
        if self.launched {
            return LaunchOutcome::AlreadyStarted;
        }
        let (Some(path), Some(setup)) = (
            self.installer_path.clone(),
            self.latest.as_ref().and_then(|r| r.setup.clone()),
        ) else {
            return LaunchOutcome::NotReady;
        };
        if !verify(&path, &setup) {
            self.installer_path = None;
            self.status = UpdateStatus::DownloadFailed;
            return LaunchOutcome::VerificationFailed;
        }
        self.launch_attempts += 1;
        if !self.launch_process || launcher.start(&path) {
            self.launched = true;
            return LaunchOutcome::Started;
        }
        LaunchOutcome::SpawnFailed
    }

    /// The window-close hook: launches on the first close only, as Qt's
    /// `onClosing` does. `None` means the close did not try to launch.
    pub fn launch_on_close(
        &mut self,
        launcher: &dyn InstallerLauncher,
        verify: &dyn Fn(&Path, &WindowsSetup) -> bool,
    ) -> Option<LaunchOutcome> {
        if !self.restart_ready() || self.launch_attempts != 0 {
            return None;
        }
        Some(self.launch_installer(launcher, verify))
    }

    fn current_check(&self, ticket: CheckTicket) -> bool {
        ticket.generation == self.generation && self.status == UpdateStatus::Checking
    }

    fn current_download(&self, ticket: &DownloadTicket) -> bool {
        ticket.generation == self.generation
            && matches!(self.status, UpdateStatus::Downloading { .. })
    }
}

/// Qt `omaircUserAgent`, sent with every request.
pub fn user_agent(current: &str) -> String {
    format!("Omairc/{current} (+{REPO_URL})")
}

/// Asks GitHub for the latest release. This blocks, so the caller runs it on the
/// background executor. A transport error, a redirect the policy refuses, a
/// non-200 reply, an oversized body, or a reply that does not parse is `None`.
/// Qt `OmaircUpdateCheck::check`.
pub fn check_latest(fetch: &dyn HttpFetch, current: &str, api_url: &str) -> Option<Release> {
    let request = FetchRequest::get(api_url, CHECK_TIMEOUT, MAXIMUM_RELEASE_BYTES)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", &user_agent(current));
    let mut body = Vec::new();
    let fetched = get_following(
        fetch,
        request,
        MAXIMUM_REDIRECTS,
        &|url: &Url| url.scheme() == "https",
        &mut |bytes| {
            if body.len() as u64 + bytes.len() as u64 > MAXIMUM_RELEASE_BYTES {
                return false;
            }
            body.extend_from_slice(bytes);
            true
        },
    )
    .ok()?;
    release_from_reply(fetched.status, &body)
}

/// Judges one reply. Only a 200 that carries a release says anything.
pub fn release_from_reply(status: u16, body: &[u8]) -> Option<Release> {
    if status != 200 {
        return None;
    }
    parse_release(std::str::from_utf8(body).ok()?)
}

/// Reads the release JSON that GitHub returns. `None` when the body is not an
/// object, has no tag, or its page is not on the repository. The setup asset is
/// `None` unless exactly one asset passes every check.
pub fn parse_release(body: &str) -> Option<Release> {
    let value: Value = serde_json::from_str(body).ok()?;
    let object = value.as_object()?;
    let tag = object.get("tag_name")?.as_str()?.trim();
    let version = strip_v(tag);
    if version.is_empty() {
        return None;
    }
    let html_url = object
        .get("html_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .unwrap_or(LATEST_RELEASE_PAGE_URL);
    if !release_url_is_safe(html_url) {
        return None;
    }
    let setup = if release_token_is_safe(tag) && release_token_is_safe(version) {
        object
            .get("assets")
            .and_then(Value::as_array)
            .and_then(|assets| windows_setup_for(assets, tag, version))
    } else {
        None
    };
    Some(Release {
        version: version.to_string(),
        tag: tag.to_string(),
        html_url: html_url.to_string(),
        setup,
    })
}

/// The one asset named `omairc-<version>-windows-x64-setup.exe` whose URL, digest
/// and size pass. Two matches are ambiguous, so neither is offered. Qt
fn windows_setup_for(assets: &[Value], tag: &str, version: &str) -> Option<WindowsSetup> {
    let expected = windows_setup_name(version);
    let mut found = None;
    let mut matches = 0;
    for asset in assets {
        let Some(asset) = asset.as_object() else {
            continue;
        };
        if asset.get("name").and_then(Value::as_str) != Some(expected.as_str()) {
            continue;
        }
        let Some(sha256) = asset
            .get("digest")
            .and_then(Value::as_str)
            .and_then(|digest| parse_sha256_digest(digest.trim()))
        else {
            continue;
        };
        let Some(raw_size) = asset.get("size").and_then(Value::as_f64) else {
            continue;
        };
        if raw_size.fract() != 0.0 || raw_size <= 0.0 || raw_size > MAXIMUM_SETUP_BYTES as f64 {
            continue;
        }
        let url_text = asset
            .get("browser_download_url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let Ok(download) = Url::parse(url_text) else {
            continue;
        };
        if !windows_setup_request_is_safe(&download, tag, version) {
            continue;
        }
        matches += 1;
        found = Some(WindowsSetup {
            url: download.to_string(),
            sha256,
            size: raw_size as u64,
        });
    }
    if matches == 1 {
        found
    } else {
        None
    }
}

/// `omairc-<version>-windows-x64-setup.exe`. Qt `omaircExpectedWindowsSetupName`.
pub fn windows_setup_name(version: &str) -> String {
    format!("omairc-{version}-windows-x64-setup.exe")
}

/// A tag or version may only hold letters, digits, `.`, `_` and `-`, at most 64
/// bytes. The download path is built from them, so nothing else may reach it.
pub fn release_token_is_safe(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 64
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// The setup download must be the release's own asset path on github.com, over
/// https, with no query, fragment or userinfo. Qt `omaircWindowsSetupRequestIsSafe`.
pub fn windows_setup_request_is_safe(url: &Url, tag: &str, version: &str) -> bool {
    if !release_token_is_safe(tag) || !release_token_is_safe(version) {
        return false;
    }
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return false;
    }
    url.path()
        == format!(
            "{REPO_PATH}/releases/download/{tag}/{}",
            windows_setup_name(version)
        )
}

/// Where a redirect from the download may land: github.com or githubusercontent.com
/// and their subdomains, over https, with no userinfo. Qt
pub fn windows_setup_redirect_is_safe(url: &Url) -> bool {
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    host == "github.com"
        || host == "githubusercontent.com"
        || host.ends_with(".github.com")
        || host.ends_with(".githubusercontent.com")
}

/// The digest GitHub publishes, `sha256:<64 hex>`, returned as lowercase hex.
pub fn parse_sha256_digest(digest: &str) -> Option<String> {
    let hex = digest.strip_prefix("sha256:")?.to_ascii_lowercase();
    (hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(hex)
}

/// Whether an install location names the running app directory. Slashes and a
/// trailing separator are ignored, and Windows compares case-insensitively. Qt
pub fn install_location_matches(install_location: &str, app_dir: &str) -> bool {
    let left = normalize_install_dir(install_location);
    let right = normalize_install_dir(app_dir);
    if left.is_empty() || right.is_empty() {
        return false;
    }
    if cfg!(windows) {
        left.eq_ignore_ascii_case(&right)
    } else {
        left == right
    }
}

fn normalize_install_dir(path: &str) -> String {
    path.trim().replace('\\', "/").trim_end_matches('/').to_string()
}

/// A release page must be on `github.com` under the repository, over https. The
/// host is matched without userinfo or a port, so anything odd fails closed.
fn release_url_is_safe(url: &str) -> bool {
    let Some(rest) = strip_https(url) else {
        return false;
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(authority_end);
    if !authority.eq_ignore_ascii_case("github.com") {
        return false;
    }
    let path = tail.split(['?', '#']).next().unwrap_or("");
    path == REPO_PATH
        || path
            .strip_prefix(REPO_PATH)
            .is_some_and(|after| after.starts_with('/'))
}

fn strip_https(url: &str) -> Option<&str> {
    let scheme = url.get(.."https://".len())?;
    scheme
        .eq_ignore_ascii_case("https://")
        .then(|| &url[scheme.len()..])
}

fn strip_v(tag: &str) -> &str {
    tag.strip_prefix(['v', 'V']).unwrap_or(tag)
}

/// Orders two version strings the way Qt's `omaircCompareVersions` does: the
/// numeric parts first, missing parts count as zero, then the suffix. An empty
/// suffix (a release) outranks alpha, beta and rc, and any other suffix sorts
/// lowest. A leading `v` and a `-` or `+` before the suffix are ignored.
pub fn compare_versions(left: &str, right: &str) -> Ordering {
    let a = parse_version(left);
    let b = parse_version(right);
    let count = a.numeric.len().max(b.numeric.len());
    for index in 0..count {
        let av = a.numeric.get(index).copied().unwrap_or(0);
        let bv = b.numeric.get(index).copied().unwrap_or(0);
        if av != bv {
            return av.cmp(&bv);
        }
    }
    suffix_rank(&a.suffix)
        .cmp(&suffix_rank(&b.suffix))
        .then_with(|| a.suffix.cmp(&b.suffix))
}

struct Parsed {
    numeric: Vec<u64>,
    suffix: String,
}

fn parse_version(text: &str) -> Parsed {
    let text = strip_v(text.trim());
    let bytes = text.as_bytes();
    let mut numeric = Vec::new();
    let mut index = 0;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        let mut value: u64 = 0;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            value = value
                .saturating_mul(10)
                .saturating_add(u64::from(bytes[index] - b'0'));
            index += 1;
        }
        numeric.push(value);
        if index < bytes.len() && bytes[index] == b'.' {
            index += 1;
            continue;
        }
        break;
    }
    // Only ASCII bytes were consumed, so `index` is a character boundary.
    let mut suffix = text[index..].to_lowercase();
    if suffix.starts_with(['-', '+']) {
        suffix.remove(0);
    }
    Parsed { numeric, suffix }
}

fn suffix_rank(suffix: &str) -> u8 {
    if suffix.is_empty() {
        100
    } else if suffix.starts_with("alpha") {
        10
    } else if suffix.starts_with("beta") {
        20
    } else if suffix.starts_with("rc") {
        30
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::httpfetch::{serve_once, FakeFetch};
    use crate::desktop::installer::RecordingLauncher;
    use std::cell::RefCell;

    const SETUP_NAME: &str = "omairc-9.9.9-windows-x64-setup.exe";
    const SETUP_URL: &str =
        "https://github.com/fredimachado/omairc/releases/download/v9.9.9/omairc-9.9.9-windows-x64-setup.exe";
    /// The digest and size the fixture declares. Its body is the 12 bytes "omairc-setup".
    const SETUP_DIGEST: &str =
        "sha256:d323de13bbb0973b891849578f74c69cb4ef85fc37ffeb05b31d835f708c0ae9";

    /// A trimmed `releases/latest` reply, shaped as GitHub sends it.
    const LATEST_RELEASE: &str = r#"{
        "url": "https://api.github.com/repos/fredimachado/omairc/releases/123",
        "html_url": "https://github.com/fredimachado/omairc/releases/tag/v1.11.0",
        "tag_name": "v1.11.0",
        "name": "Omairc 1.11.0",
        "draft": false,
        "prerelease": false,
        "published_at": "2026-10-10T09:00:00Z",
        "assets": [
            {
                "name": "omairc-1.11.0-windows-x64-setup.exe",
                "size": 12345678,
                "browser_download_url": "https://github.com/fredimachado/omairc/releases/download/v1.11.0/omairc-1.11.0-windows-x64-setup.exe"
            }
        ]
    }"#;

    fn release_json(tag: &str, html_url: &str) -> String {
        format!(r#"{{"tag_name":"{tag}","html_url":"{html_url}"}}"#)
    }

    fn setup_release_json(download_url: &str, digest: &str, size: &str) -> String {
        format!(
            r#"{{"tag_name":"v9.9.9","html_url":"https://github.com/fredimachado/omairc/releases/tag/v9.9.9","assets":[{{"name":"{SETUP_NAME}","browser_download_url":"{download_url}","size":{size},"digest":"{digest}"}}]}}"#
        )
    }

    /// A release that offers the verified setup, as an installed copy sees it.
    fn setup_release() -> Release {
        parse_release(&setup_release_json(SETUP_URL, SETUP_DIGEST, "12")).expect("parses")
    }

    fn verify_always(_: &Path, _: &WindowsSetup) -> bool {
        true
    }

    /// A check that finished with `release` on an installed or portable copy.
    fn checked(installed: bool, release: Release) -> UpdateCheck {
        let mut state = UpdateCheck::new("0.1.0", installed);
        let ticket = state.begin_check().expect("first check");
        assert!(state.finish_check(ticket, Some(release)));
        state
    }

    #[test]
    fn compares_versions_as_qt_does() {
        use Ordering::{Equal, Greater, Less};
        assert_eq!(compare_versions("0.7.0", "0.7.0"), Equal);
        assert_eq!(compare_versions("v0.7.0", "0.7.0"), Equal);
        assert_eq!(compare_versions("0.7", "0.7.0"), Equal);
        assert_eq!(compare_versions("0.7.0alpha", "0.7.0"), Less);
        assert_eq!(compare_versions("0.7.0", "0.7.0alpha"), Greater);
        assert_eq!(compare_versions("0.7.0alpha", "0.7.0beta"), Less);
        assert_eq!(compare_versions("0.7.0rc1", "0.7.0beta"), Greater);
        assert_eq!(compare_versions("0.7.0", "0.8.0"), Less);
        assert_eq!(compare_versions("0.8.0", "0.7.9"), Greater);
        // Numeric parts compare as numbers, not text.
        assert_eq!(compare_versions("1.10.3", "1.9.9"), Greater);
        assert_eq!(compare_versions("1.10.3", "1.10.3-beta.1"), Greater);
    }

    #[test]
    fn parses_the_latest_release_fixture() {
        let release = parse_release(LATEST_RELEASE).expect("fixture parses");
        assert_eq!(release.version, "1.11.0");
        assert_eq!(release.tag, "v1.11.0");
        assert_eq!(
            release.html_url,
            "https://github.com/fredimachado/omairc/releases/tag/v1.11.0"
        );
        assert_eq!(release.setup, None, "an asset with no digest offers no installer");
    }

    /// GitHub still publishes `omairc-<v>-windows-x64-setup.exe` after the Rust
    /// cutover, so a Qt install can download and upgrade. The release JSON shape
    /// (name, digest, size, browser_download_url) must stay accepted.
    #[test]
    fn github_release_json_accepts_the_windows_setup_asset_name() {
        let body = r#"{
            "url": "https://api.github.com/repos/fredimachado/omairc/releases/99",
            "html_url": "https://github.com/fredimachado/omairc/releases/tag/v1.0.4",
            "tag_name": "v1.0.4",
            "name": "Omairc 1.0.4",
            "draft": false,
            "prerelease": false,
            "published_at": "2026-10-20T12:00:00Z",
            "assets": [
                {
                    "name": "omairc-1.0.4-macos-arm64.zip",
                    "size": 11111111,
                    "digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
                    "browser_download_url": "https://github.com/fredimachado/omairc/releases/download/v1.0.4/omairc-1.0.4-macos-arm64.zip"
                },
                {
                    "name": "omairc-1.0.4-windows-x64-setup.exe",
                    "size": 22222222,
                    "digest": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
                    "browser_download_url": "https://github.com/fredimachado/omairc/releases/download/v1.0.4/omairc-1.0.4-windows-x64-setup.exe"
                },
                {
                    "name": "omairc-1.0.4-windows-x64.zip",
                    "size": 33333333,
                    "digest": "sha256:3333333333333333333333333333333333333333333333333333333333333333",
                    "browser_download_url": "https://github.com/fredimachado/omairc/releases/download/v1.0.4/omairc-1.0.4-windows-x64.zip"
                }
            ]
        }"#;
        let release = parse_release(body).expect("release JSON parses");
        assert_eq!(release.version, "1.0.4");
        assert_eq!(release.tag, "v1.0.4");
        assert_eq!(windows_setup_name(&release.version), "omairc-1.0.4-windows-x64-setup.exe");
        let setup = release.setup.as_ref().expect("the windows-x64-setup asset is offered");
        assert_eq!(
            setup.url,
            "https://github.com/fredimachado/omairc/releases/download/v1.0.4/omairc-1.0.4-windows-x64-setup.exe"
        );
        assert_eq!(
            setup.sha256,
            "2222222222222222222222222222222222222222222222222222222222222222"
        );
        assert_eq!(setup.size, 22_222_222);

        let mut state = UpdateCheck::new("1.0.3", true);
        let ticket = state.begin_check().expect("check starts");
        assert!(state.finish_check(ticket, Some(release)));
        assert!(state.installer_update_available());
        assert_eq!(state.action(), StatusAction::Download);
    }

    #[test]
    fn falls_back_to_the_latest_page_without_html_url() {
        let release = parse_release(r#"{"tag_name":"v1.0.0"}"#).expect("tag is enough");
        assert_eq!(release.version, "1.0.0");
        assert_eq!(
            release.html_url,
            "https://github.com/fredimachado/omairc/releases/latest"
        );
    }

    #[test]
    fn rejects_replies_that_are_not_a_release() {
        assert_eq!(parse_release("{}"), None);
        assert_eq!(parse_release("not json"), None);
        assert_eq!(parse_release("[]"), None);
        assert_eq!(parse_release(r#"{"tag_name":""}"#), None);
        assert_eq!(parse_release(r#"{"tag_name":"v"}"#), None);
        assert_eq!(parse_release(r#"{"tag_name":42}"#), None);
        assert_eq!(
            parse_release(
                r#"{"tag_name":"v1.11.0","html_url":"https://evil.example/fredimachado/omairc"}"#
            ),
            None
        );
    }

    #[test]
    fn release_pages_must_sit_under_the_repository() {
        for url in [
            "https://github.com/fredimachado/omairc",
            "https://github.com/fredimachado/omairc/releases/tag/v1.11.0",
            "https://github.com/fredimachado/omairc?tab=readme",
            "HTTPS://GitHub.com/fredimachado/omairc/releases",
        ] {
            assert!(release_url_is_safe(url), "{url} should pass");
        }
        for url in [
            "http://github.com/fredimachado/omairc",
            "https://github.com/fredimachado/omairc-evil",
            "https://github.com/other/omairc",
            "https://github.com.evil.example/fredimachado/omairc",
            "https://user@github.com/fredimachado/omairc",
            "https://github.com:443/fredimachado/omairc",
            "github.com/fredimachado/omairc",
            "",
        ] {
            assert!(!release_url_is_safe(url), "{url} should fail");
        }
    }

    #[test]
    fn windows_setup_asset_names_and_safety() {
        assert_eq!(windows_setup_name("9.9.9"), SETUP_NAME);
        let setup_url = Url::parse(SETUP_URL).unwrap();
        assert!(windows_setup_request_is_safe(&setup_url, "v9.9.9", "9.9.9"));
        for raw in [
            "https://evil.example/omairc-9.9.9-windows-x64-setup.exe",
            "https://github.com/fredimachado/omairc/releases/download/v9.9.9/omairc-9.9.9-windows-x64-setup.exe?raw=1",
            "https://user@github.com/fredimachado/omairc/releases/download/v9.9.9/omairc-9.9.9-windows-x64-setup.exe",
            "http://github.com/fredimachado/omairc/releases/download/v9.9.9/omairc-9.9.9-windows-x64-setup.exe",
        ] {
            let url = Url::parse(raw).unwrap();
            assert!(!windows_setup_request_is_safe(&url, "v9.9.9", "9.9.9"), "{raw}");
        }
        assert!(!windows_setup_request_is_safe(
            &setup_url,
            "v9.9.9/../x",
            "9.9.9"
        ));
        assert!(windows_setup_redirect_is_safe(
            &Url::parse("https://objects.githubusercontent.com/releases/setup.exe").unwrap()
        ));
        assert!(!windows_setup_redirect_is_safe(
            &Url::parse("https://evil.example/setup.exe").unwrap()
        ));
        assert!(!windows_setup_redirect_is_safe(
            &Url::parse("http://github.com/setup.exe").unwrap()
        ));
    }

    #[test]
    fn install_locations_match_the_app_directory() {
        assert!(install_location_matches(
            "C:/Users/Fredi/AppData/Local/Programs/Omairc/",
            "C:/Users/Fredi/AppData/Local/Programs/Omairc"
        ));
        assert!(install_location_matches(
            "C:\\Users\\Fredi\\AppData\\Local\\Programs\\Omairc\\",
            "C:/Users/Fredi/AppData/Local/Programs/Omairc"
        ));
        assert!(!install_location_matches("C:/elsewhere", "C:/Omairc"));
        assert!(!install_location_matches("", "C:/Omairc"));
        assert!(!install_location_matches("   ", "C:/Omairc"));
    }

    #[test]
    fn a_verified_setup_asset_is_picked_up() {
        let release = setup_release();
        let setup = release.setup.expect("the one matching asset");
        assert_eq!(setup.url, SETUP_URL);
        assert_eq!(
            setup.sha256,
            "d323de13bbb0973b891849578f74c69cb4ef85fc37ffeb05b31d835f708c0ae9"
        );
        assert_eq!(setup.size, 12);
    }

    #[test]
    fn a_setup_asset_must_pass_every_check() {
        let digest = SETUP_DIGEST;
        let unsafe_url = setup_release_json(
            "https://evil.example/omairc-9.9.9-windows-x64-setup.exe",
            digest,
            "12",
        );
        assert_eq!(parse_release(&unsafe_url).expect("release").setup, None);

        let missing_digest = setup_release_json(SETUP_URL, "md5:abcd", "12");
        assert_eq!(parse_release(&missing_digest).expect("release").setup, None);

        let fractional = setup_release_json(SETUP_URL, digest, "12.5");
        assert_eq!(parse_release(&fractional).expect("release").setup, None);

        let zero = setup_release_json(SETUP_URL, digest, "0");
        assert_eq!(parse_release(&zero).expect("release").setup, None);

        let oversized = setup_release_json(SETUP_URL, digest, &(MAXIMUM_SETUP_BYTES + 1).to_string());
        assert_eq!(parse_release(&oversized).expect("release").setup, None);

        // Two matching assets are ambiguous, so neither is offered.
        let duplicated = setup_release_json(SETUP_URL, digest, "12").replacen(
            "]}",
            &format!(
                r#",{{"name":"{SETUP_NAME}","browser_download_url":"{SETUP_URL}","size":12,"digest":"{digest}"}}]}}"#
            ),
            1,
        );
        let release = parse_release(&duplicated).expect("release still parses");
        assert_eq!(release.setup, None);
        assert_eq!(release.version, "9.9.9");
    }

    #[test]
    fn a_reply_judged_from_its_status_and_body() {
        let parsed = release_from_reply(200, release_json("v0.8.0", "https://github.com/fredimachado/omairc/releases/tag/v0.8.0").as_bytes());
        assert_eq!(parsed.expect("release").version, "0.8.0");
        assert_eq!(release_from_reply(404, LATEST_RELEASE.as_bytes()), None);
        assert_eq!(release_from_reply(200, &[0xff, 0xfe]), None);
    }

    #[test]
    fn a_fresh_check_is_idle_and_hides_the_status_line() {
        let state = UpdateCheck::new("0.1.0", false);
        assert_eq!(state.status(), UpdateStatus::Idle);
        assert_eq!(state.message(), None);
        assert_eq!(state.action(), StatusAction::None);
    }

    #[test]
    fn a_newer_release_offers_its_page_on_a_portable_copy() {
        let state = checked(
            false,
            parse_release(&release_json(
                "v9.9.9",
                "https://github.com/fredimachado/omairc/releases/tag/v9.9.9",
            ))
            .unwrap(),
        );
        assert_eq!(state.status(), UpdateStatus::Available);
        assert_eq!(state.message().as_deref(), Some("Version 9.9.9 is available."));
        assert_eq!(
            state.action(),
            StatusAction::OpenPage(
                "https://github.com/fredimachado/omairc/releases/tag/v9.9.9".to_string()
            )
        );
        assert!(!state.installer_update_available());
    }

    #[test]
    fn an_installed_copy_with_a_setup_downloads_instead_of_opening_the_page() {
        let state = checked(true, setup_release());
        assert!(state.installer_update_available());
        assert_eq!(state.action(), StatusAction::Download);
    }

    #[test]
    fn the_same_or_an_older_release_is_up_to_date() {
        for current in ["1.11.0", "v1.11.0", "1.12.0"] {
            let mut state = UpdateCheck::new(current, false);
            let ticket = state.begin_check().unwrap();
            state.finish_check(ticket, parse_release(LATEST_RELEASE));
            assert_eq!(state.status(), UpdateStatus::UpToDate, "current {current}");
            assert_eq!(state.message().as_deref(), Some("Omairc is up to date."));
            assert_eq!(state.action(), StatusAction::None);
        }
    }

    #[test]
    fn a_failed_check_says_so_and_clears_the_release() {
        let mut state = checked(false, setup_release());
        let ticket = state.begin_check().unwrap();
        assert!(state.finish_check(ticket, None));
        assert_eq!(state.status(), UpdateStatus::Failed);
        assert_eq!(state.message().as_deref(), Some("Could not check for updates."));
        assert!(state.latest().is_none());
    }

    #[test]
    fn a_check_while_one_runs_does_nothing() {
        let mut state = UpdateCheck::new("0.1.0", false);
        assert!(state.begin_check().is_some());
        assert!(state.begin_check().is_none());
    }

    #[test]
    fn a_reply_for_a_cancelled_check_is_ignored() {
        let mut state = UpdateCheck::new("0.1.0", false);
        let ticket = state.begin_check().unwrap();
        state.cancel_check();
        assert_eq!(state.status(), UpdateStatus::Idle);
        assert!(!state.finish_check(ticket, parse_release(LATEST_RELEASE)));
        assert_eq!(state.status(), UpdateStatus::Idle);
    }

    #[test]
    fn check_latest_reads_a_served_release_through_the_seam() {
        let fake = FakeFetch::new();
        let api = LATEST_RELEASE_API_URL;
        fake.serve(api, 200, LATEST_RELEASE.as_bytes());
        let release = check_latest(&fake, "1.10.3", api).expect("release");
        assert_eq!(release.version, "1.11.0");
        let requests = fake.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url, api);
        assert!(requests[0]
            .headers
            .contains(&("Accept".to_string(), "application/vnd.github+json".to_string())));
        assert!(requests[0].headers.contains(&(
            "User-Agent".to_string(),
            "Omairc/1.10.3 (+https://github.com/fredimachado/omairc)".to_string()
        )));
    }

    #[test]
    fn check_latest_fails_closed() {
        let fake = FakeFetch::new();
        fake.serve("https://api.example/404", 404, br#"{"message":"Not Found"}"#);
        assert_eq!(check_latest(&fake, "1.0.0", "https://api.example/404"), None);

        fake.serve("https://api.example/oversized", 200, &vec![b' '; (MAXIMUM_RELEASE_BYTES + 1) as usize]);
        assert_eq!(check_latest(&fake, "1.0.0", "https://api.example/oversized"), None);

        fake.fail("https://api.example/down", "DNS failed");
        assert_eq!(check_latest(&fake, "1.0.0", "https://api.example/down"), None);

        fake.redirect("https://api.example/moved", 302, "http://api.example/plain");
        fake.serve("http://api.example/plain", 200, LATEST_RELEASE.as_bytes());
        assert_eq!(check_latest(&fake, "1.0.0", "https://api.example/moved"), None,
            "a downgrade to http is refused");
    }

    #[test]
    fn check_latest_sends_the_user_agent_over_the_wire() {
        let (url, server) = serve_once("200 OK", "", LATEST_RELEASE.as_bytes().to_vec());
        let release = check_latest(&crate::desktop::httpfetch::UreqFetch, "1.10.3", &url)
            .expect("release over loopback");
        let (head, _) = server.join().expect("server thread");
        let head = head.to_ascii_lowercase();
        assert_eq!(release.version, "1.11.0");
        assert!(
            head.contains("user-agent: omairc/1.10.3 (+https://github.com/fredimachado/omairc)"),
            "request was {head}"
        );
    }

    #[test]
    fn start_download_needs_an_installed_copy_and_a_setup() {
        let mut portable = checked(false, setup_release());
        assert!(portable.start_download().is_none());
        assert_eq!(portable.status(), UpdateStatus::Available);

        let mut no_setup = checked(
            true,
            parse_release(LATEST_RELEASE).expect("no digest, so no setup"),
        );
        assert!(no_setup.start_download().is_none());
    }

    #[test]
    fn an_installed_copy_downloads_then_offers_restart() {
        let mut state = checked(true, setup_release());
        let ticket = state.start_download().expect("download starts");
        assert_eq!(ticket.setup.size, 12);
        assert_eq!(
            state.status(),
            UpdateStatus::Downloading { written: 0, total: 12 }
        );
        assert_eq!(state.message().as_deref(), Some("Downloading update…"));
        assert!(state.report_progress(&ticket, 6));
        assert_eq!(state.message().as_deref(), Some("Downloading update… 50%"));
        assert!(state.commit_download(&ticket, Some(PathBuf::from("/tmp/setup.exe"))));
        assert_eq!(state.status(), UpdateStatus::ReadyToRestart);
        assert_eq!(state.message().as_deref(), Some("Restart to update"));
        assert_eq!(state.action(), StatusAction::Restart);
        assert!(state.restart_ready());
        assert_eq!(state.installer_path(), Some(Path::new("/tmp/setup.exe")));
    }

    #[test]
    fn a_failed_download_says_so_and_can_be_retried() {
        let mut state = checked(true, setup_release());
        let ticket = state.start_download().unwrap();
        assert!(state.commit_download(&ticket, None));
        assert_eq!(state.status(), UpdateStatus::DownloadFailed);
        assert_eq!(
            state.message().as_deref(),
            Some("Could not download the update.")
        );
        assert_eq!(state.action(), StatusAction::Download);
        assert!(state.start_download().is_some(), "a failed download may start again");
    }

    #[test]
    fn a_check_waits_for_a_download() {
        let mut state = checked(true, setup_release());
        let ticket = state.start_download().unwrap();
        assert!(
            state.begin_check().is_none(),
            "Check for Updates is disabled while the setup downloads"
        );
        assert!(state.report_progress(&ticket, 5));
        assert!(state.commit_download(&ticket, Some(PathBuf::from("/tmp/setup.exe"))));
        assert_eq!(state.status(), UpdateStatus::ReadyToRestart);
    }

    #[test]
    fn a_download_ticket_from_an_earlier_generation_is_ignored() {
        let mut state = checked(true, setup_release());
        let first = state.start_download().unwrap();
        assert!(state.commit_download(&first, None));
        let retry = state.start_download().unwrap();
        assert!(
            !state.commit_download(&first, Some(PathBuf::from("/tmp/old.exe"))),
            "the first ticket no longer names the current download"
        );
        assert!(state.installer_path().is_none());
        assert!(matches!(state.status(), UpdateStatus::Downloading { .. }));
        assert!(state.commit_download(&retry, Some(PathBuf::from("/tmp/new.exe"))));
        assert_eq!(state.installer_path(), Some(Path::new("/tmp/new.exe")));
    }

    #[test]
    fn cancel_during_a_download_does_not_abort_it() {
        let mut state = checked(true, setup_release());
        let ticket = state.start_download().unwrap();
        state.cancel_check();
        assert!(matches!(state.status(), UpdateStatus::Downloading { .. }));
        assert!(state.commit_download(&ticket, Some(PathBuf::from("/tmp/setup.exe"))));
        assert_eq!(state.status(), UpdateStatus::ReadyToRestart);
    }

    #[test]
    fn launching_counts_and_suppresses_the_process_in_tests() {
        let launcher = RecordingLauncher::new(true);
        let mut state = checked(true, setup_release());
        let ticket = state.start_download().unwrap();
        state.commit_download(&ticket, Some(PathBuf::from("/tmp/setup.exe")));
        state.suppress_installer_launch();
        assert_eq!(state.launch_installer(&launcher, &verify_always), LaunchOutcome::Started);
        assert_eq!(state.launch_attempts(), 1);
        assert_eq!(state.status(), UpdateStatus::ReadyToRestart);
        assert!(launcher.started().is_empty(), "the latch never starts a process");
    }

    #[test]
    fn the_installer_starts_once_by_click_or_close() {
        let launcher = RecordingLauncher::new(true);
        let mut state = checked(true, setup_release());
        let ticket = state.start_download().unwrap();
        state.commit_download(&ticket, Some(PathBuf::from("/tmp/setup.exe")));

        assert_eq!(state.launch_installer(&launcher, &verify_always), LaunchOutcome::Started);
        assert_eq!(state.launch_installer(&launcher, &verify_always), LaunchOutcome::AlreadyStarted);
        assert_eq!(state.launch_on_close(&launcher, &verify_always), None, "close after a click does not launch");
        assert_eq!(launcher.started(), vec![PathBuf::from("/tmp/setup.exe")]);
        assert_eq!(state.launch_attempts(), 1);
    }

    #[test]
    fn the_first_window_close_starts_the_installer_when_nothing_else_did() {
        let launcher = RecordingLauncher::new(true);
        let mut state = checked(true, setup_release());
        let ticket = state.start_download().unwrap();
        state.commit_download(&ticket, Some(PathBuf::from("/tmp/setup.exe")));
        assert_eq!(
            state.launch_on_close(&launcher, &verify_always),
            Some(LaunchOutcome::Started)
        );
        assert_eq!(launcher.started().len(), 1);
    }

    #[test]
    fn a_close_with_no_ready_installer_does_nothing() {
        let launcher = RecordingLauncher::new(true);
        let mut state = checked(true, setup_release());
        assert_eq!(state.launch_on_close(&launcher, &verify_always), None);
        assert_eq!(state.launch_installer(&launcher, &verify_always), LaunchOutcome::NotReady);
        assert!(launcher.started().is_empty());
    }

    #[test]
    fn a_file_that_no_longer_matches_is_not_launched() {
        let launcher = RecordingLauncher::new(true);
        let mut state = checked(true, setup_release());
        let ticket = state.start_download().unwrap();
        state.commit_download(&ticket, Some(PathBuf::from("/tmp/setup.exe")));
        let seen = RefCell::new(Vec::new());
        let outcome = state.launch_installer(&launcher, &|path, setup| {
            seen.borrow_mut().push((path.to_path_buf(), setup.size));
            false
        });
        assert_eq!(outcome, LaunchOutcome::VerificationFailed);
        assert_eq!(state.status(), UpdateStatus::DownloadFailed);
        assert!(state.installer_path().is_none());
        assert_eq!(seen.borrow().as_slice(), [(PathBuf::from("/tmp/setup.exe"), 12)]);
        assert!(launcher.started().is_empty());
        assert_eq!(state.launch_attempts(), 0);
    }

    #[test]
    fn a_failed_spawn_is_counted_and_not_latched() {
        let launcher = RecordingLauncher::new(false);
        let mut state = checked(true, setup_release());
        let ticket = state.start_download().unwrap();
        state.commit_download(&ticket, Some(PathBuf::from("/tmp/setup.exe")));
        assert_eq!(state.launch_installer(&launcher, &verify_always), LaunchOutcome::SpawnFailed);
        assert_eq!(state.launch_attempts(), 1);
        // The next click may try again, since nothing started.
        let retry = RecordingLauncher::new(true);
        assert_eq!(state.launch_installer(&retry, &verify_always), LaunchOutcome::Started);
    }

    #[test]
    fn the_digest_parser_takes_only_full_sha256_hex() {
        assert_eq!(
            parse_sha256_digest("sha256:D323DE13BBB0973B891849578F74C69CB4EF85FC37FFEB05B31D835F708C0AE9").as_deref(),
            Some("d323de13bbb0973b891849578f74c69cb4ef85fc37ffeb05b31d835f708c0ae9")
        );
        assert_eq!(parse_sha256_digest("sha256:abcd"), None);
        assert_eq!(parse_sha256_digest("md5:d323de13bbb0973b891849578f74c69cb4ef85fc37ffeb05b31d835f708c0ae9"), None);
        assert_eq!(parse_sha256_digest(&format!("sha256:{}", "g".repeat(64))), None);
    }

    #[test]
    fn the_user_agent_names_the_app_and_repository() {
        assert_eq!(
            user_agent("1.10.3"),
            "Omairc/1.10.3 (+https://github.com/fredimachado/omairc)"
        );
    }

    #[test]
    fn the_release_token_guard_matches_qt() {
        assert!(release_token_is_safe("v9.9.9"));
        assert!(release_token_is_safe("9.9.9-rc_1"));
        assert!(!release_token_is_safe(""));
        assert!(!release_token_is_safe("9.9/9"));
        assert!(!release_token_is_safe(&"9".repeat(65)));
    }

    #[test]
    fn the_repository_and_api_urls_are_fixed() {
        assert_eq!(REPO_URL, "https://github.com/fredimachado/omairc");
        assert_eq!(
            LATEST_RELEASE_API_URL,
            "https://api.github.com/repos/fredimachado/omairc/releases/latest"
        );
    }

    #[test]
    fn the_windows_uninstall_key_matches_the_inno_app_id() {
        assert_eq!(
            WINDOWS_UNINSTALL_KEY,
            "{37400F83-8332-4043-8DFF-1F69E51F6555}_is1"
        );
        assert_eq!(
            windows_setup_name("1.0.0"),
            "omairc-1.0.0-windows-x64-setup.exe"
        );
    }
}
