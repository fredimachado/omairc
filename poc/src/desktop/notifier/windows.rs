//! Windows Toast notifications. Port of `windowsnotifications.cpp`.
//!
//! Toasts use the AUMID `FrediMachado.Omairc`. Activation goes through the activator
//! registration (`LocalServer32`) and a Start Menu `Omairc.lnk` that carries the AUMID.
//! Which exe those point at is decided by two rules:
//!
//! - An installed copy, one under `%LOCALAPPDATA%\Programs\Omairc` or `%ProgramFiles%\Omairc`,
//!   may take the activator registration and the per-user shortcut from another exe, even one
//!   that still exists.
//! - Any other build, such as a dev or portable build, may take them only when nothing is
//!   registered, when the registered exe is itself, or when the registered exe no longer
//!   exists. So it does not displace an exe that still exists.
//!
//! Two installed copies take turns: each launch overwrites the other's registration and link,
//! and this is intended.
//!
//! The shortcut fallback reads two Start Menu `Omairc.lnk` links: the per-user one and the
//! all-users one, under `FOLDERID_CommonPrograms`. It writes only the per-user link, and only
//! when neither link refuses. A link refuses when it cannot be read, when its target does not
//! resolve to a path (an empty target, or `%VAR%` text that does not expand), or when it names
//! an exe this build may not take. Before the fallback, the repair path re-stamps a link that
//! already names this exe, but only when no such link is stamped yet, in either location. It
//! never creates an all-users link. An installed copy then reaches the per-user decision, unless
//! its per-user link already names this exe and is stamped.
//!
//! This code decides what it writes. It does not establish which copy receives a click while
//! more than one copy is registered.

use std::collections::VecDeque;
use std::ffi::OsStr;
use std::os::windows::prelude::OsStrExt;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use async_channel::Sender;
use std::mem::ManuallyDrop;
use std::path::{Component, Path, PathBuf};

use windows::core::{GUID, HSTRING, Interface, PCWSTR};
use windows::Win32::Foundation::{
    CLASS_E_NOAGGREGATION, ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, HANDLE, WAIT_OBJECT_0,
    WIN32_ERROR,
};
use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ToastActivatorCLSID;
use windows::Win32::System::Com::{
    CoAddRefServerProcess, CoCreateInstance, CoRegisterClassObject, CoReleaseServerProcess,
    CoRevokeClassObject, CoTaskMemFree, CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER, IClassFactory,
    IClassFactory_Impl, IPersistFile, STGM_READ, REGCLS_MULTIPLEUSE,
};
use windows::Win32::System::Com::StructuredStorage::{
    InitPropVariantFromCLSID, PropVariantClear, PROPVARIANT, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
};
use windows::Win32::System::Variant::{VT_CLSID, VT_LPWSTR};
use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::System::Registry::{
    RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
};
use windows::Win32::System::Threading::{CreateEventW, SetEvent};
use windows::Win32::UI::Notifications::{
    INotificationActivationCallback, INotificationActivationCallback_Impl,
    NOTIFICATION_USER_INPUT_DATA,
};
use windows::Win32::UI::Shell::{
    FOLDERID_CommonPrograms, FOLDERID_ProgramFiles, FOLDERID_UserProgramFiles, IShellLinkW,
    KF_FLAG_DEFAULT, SHGetKnownFolderPath, SHStrDupW, ShellLink, SLGP_RAWPATH,
};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MsgWaitForMultipleObjects, PeekMessageW, QS_ALLINPUT, TranslateMessage,
    MSG, PM_REMOVE,
};
use windows_implement::implement;
use windows::Data::Xml::Dom::XmlDocument;
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager, ToastNotifier};

use super::util::{build_launch_url, build_toast_xml, conversation_tag, parse_launch_url};
use super::{Activation, Notifier};

const AUMID: &str = "FrediMachado.Omairc";
const ACTIVATOR_CLSID: GUID = GUID::from_u128(0xDBE38477_5F87_42A1_AFA1_11FA12F2E5E5);
/// Wide-character buffer for a path up to the long-path limit (32767 characters) and its NUL.
const LONG_PATH_BUFFER: usize = 32_768;

struct ToastWork {
    summary: String,
    body: String,
    network_id: String,
    target: String,
    msgid: String,
}

struct WindowsNotifier {
    enabled: Arc<std::sync::atomic::AtomicBool>,
}

static TOAST_QUEUE: OnceLock<Mutex<VecDeque<ToastWork>>> = OnceLock::new();
static ACTIVATION_TX: OnceLock<Sender<Activation>> = OnceLock::new();
static TOAST_EVENT: OnceLock<usize> = OnceLock::new();

impl Notifier for WindowsNotifier {
    fn notify(
        &self,
        summary: &str,
        body: &str,
        network_id: &str,
        target: &str,
        msgid: &str,
    ) {
        if !self.enabled.load(std::sync::atomic::Ordering::Acquire) {
            return;
        }
        if let Some(queue) = TOAST_QUEUE.get() {
            queue.lock().expect("toast queue").push_back(ToastWork {
                summary: summary.to_owned(),
                body: body.to_owned(),
                network_id: network_id.to_owned(),
                target: target.to_owned(),
                msgid: msgid.to_owned(),
            });
            if let Some(event) = TOAST_EVENT.get() {
                let handle = HANDLE(*event as *mut std::ffi::c_void);
                if let Err(error) = unsafe { SetEvent(handle) } {
                    log::warn!(
                        "Windows notifier: could not signal the toast thread, so a queued toast may stay queued: {:?}",
                        error
                    );
                }
            }
        }
    }
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain([0]).collect()
}

fn propvariant_inner(prop: &PROPVARIANT) -> &PROPVARIANT_0_0 {
    unsafe { &*(std::ptr::from_ref(prop) as *const PROPVARIANT_0_0) }
}

fn init_propvariant_from_string(value: &str) -> Result<PROPVARIANT, windows::core::Error> {
    let value_wide = wide(value);
    let pwsz = unsafe { SHStrDupW(PCWSTR(value_wide.as_ptr()))? };
    let mut prop = PROPVARIANT::default();
    prop.Anonymous.Anonymous = ManuallyDrop::new(PROPVARIANT_0_0 {
        vt: VT_LPWSTR,
        wReserved1: 0,
        wReserved2: 0,
        wReserved3: 0,
        Anonymous: PROPVARIANT_0_0_0 { pwszVal: pwsz },
    });
    Ok(prop)
}

fn same_exe_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left
            .to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy()),
    }
}

/// Pulls the exe out of a `LocalServer32` command line: the quoted path, then any
/// arguments. An empty value names no exe.
fn registered_exe(command: &str) -> Option<PathBuf> {
    let command = command.trim();
    let exe = match command.strip_prefix('"') {
        Some(rest) => rest.split('"').next()?,
        None => command,
    };
    (!exe.is_empty()).then(|| PathBuf::from(exe))
}

/// Whether this exe may take a registration or shortcut. It may when nothing is
/// registered, when the registered exe is itself, or when the registered exe is gone.
/// A registered exe that still exists belongs to another install, so it is left alone.
fn should_claim(
    existing: Option<&Path>,
    this_exe: &Path,
    exists: impl Fn(&Path) -> bool,
) -> bool {
    match existing {
        None => true,
        Some(existing) => same_exe_path(existing, this_exe) || !exists(existing),
    }
}

/// The canonical form of `path`, which resolves 8.3 short names and junctions. `None` when the
/// disk cannot resolve it, as when it does not exist.
fn resolve_path(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

/// Drops the `\\?\` verbatim prefix that `canonicalize` adds on Windows, so `\\?\C:\x` and
/// `C:\x` compare equal. `\\?\UNC\server\share` becomes `\\server\share`.
fn strip_verbatim(path: &Path) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path.to_path_buf();
    };
    if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{unc}"))
    } else if let Some(plain) = text.strip_prefix(r"\\?\") {
        PathBuf::from(plain)
    } else {
        path.to_path_buf()
    }
}

/// Whether `this_exe` lies inside one of `install_roots`. `resolve` names the path the disk
/// gives, or `None` when there is none, and a path with no resolution is compared as given. The
/// exe and each root are resolved, then stripped of the verbatim prefix, before the comparison.
/// Path components are compared one at a time, ignoring ASCII case, so `...\Omairc-dev\x.exe`
/// is not inside `...\Omairc`. A `..` left in the compared exe path is refused, since it could
/// leave the root without changing the prefix.
fn is_installed_exe(
    this_exe: &Path,
    install_roots: &[PathBuf],
    resolve: impl Fn(&Path) -> Option<PathBuf>,
) -> bool {
    let resolved = |path: &Path| {
        strip_verbatim(&resolve(path).unwrap_or_else(|| path.to_path_buf()))
    };
    let exe = resolved(this_exe);
    let exe_parts: Vec<Component<'_>> = exe.components().collect();
    if exe_parts.contains(&Component::ParentDir) {
        return false;
    }
    install_roots.iter().any(|root| {
        let root = resolved(root);
        let root_parts: Vec<Component<'_>> = root.components().collect();
        !root_parts.is_empty()
            && root_parts.len() < exe_parts.len()
            && root_parts.iter().zip(&exe_parts).all(|(root_part, exe_part)| {
                root_part.as_os_str().eq_ignore_ascii_case(exe_part.as_os_str())
            })
    })
}

/// Where a Start Menu `Omairc.lnk` lives. Only `PerUser` is ever written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinkSlot {
    AllUsers,
    PerUser,
}

/// One Start Menu `Omairc.lnk`, as read from disk. Only a confirmed absence is `Missing`.
#[derive(Debug, PartialEq, Eq)]
enum LinkState {
    Missing,
    /// The link exists, but its target cannot be read. Fail closed.
    Unreadable,
    /// The link names an exe path.
    Targets(PathBuf),
}

/// Why the shortcut fallback must not write the per-user link.
#[derive(Debug, PartialEq, Eq)]
enum LinkRefusal {
    Unreadable(LinkSlot),
    NamesOtherExe(LinkSlot, PathBuf),
}

/// Decides whether the shortcut fallback may write the per-user link, given both links as
/// read from disk. An unreadable link refuses. Any other link allows when this exe is an
/// installed copy, or when `should_claim` allows it: the link names this exe, or an exe that is
/// gone. The all-users link is checked first and the first refusal wins, so when both links
/// refuse, the all-users refusal is returned.
fn check_existing_links(
    this_exe: &Path,
    install_roots: &[PathBuf],
    all_users: &LinkState,
    per_user: &LinkState,
    exists: impl Fn(&Path) -> bool,
    resolve: impl Fn(&Path) -> Option<PathBuf>,
) -> Result<(), LinkRefusal> {
    let from_install = is_installed_exe(this_exe, install_roots, resolve);
    for (slot, state) in [(LinkSlot::AllUsers, all_users), (LinkSlot::PerUser, per_user)] {
        match state {
            LinkState::Missing => {}
            LinkState::Unreadable => return Err(LinkRefusal::Unreadable(slot)),
            LinkState::Targets(target) => {
                if !from_install && !should_claim(Some(target.as_path()), this_exe, &exists) {
                    return Err(LinkRefusal::NamesOtherExe(slot, target.clone()));
                }
            }
        }
    }
    Ok(())
}

/// Decides what a link's raw target means. `raw` is what `shortcut_target` read, or `None` when
/// that read failed. `expand` turns `%VAR%` text into a path, and returns `None` when it fails.
/// The link is `Unreadable` when the read or the expansion fails, when the result is empty
/// (an empty target names no exe), or when `%` survives expansion. A variable this session cannot
/// resolve leaves its text in place, and that text is not a path to test against the disk.
fn link_state_from_target(
    raw: Option<&str>,
    expand: impl Fn(&str) -> Option<String>,
) -> LinkState {
    match raw.and_then(expand) {
        Some(path) if !path.is_empty() && !path.contains('%') => {
            LinkState::Targets(PathBuf::from(path))
        }
        _ => LinkState::Unreadable,
    }
}

/// Expands `%VAR%` text in a raw link target with `ExpandEnvironmentStringsW`. `None` when the
/// call fails or the result does not fit the long-path buffer.
fn expand_environment(raw: &str) -> Option<String> {
    let source = wide(raw);
    let mut buffer = vec![0u16; LONG_PATH_BUFFER];
    let written = unsafe {
        ExpandEnvironmentStringsW(PCWSTR(source.as_ptr()), Some(buffer.as_mut_slice()))
    } as usize;
    if written == 0 || written > buffer.len() {
        return None;
    }
    // The count includes the terminating NUL.
    String::from_utf16(&buffer[..written - 1]).ok()
}

/// Reads one Start Menu `Omairc.lnk`. Only a confirmed absence counts as `Missing`, so a
/// failure to check the folder leaves the link `Unreadable`.
fn read_link_state(link: &Path) -> LinkState {
    match link.try_exists() {
        Ok(false) => LinkState::Missing,
        Ok(true) => link_state_from_target(
            shortcut_target(&link.to_string_lossy()).as_deref(),
            expand_environment,
        ),
        Err(_) => LinkState::Unreadable,
    }
}

/// Reads the default value of the activator's `LocalServer32` key. `Ok(None)` means the
/// value is absent. Any other failure is an error, so the caller can fail closed.
fn read_registered_command(subkey: &[u16]) -> Result<Option<String>, WIN32_ERROR> {
    let mut data = vec![0u16; 32_768];
    let mut bytes = (data.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            None,
            RRF_RT_REG_SZ,
            None,
            Some(data.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    if status != ERROR_SUCCESS {
        return Err(status);
    }
    let len = data.iter().position(|&ch| ch == 0).unwrap_or(data.len());
    Ok(Some(String::from_utf16_lossy(&data[..len])))
}

/// Resolves a known folder through the shell.
fn known_folder(id: &GUID) -> Option<PathBuf> {
    let raw = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }.ok()?;
    let path = unsafe { raw.to_string() }.ok().map(PathBuf::from);
    unsafe { CoTaskMemFree(Some(raw.as_ptr().cast_const().cast())) };
    path
}

/// The roots an installed copy lives under. These match `DefaultDirName={autopf}\Omairc` in
/// `packaging/windows/omairc.iss`: the per-user `Programs` folder and the all-users Program
/// Files folder. A root whose folder cannot be resolved is left out, so nothing matches it.
fn install_roots() -> Vec<PathBuf> {
    [known_folder(&FOLDERID_UserProgramFiles), known_folder(&FOLDERID_ProgramFiles)]
        .into_iter()
        .flatten()
        .map(|folder| folder.join("Omairc"))
        .collect()
}

fn start_menu_program_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = dirs::home_dir() {
        dirs.push(
            home.join("AppData/Roaming/Microsoft/Windows/Start Menu/Programs"),
        );
    }
    if let Ok(program_data) = std::env::var("ProgramData") {
        dirs.push(
            PathBuf::from(program_data).join("Microsoft/Windows/Start Menu/Programs"),
        );
    }
    dirs
}

fn shortcut_target(link_path: &str) -> Option<String> {
    let shell_link: IShellLinkW = match unsafe {
        CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
    } {
        Ok(link) => link,
        Err(_) => return None,
    };
    let persist: IPersistFile = match shell_link.cast() {
        Ok(persist) => persist,
        Err(_) => return None,
    };
    let link_wide = wide(link_path);
    if unsafe { persist.Load(PCWSTR(link_wide.as_ptr()), STGM_READ) }.is_err() {
        return None;
    }
    let mut target = vec![0u16; LONG_PATH_BUFFER];
    if unsafe {
        shell_link.GetPath(&mut target, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32)
    }
    .is_err()
    {
        return None;
    }
    // A target with no NUL inside the buffer was cut short, so it is not read as a path.
    let len = target.iter().position(|&ch| ch == 0)?;
    String::from_utf16(&target[..len]).ok()
}

fn shortcut_is_stamped(link_path: &str) -> bool {
    let shell_link: IShellLinkW = match unsafe {
        CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
    } {
        Ok(link) => link,
        Err(_) => return false,
    };
    let persist: IPersistFile = match shell_link.cast() {
        Ok(persist) => persist,
        Err(_) => return false,
    };
    let link_wide = wide(link_path);
    if unsafe { persist.Load(PCWSTR(link_wide.as_ptr()), STGM_READ) }.is_err() {
        return false;
    }
    let store: IPropertyStore = match shell_link.cast() {
        Ok(store) => store,
        Err(_) => return false,
    };

    let mut id = match unsafe { store.GetValue(&PKEY_AppUserModel_ID) } {
        Ok(id) => id,
        Err(_) => return false,
    };
    let id_inner = propvariant_inner(&id);
    let aumid_matches = unsafe {
        id_inner.vt == VT_LPWSTR
            && !id_inner.Anonymous.pwszVal.is_null()
            && id_inner.Anonymous.pwszVal.to_string().is_ok_and(|value| value == AUMID)
    };
    unsafe {
        let _ = PropVariantClear(&mut id);
    }
    if !aumid_matches {
        return false;
    }

    let mut clsid = match unsafe { store.GetValue(&PKEY_AppUserModel_ToastActivatorCLSID) } {
        Ok(clsid) => clsid,
        Err(_) => return false,
    };
    let clsid_inner = propvariant_inner(&clsid);
    let clsid_matches = unsafe {
        clsid_inner.vt == VT_CLSID
            && !clsid_inner.Anonymous.puuid.is_null()
            && *clsid_inner.Anonymous.puuid == ACTIVATOR_CLSID
    };
    unsafe {
        let _ = PropVariantClear(&mut clsid);
    }
    clsid_matches
}

fn installed_shortcut_paths(exe_path: &str) -> Vec<PathBuf> {
    let mut links = Vec::new();
    for dir in start_menu_program_dirs() {
        let link = dir.join("Omairc.lnk");
        if !link.is_file() {
            continue;
        }
        let link_path = link.to_string_lossy();
        let Some(target) = shortcut_target(&link_path) else {
            continue;
        };
        if same_exe_path(Path::new(&target), Path::new(exe_path)) {
            links.push(link);
        }
    }
    links
}

/// Claims the toast activator registration. An installed copy always may. Any other build may
/// only when `should_claim` allows it.
fn register_com_server(exe: &str, install_roots: &[PathBuf]) {
    let subkey =
        "SOFTWARE\\Classes\\CLSID\\{DBE38477-5F87-42A1-AFA1-11FA12F2E5E5}\\LocalServer32";
    let subkey_wide = wide(subkey);
    let existing = match read_registered_command(&subkey_wide) {
        Ok(command) => command.as_deref().and_then(registered_exe),
        Err(status) => {
            log::info!(
                "Windows notifier: could not read the toast activator registration ({:?}), leaving it unchanged",
                status
            );
            return;
        }
    };
    let this_exe = Path::new(exe);
    let from_install = is_installed_exe(this_exe, install_roots, resolve_path);
    let claimed_by_rule = should_claim(existing.as_deref(), this_exe, Path::exists);
    if !from_install && !claimed_by_rule {
        if let Some(owner) = existing.as_deref() {
            log::info!(
                "Windows notifier: {} owns toast activation, so this build leaves it registered",
                owner.display()
            );
        }
        return;
    }
    if from_install && !claimed_by_rule {
        if let Some(owner) = existing.as_deref() {
            log::info!(
                "Windows notifier: this installed copy replaces the toast activation registration owned by {}",
                owner.display()
            );
        }
    }
    let command = format!("\"{}\"", exe.replace('/', "\\"));
    let command_wide = wide(&command);
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey_wide.as_ptr()),
            None,
            REG_SZ.0,
            Some(command_wide.as_ptr() as *const _),
            (command_wide.len() * 2) as u32,
        )
    };
    if status != ERROR_SUCCESS {
        log::warn!(
            "Windows notifier: could not write the toast activator registration: {:?}",
            status
        );
    }
}

fn install_shortcut(link_path: &str, exe_path: &str) -> Result<(), windows::core::Error> {
    let shell_link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)? };
    let exe_wide = wide(exe_path);
    unsafe { shell_link.SetPath(PCWSTR(exe_wide.as_ptr()))? };
    let work_dir = std::path::Path::new(exe_path)
        .parent()
        .map(|parent| parent.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !work_dir.is_empty() {
        let work_wide = wide(&work_dir);
        unsafe { shell_link.SetWorkingDirectory(PCWSTR(work_wide.as_ptr()))? };
    }
    let icon_wide = wide(exe_path);
    unsafe { shell_link.SetIconLocation(PCWSTR(icon_wide.as_ptr()), 0)? };

    let store: IPropertyStore = shell_link.cast()?;
    let mut aumid_prop = init_propvariant_from_string(AUMID)?;
    unsafe {
        store.SetValue(&PKEY_AppUserModel_ID, &aumid_prop)?;
        PropVariantClear(&mut aumid_prop)?;
    }
    let mut clsid_prop = unsafe { InitPropVariantFromCLSID(&ACTIVATOR_CLSID)? };
    unsafe {
        store.SetValue(&PKEY_AppUserModel_ToastActivatorCLSID, &clsid_prop)?;
        PropVariantClear(&mut clsid_prop)?;
        store.Commit()?;
    }

    let persist: IPersistFile = shell_link.cast()?;
    let link_wide = wide(link_path);
    unsafe { persist.Save(PCWSTR(link_wide.as_ptr()), true) }
}

/// Repairs the Start Menu links that already name this exe. When one is stamped, nothing is
/// written. Otherwise the first link that takes a stamp is re-stamped. Returns whether a link
/// naming this exe is stamped afterwards.
fn repair_installed_links(exe_path: &str, installed: &[PathBuf]) -> bool {
    for link in installed {
        if shortcut_is_stamped(&link.to_string_lossy()) {
            return true;
        }
    }
    for link in installed {
        let link_path = link.to_string_lossy();
        if install_shortcut(&link_path, exe_path).is_ok() {
            return true;
        }
        log::warn!(
            "Windows notifier: could not stamp installed Start Menu shortcut {:?}",
            link
        );
    }
    false
}

/// Whether the registration stops once the repair path has run, before the per-user link
/// decision. A build that repaired a link and is not an installed copy stops, so a dev build
/// never overwrites a link it did not own. An installed copy stops only when its per-user link
/// already names this exe and is stamped, since nothing is left to write. Without a repair,
/// nothing stops here.
fn repair_ends_registration(repaired: bool, from_install: bool, per_user_settled: bool) -> bool {
    repaired && (!from_install || per_user_settled)
}

fn ensure_notification_registration(exe_path: &str) {
    let roots = install_roots();
    register_com_server(exe_path, &roots);

    let installed = installed_shortcut_paths(exe_path);
    let repaired = repair_installed_links(exe_path, &installed);

    let Some(programs) = dirs::home_dir().map(|home| {
        home.join("AppData/Roaming/Microsoft/Windows/Start Menu/Programs")
    }) else {
        return;
    };
    let Some(all_users_programs) = known_folder(&FOLDERID_CommonPrograms) else {
        log::info!(
            "Windows notifier: could not resolve the all-users Start Menu folder, so no toast shortcut is written"
        );
        return;
    };
    let link = programs.join("Omairc.lnk");
    let all_users_link = all_users_programs.join("Omairc.lnk");
    let link_path = link.to_string_lossy();
    let all_users_state = read_link_state(&all_users_link);
    let per_user_state = read_link_state(&link);
    let this_exe = Path::new(exe_path);
    let from_install = is_installed_exe(this_exe, &roots, resolve_path);
    // A per-user link that names this exe and is stamped after the repair needs no write.
    let per_user_settled =
        repaired && installed.contains(&link) && shortcut_is_stamped(&link_path);
    if repair_ends_registration(repaired, from_install, per_user_settled) {
        return;
    }
    if let Err(refusal) = check_existing_links(
        this_exe,
        &roots,
        &all_users_state,
        &per_user_state,
        Path::exists,
        resolve_path,
    ) {
        match refusal {
            LinkRefusal::Unreadable(LinkSlot::AllUsers) => log::info!(
                "Windows notifier: could not read {:?} to see which exe it names, leaving it unchanged",
                all_users_link
            ),
            LinkRefusal::Unreadable(LinkSlot::PerUser) => log::info!(
                "Windows notifier: could not read {:?} to see which exe it names, leaving it unchanged",
                link
            ),
            LinkRefusal::NamesOtherExe(LinkSlot::AllUsers, target) => log::info!(
                "Windows notifier: {:?} already names {}, so this build leaves it alone",
                all_users_link,
                target.display()
            ),
            LinkRefusal::NamesOtherExe(LinkSlot::PerUser, target) => log::info!(
                "Windows notifier: {:?} already names {}, so this build leaves it alone",
                link,
                target.display()
            ),
        }
        return;
    }
    if let LinkState::Targets(target) = &per_user_state {
        if is_installed_exe(this_exe, &roots, resolve_path)
            && !should_claim(Some(target.as_path()), this_exe, Path::exists)
        {
            log::info!(
                "Windows notifier: this installed copy replaces the Start Menu shortcut {:?}, which names {}",
                link,
                target.display()
            );
        }
    }
    let _ = std::fs::create_dir_all(&programs);
    if let Err(error) = install_shortcut(&link_path, exe_path) {
        log::warn!(
            "Windows notifier: could not write Start Menu shortcut for toasts: {:?}",
            error
        );
    }
}

fn show_toast(work: &ToastWork) -> Result<(), windows::core::Error> {
    let notifier: ToastNotifier =
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID))?;
    let launch = build_launch_url(&work.network_id, &work.target, &work.msgid);
    let xml = build_toast_xml(&work.summary, &work.body, &launch);
    let document = XmlDocument::new()?;
    document.LoadXml(&HSTRING::from(&xml))?;
    let notification = ToastNotification::CreateToastNotification(&document)?;
    let tag = conversation_tag(&work.network_id, &work.target);
    notification.SetTag(&HSTRING::from(&tag))?;
    notification.SetGroup(&HSTRING::from(&tag))?;
    notifier.Show(&notification)?;
    Ok(())
}

fn post_activation(invoked_args: &str) {
    let Some((network_id, target, msgid)) = parse_launch_url(invoked_args) else {
        return;
    };
    if let Some(tx) = ACTIVATION_TX.get() {
        let _ = tx.send_blocking(Activation {
            network_id,
            target,
            msgid,
        });
    }
}

#[implement(INotificationActivationCallback)]
struct NotificationActivator;

impl INotificationActivationCallback_Impl for NotificationActivator_Impl {
    fn Activate(
        &self,
        _appusermodelid: &PCWSTR,
        invokedargs: &PCWSTR,
        _data: *const NOTIFICATION_USER_INPUT_DATA,
        _count: u32,
    ) -> windows::core::Result<()> {
        let args = unsafe { invokedargs.to_string()? };
        post_activation(&args);
        Ok(())
    }
}

#[implement(IClassFactory)]
struct ActivatorFactory;

impl IClassFactory_Impl for ActivatorFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: windows::core::Ref<'_, windows::core::IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut std::ffi::c_void,
    ) -> windows::core::Result<()> {
        if !punkouter.is_null() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let activator: INotificationActivationCallback = NotificationActivator.into();
        unsafe { activator.query(riid, ppvobject).ok() }
    }

    fn LockServer(&self, flock: windows::core::BOOL) -> windows::core::Result<()> {
        if flock.as_bool() {
            unsafe { CoAddRefServerProcess() };
        } else {
            unsafe { CoReleaseServerProcess() };
        }
        Ok(())
    }
}

fn pump_com_messages() {
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            // Zero means no character message was posted, not an error, so the BOOL is ignored.
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn run_notification_thread(stop: usize, toast_event: usize, enabled: Arc<std::sync::atomic::AtomicBool>) {
    let stop = HANDLE(stop as *mut std::ffi::c_void);
    let toast_event = HANDLE(toast_event as *mut std::ffi::c_void);
    let ro_initialized = unsafe { RoInitialize(RO_INIT_MULTITHREADED).is_ok() };
    let class_cookie = if ro_initialized {
        if let Ok(exe) = std::env::current_exe() {
            ensure_notification_registration(&exe.to_string_lossy());
        }
        let factory: IClassFactory = ActivatorFactory.into();
        let cookie = unsafe {
            CoRegisterClassObject(
                &ACTIVATOR_CLSID,
                &factory,
                CLSCTX_LOCAL_SERVER,
                REGCLS_MULTIPLEUSE,
            )
        };
        if cookie.is_ok() {
            unsafe { CoAddRefServerProcess() };
            enabled.store(true, std::sync::atomic::Ordering::Release);
        }
        cookie.ok()
    } else {
        log::warn!("Windows notifier: RoInitialize failed, so Windows toasts are unavailable");
        None
    };
    let queue = TOAST_QUEUE.get_or_init(|| Mutex::new(VecDeque::new()));
    let handles = [stop, toast_event];
    loop {
        let wait = unsafe {
            MsgWaitForMultipleObjects(Some(&handles), false, u32::MAX, QS_ALLINPUT)
        };
        match wait {
            WAIT_OBJECT_0 => break,
            windows::Win32::Foundation::WAIT_EVENT(1) => {
                let pending: Vec<ToastWork> =
                    queue.lock().expect("toast queue").drain(..).collect();
                for work in pending {
                    let _ = show_toast(&work);
                }
            }
            windows::Win32::Foundation::WAIT_EVENT(2) => pump_com_messages(),
            _ => break,
        }
    }
    if let Some(class_cookie) = class_cookie {
        unsafe {
            if let Err(error) = CoRevokeClassObject(class_cookie) {
                log::warn!(
                    "Windows notifier: could not revoke the toast activator class: {:?}",
                    error
                );
            }
            CoReleaseServerProcess();
        }
    }
    if ro_initialized {
        unsafe { RoUninitialize() };
    }
}

pub fn open(tx: Sender<Activation>) -> Box<dyn Notifier> {
    let _ = ACTIVATION_TX.set(tx);
    let enabled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let enabled_thread = Arc::clone(&enabled);
    let stop = unsafe { CreateEventW(None, false, false, None) }
        .expect("toast stop event");
    let toast_event = unsafe { CreateEventW(None, false, false, None) }
        .expect("toast wake event");
    let _ = TOAST_EVENT.set(toast_event.0 as usize);
    let stop_handle = stop.0 as usize;
    let toast_handle = toast_event.0 as usize;
    thread::spawn(move || run_notification_thread(stop_handle, toast_handle, enabled_thread));
    Box::new(WindowsNotifier { enabled })
}

/// Parses a toast foreground `launch` argument from the process command line, if present.
pub fn activation_from_args(args: &[String]) -> Option<Activation> {
    for arg in args {
        if let Some((network_id, target, msgid)) = parse_launch_url(arg) {
            return Some(Activation {
                network_id,
                target,
                msgid,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        check_existing_links, expand_environment, is_installed_exe, link_state_from_target,
        registered_exe, repair_ends_registration, should_claim, strip_verbatim, LinkRefusal,
        LinkSlot, LinkState,
    };

    const THIS_EXE: &str = r"C:\dev\omairc\omairc-poc-gpui.exe";

    /// Stands in for the disk where a test needs no resolution: nothing has a canonical form.
    fn no_resolve(_: &Path) -> Option<PathBuf> {
        None
    }

    /// Stands in for `canonicalize` on a machine where `C:\PROGRA~1` names `C:\Program Files`.
    fn expand_8_3(path: &Path) -> Option<PathBuf> {
        let rest = path.to_str()?.strip_prefix(r"C:\PROGRA~1\")?;
        Some(PathBuf::from(format!(r"C:\Program Files\{rest}")))
    }

    /// Stands in for `canonicalize` by adding the verbatim prefix it returns on Windows.
    fn verbatim(path: &Path) -> Option<PathBuf> {
        Some(PathBuf::from(format!(r"\\?\{}", path.display())))
    }

    #[test]
    fn nothing_registered_is_claimed() {
        assert!(should_claim(None, Path::new(THIS_EXE), |_| false));
    }

    #[test]
    fn the_same_exe_is_claimed_regardless_of_existence() {
        let this_exe = Path::new(THIS_EXE);
        assert!(should_claim(Some(this_exe), this_exe, |_| false));
    }

    #[test]
    fn a_different_exe_that_still_exists_is_not_claimed() {
        let installed = Path::new(r"C:\Program Files\Omairc\omairc.exe");
        assert!(!should_claim(
            Some(installed),
            Path::new(THIS_EXE),
            |path| path == installed,
        ));
    }

    #[test]
    fn a_different_exe_that_is_gone_is_claimed() {
        let gone = Path::new(r"C:\old\omairc\omairc.exe");
        assert!(should_claim(
            Some(gone),
            Path::new(THIS_EXE),
            |path| path != gone,
        ));
    }

    #[test]
    fn a_quoted_registry_value_is_compared_by_its_exe() {
        let this_exe = Path::new(THIS_EXE);
        let same = registered_exe(&format!("\"{THIS_EXE}\""));
        assert!(should_claim(same.as_deref(), this_exe, |_| false));

        let installed_exe = Path::new(r"C:\Program Files\Omairc\omairc.exe");
        let installed = registered_exe(r#""C:\Program Files\Omairc\omairc.exe" -Embedding"#);
        assert!(!should_claim(
            installed.as_deref(),
            this_exe,
            |path| path == installed_exe,
        ));
    }

    #[test]
    fn an_empty_registry_value_names_no_exe_and_is_claimed() {
        assert_eq!(registered_exe("   "), None);
        assert!(should_claim(
            registered_exe("\"\"").as_deref(),
            Path::new(THIS_EXE),
            |_| false,
        ));
    }

    const PER_USER_ROOT: &str = r"C:\Users\Fredi\AppData\Local\Programs\Omairc";
    const ALL_USERS_ROOT: &str = r"C:\Program Files\Omairc";

    fn test_roots() -> Vec<PathBuf> {
        vec![PathBuf::from(PER_USER_ROOT), PathBuf::from(ALL_USERS_ROOT)]
    }

    #[test]
    fn an_exe_under_the_per_user_root_is_installed() {
        let exe = Path::new(r"C:\Users\Fredi\AppData\Local\Programs\Omairc\omairc.exe");
        assert!(is_installed_exe(exe, &test_roots(), no_resolve));
    }

    #[test]
    fn an_exe_under_the_all_users_root_is_installed() {
        let exe = Path::new(r"C:\Program Files\Omairc\omairc.exe");
        assert!(is_installed_exe(exe, &test_roots(), no_resolve));
    }

    #[test]
    fn a_dev_build_in_the_checkout_is_not_installed() {
        let exe = Path::new(r"D:\code\omairc\poc\target\debug\omairc-poc-gpui.exe");
        assert!(!is_installed_exe(exe, &test_roots(), no_resolve));
    }

    #[test]
    fn a_lookalike_root_is_not_installed() {
        let exe = Path::new(r"C:\Users\Fredi\AppData\Local\Programs\Omairc-dev\x.exe");
        assert!(!is_installed_exe(exe, &test_roots(), no_resolve));
    }

    #[test]
    fn root_matching_ignores_ascii_case() {
        let exe = Path::new(r"c:\users\fredi\appdata\local\programs\OMAIRC\omairc.exe");
        assert!(is_installed_exe(exe, &test_roots(), no_resolve));
    }

    #[test]
    fn a_verbatim_exe_path_matches_the_plain_root() {
        let exe = Path::new(r"\\?\C:\Users\Fredi\AppData\Local\Programs\Omairc\omairc.exe");
        assert!(is_installed_exe(exe, &test_roots(), no_resolve));
    }

    #[test]
    fn a_verbatim_root_matches_the_plain_exe_path() {
        let roots = [PathBuf::from(r"\\?\C:\Program Files\Omairc")];
        let exe = Path::new(r"C:\Program Files\Omairc\omairc.exe");
        assert!(is_installed_exe(exe, &roots, no_resolve));
    }

    #[test]
    fn a_resolved_verbatim_path_matches_the_plain_root() {
        // `canonicalize` returns the verbatim form, so the strip must run after resolution.
        let exe = Path::new(r"C:\Program Files\Omairc\omairc.exe");
        assert!(is_installed_exe(exe, &test_roots(), verbatim));
    }

    #[test]
    fn an_8_3_exe_path_matches_its_long_path_through_resolution() {
        let short = Path::new(r"C:\PROGRA~1\Omairc\omairc.exe");
        assert!(is_installed_exe(short, &test_roots(), expand_8_3));
        assert!(
            !is_installed_exe(short, &test_roots(), no_resolve),
            "without resolution the short name is refused, which fails safe"
        );
    }

    #[test]
    fn a_unc_verbatim_path_strips_to_a_unc_path() {
        assert_eq!(
            strip_verbatim(Path::new(r"\\?\UNC\server\share\omairc.exe")),
            PathBuf::from(r"\\server\share\omairc.exe"),
        );
    }

    #[test]
    fn a_parent_dir_cannot_leave_the_root() {
        let exe = Path::new(r"C:\Users\Fredi\AppData\Local\Programs\Omairc\..\..\dev\omairc.exe");
        assert!(!is_installed_exe(exe, &test_roots(), no_resolve));
    }

    #[test]
    fn no_roots_or_an_empty_root_matches_nothing() {
        let exe = Path::new(THIS_EXE);
        assert!(!is_installed_exe(exe, &[], no_resolve));
        assert!(!is_installed_exe(exe, &[PathBuf::new()], no_resolve));
    }

    const OTHER_EXE: &str = r"C:\Program Files\Other\omairc.exe";

    fn targets(path: &str) -> LinkState {
        LinkState::Targets(PathBuf::from(path))
    }

    fn identity(raw: &str) -> Option<String> {
        Some(raw.to_owned())
    }

    /// Expands the one variable these tests use, `%ROOT%`, to the other install's folder.
    fn expand_root(raw: &str) -> Option<String> {
        Some(raw.replace("%ROOT%", r"C:\Program Files\Other"))
    }

    const ROOT_TARGET: &str = r"%ROOT%\omairc.exe";

    #[test]
    fn with_no_links_the_fallback_may_write() {
        let this_exe = Path::new(THIS_EXE);
        assert_eq!(
            check_existing_links(
                this_exe,
                &[],
                &LinkState::Missing,
                &LinkState::Missing,
                |_| false,
                no_resolve,
            ),
            Ok(()),
        );
    }

    #[test]
    fn an_all_users_link_naming_this_exe_allows() {
        let this_exe = Path::new(THIS_EXE);
        assert_eq!(
            check_existing_links(
                this_exe,
                &[],
                &targets(THIS_EXE),
                &LinkState::Missing,
                |_| true,
                no_resolve,
            ),
            Ok(()),
        );
    }

    #[test]
    fn an_all_users_link_naming_another_existing_exe_refuses() {
        let this_exe = Path::new(THIS_EXE);
        let other = Path::new(OTHER_EXE);
        assert_eq!(
            check_existing_links(
                this_exe,
                &[],
                &targets(OTHER_EXE),
                &LinkState::Missing,
                |path| path == other,
                no_resolve,
            ),
            Err(LinkRefusal::NamesOtherExe(LinkSlot::AllUsers, PathBuf::from(OTHER_EXE))),
        );
    }

    #[test]
    fn an_all_users_link_naming_a_gone_exe_allows() {
        let this_exe = Path::new(THIS_EXE);
        assert_eq!(
            check_existing_links(
                this_exe,
                &[],
                &targets(OTHER_EXE),
                &LinkState::Missing,
                |_| false,
                no_resolve,
            ),
            Ok(()),
        );
    }

    #[test]
    fn an_unreadable_all_users_link_refuses() {
        let this_exe = Path::new(THIS_EXE);
        assert_eq!(
            check_existing_links(
                this_exe,
                &[],
                &LinkState::Unreadable,
                &LinkState::Missing,
                |_| false,
                no_resolve,
            ),
            Err(LinkRefusal::Unreadable(LinkSlot::AllUsers)),
        );
    }

    #[test]
    fn a_per_user_link_naming_another_existing_exe_refuses() {
        let this_exe = Path::new(THIS_EXE);
        let other = Path::new(OTHER_EXE);
        assert_eq!(
            check_existing_links(
                this_exe,
                &[],
                &LinkState::Missing,
                &targets(OTHER_EXE),
                |path| path == other,
                no_resolve,
            ),
            Err(LinkRefusal::NamesOtherExe(LinkSlot::PerUser, PathBuf::from(OTHER_EXE))),
        );
    }

    #[test]
    fn an_installed_copy_is_allowed_past_another_exes_link() {
        let installed = Path::new(r"C:\Users\Fredi\AppData\Local\Programs\Omairc\omairc.exe");
        let other = Path::new(OTHER_EXE);
        assert_eq!(
            check_existing_links(
                installed,
                &test_roots(),
                &targets(OTHER_EXE),
                &LinkState::Missing,
                |path| path == other,
                no_resolve,
            ),
            Ok(()),
        );
    }

    #[test]
    fn an_empty_target_is_unreadable() {
        assert_eq!(link_state_from_target(Some(""), identity), LinkState::Unreadable);
    }

    #[test]
    fn a_failed_target_read_is_unreadable() {
        assert_eq!(link_state_from_target(None, identity), LinkState::Unreadable);
    }

    #[test]
    fn an_unexpanded_variable_is_unreadable() {
        assert_eq!(link_state_from_target(Some(ROOT_TARGET), identity), LinkState::Unreadable);
    }

    #[test]
    fn a_target_expanding_to_another_existing_exe_refuses() {
        let state = link_state_from_target(Some(ROOT_TARGET), expand_root);
        assert_eq!(state, targets(OTHER_EXE));
        let other = Path::new(OTHER_EXE);
        assert_eq!(
            check_existing_links(
                Path::new(THIS_EXE),
                &[],
                &LinkState::Missing,
                &state,
                |path| path == other,
                no_resolve,
            ),
            Err(LinkRefusal::NamesOtherExe(LinkSlot::PerUser, PathBuf::from(OTHER_EXE))),
        );
    }

    #[test]
    fn a_target_expanding_to_a_gone_exe_is_claimed() {
        let state = link_state_from_target(Some(ROOT_TARGET), expand_root);
        assert_eq!(
            check_existing_links(
                Path::new(THIS_EXE),
                &[],
                &LinkState::Missing,
                &state,
                |_| false,
                no_resolve,
            ),
            Ok(()),
        );
    }

    #[test]
    fn an_expansion_failure_refuses() {
        let state = link_state_from_target(Some(ROOT_TARGET), |_| None);
        assert_eq!(
            check_existing_links(
                Path::new(THIS_EXE),
                &[],
                &LinkState::Missing,
                &state,
                |_| false,
                no_resolve,
            ),
            Err(LinkRefusal::Unreadable(LinkSlot::PerUser)),
        );
    }

    const INSTALLED_EXE: &str = r"C:\Program Files\Omairc\omairc.exe";
    /// A dev exe in another checkout, which still exists.
    const DEV_EXE: &str = r"D:\other-checkout\poc\target\debug\omairc-poc-gpui.exe";

    #[test]
    fn an_unreadable_per_user_link_refuses() {
        assert_eq!(
            check_existing_links(
                Path::new(THIS_EXE),
                &[],
                &LinkState::Missing,
                &LinkState::Unreadable,
                |_| false,
                no_resolve,
            ),
            Err(LinkRefusal::Unreadable(LinkSlot::PerUser)),
        );
    }

    #[test]
    fn an_installed_copy_refused_by_an_unreadable_link_does_not_overwrite() {
        let other = Path::new(OTHER_EXE);
        assert_eq!(
            check_existing_links(
                Path::new(INSTALLED_EXE),
                &test_roots(),
                &targets(OTHER_EXE),
                &LinkState::Unreadable,
                |path| path == other,
                no_resolve,
            ),
            Err(LinkRefusal::Unreadable(LinkSlot::PerUser)),
        );
    }

    #[test]
    fn when_both_links_refuse_the_all_users_refusal_is_reported() {
        let this_exe = Path::new(THIS_EXE);
        let other = Path::new(OTHER_EXE);
        assert_eq!(
            check_existing_links(
                this_exe,
                &[],
                &LinkState::Unreadable,
                &LinkState::Unreadable,
                |_| false,
                no_resolve,
            ),
            Err(LinkRefusal::Unreadable(LinkSlot::AllUsers)),
        );
        assert_eq!(
            check_existing_links(
                this_exe,
                &[],
                &targets(OTHER_EXE),
                &LinkState::Unreadable,
                |path| path == other,
                no_resolve,
            ),
            Err(LinkRefusal::NamesOtherExe(LinkSlot::AllUsers, PathBuf::from(OTHER_EXE))),
        );
    }

    #[test]
    fn an_installed_copy_overwrites_a_dev_per_user_link_past_its_own_all_users_link() {
        let dev = Path::new(DEV_EXE);
        assert_eq!(
            check_existing_links(
                Path::new(INSTALLED_EXE),
                &test_roots(),
                &targets(INSTALLED_EXE),
                &targets(DEV_EXE),
                |path| path == dev,
                no_resolve,
            ),
            Ok(()),
        );
    }

    #[test]
    fn a_dev_build_does_not_overwrite_a_dev_per_user_link_past_its_own_all_users_link() {
        let dev = Path::new(DEV_EXE);
        assert_eq!(
            check_existing_links(
                Path::new(THIS_EXE),
                &test_roots(),
                &targets(THIS_EXE),
                &targets(DEV_EXE),
                |path| path == dev,
                no_resolve,
            ),
            Err(LinkRefusal::NamesOtherExe(LinkSlot::PerUser, PathBuf::from(DEV_EXE))),
        );
    }

    #[test]
    fn without_a_repair_nothing_stops_the_registration() {
        assert!(!repair_ends_registration(false, false, false));
        assert!(!repair_ends_registration(false, true, false));
    }

    #[test]
    fn a_dev_build_stops_after_a_repair() {
        assert!(repair_ends_registration(true, false, false));
    }

    #[test]
    fn an_installed_copy_goes_on_after_a_repair_until_its_per_user_link_is_settled() {
        assert!(!repair_ends_registration(true, true, false));
        assert!(repair_ends_registration(true, true, true));
    }

    #[test]
    fn expand_environment_expands_a_defined_variable() {
        let root = std::env::var("SystemRoot").expect("SystemRoot is set on Windows");
        assert_eq!(
            expand_environment(r"%SystemRoot%\omairc.exe"),
            Some(format!(r"{root}\omairc.exe")),
        );
    }

    #[test]
    fn expand_environment_leaves_an_undefined_variable_in_place_and_it_is_unreadable() {
        let raw = r"%OMAIRC_TEST_NO_SUCH_VARIABLE%\omairc.exe";
        assert_eq!(expand_environment(raw), Some(raw.to_owned()));
        assert_eq!(
            link_state_from_target(Some(raw), expand_environment),
            LinkState::Unreadable,
        );
    }
}
