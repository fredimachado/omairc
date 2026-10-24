//! Windows Toast notifications. Port of `windowsnotifications.cpp`.
//!
//! Toasts use the AUMID `FrediMachado.Omairc`. Activation goes through two per-user records:
//! the activator registration (`LocalServer32`) and a Start Menu `Omairc.lnk` that carries the
//! AUMID. Which exe those records name is decided by two rules, both implemented in
//! `windows_ownership`:
//!
//! - An installed copy, one under `%LOCALAPPDATA%\Programs\Omairc` or `%ProgramFiles%\Omairc`,
//!   takes the activator registration and the per-user link even from an exe that still exists.
//!   A record it cannot read is left alone (see below).
//! - Any other build, such as a dev or portable build, takes them only when nothing is
//!   registered, when the registered exe is itself, or when the registered exe is gone. It does
//!   not displace an exe that still exists (`util::should_claim`).
//!
//! Two installed copies take turns: each launch overwrites the other's registration and link,
//! and this is intended. A registration or link that cannot be read is left alone, for any
//! build, because its owner is unknown.
//!
//! The shortcut fallback reads two Start Menu `Omairc.lnk` links, the per-user one and the
//! all-users one under `FOLDERID_CommonPrograms`, and writes only the per-user link. It writes
//! only when neither link refuses. A link refuses when it cannot be read, when its target cannot
//! be expanded, when the target is empty or not valid UTF-16, or when `%` survives `%VAR%`
//! expansion (a literal `%` in a real path refuses too). For a build that is not installed, a
//! link also refuses when it names an exe that still exists and is not this one. The fallback
//! never creates an all-users link.
//!
//! Before the fallback, the repair path re-stamps a link that already names this exe, in either
//! location, but only when no such link is stamped yet. A readable link that names this exe
//! counts as found whether or not its stamp takes, and the post-repair gate reads that finding,
//! not the stamp. After a repair, a build that is not installed stops there, portable builds
//! included. An installed copy goes on only when its per-user link is readable and names another
//! exe. The fallback then replaces that per-user link, unless the all-users link is unreadable.
//! A missing or unreadable per-user link, or one that names this exe, stops there. So after a
//! repair, a per-user link is replaced, never created.
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
use std::path::{Path, PathBuf};

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
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::System::Registry::{
    RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
};
use windows::Win32::System::Threading::{CreateEventW, SetEvent};
use windows::Win32::System::Variant::{VT_CLSID, VT_LPWSTR};
use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};
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

use super::util::{
    build_launch_url, build_toast_xml, command_exe, conversation_tag, parse_launch_url, same_exe,
    should_claim,
};
use super::windows_ownership::{
    activator_claim, canonical_with, check_existing_links, found_after_repair, is_installed_exe,
    link_state_from_target, repair_ends_registration, LinkRefusal, LinkSlot, LinkState,
};
use super::{Activation, Notifier};

const AUMID: &str = "FrediMachado.Omairc";
const ACTIVATOR_CLSID: GUID = GUID::from_u128(0xDBE38477_5F87_42A1_AFA1_11FA12F2E5E5);
/// Wide-character buffer for a path up to the long-path limit (32767 characters) and its NUL.
const LONG_PATH_BUFFER: usize = 32_768;
/// The activator's `LocalServer32` value, which Windows launches when a toast is clicked.
const COM_SERVER_SUBKEY: &str =
    "SOFTWARE\\Classes\\CLSID\\{DBE38477-5F87-42A1-AFA1-11FA12F2E5E5}\\LocalServer32";

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

/// The canonical form of `path`, which resolves 8.3 short names and junctions. `None` when the
/// disk cannot resolve it, as when it does not exist.
fn resolve_path(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

/// The spelling of `path` that the ownership rules compare: its canonical form, with the verbatim
/// prefix stripped (see `windows_ownership::canonical_with`). Every exe read from disk or the
/// registry goes through here before a rule sees it, so `util::same_exe` compares like with like.
fn canonical_exe(path: &Path) -> PathBuf {
    canonical_with(path, resolve_path)
}

/// Reads one Start Menu `Omairc.lnk`. Only a confirmed absence counts as `Missing`, so a
/// failure to check the folder leaves the link `Unreadable`. A target that names an exe is
/// returned in canonical form.
fn read_link_state(link: &Path) -> LinkState {
    match link.try_exists() {
        Ok(false) => LinkState::Missing,
        Ok(true) => match link_state_from_target(
            shortcut_target(&link.to_string_lossy()).as_deref(),
            expand_environment,
        ) {
            LinkState::Targets(target) => LinkState::Targets(canonical_exe(&target)),
            unreadable => unreadable,
        },
        Err(_) => LinkState::Unreadable,
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

/// Reads the default value of the activator's `LocalServer32` key. `Ok(None)` means the
/// value is absent. Any other failure is an error, so the caller can fail closed.
fn read_registered_command(subkey: &[u16]) -> Result<Option<String>, WIN32_ERROR> {
    let mut data = vec![0u16; LONG_PATH_BUFFER];
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

/// The readable per-user and all-users `Omairc.lnk` links whose target is this exe.
fn installed_shortcut_paths(exe_path: &str) -> Vec<PathBuf> {
    let this_exe = canonical_exe(Path::new(exe_path));
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
        if same_exe(&canonical_exe(Path::new(&target)), &this_exe) {
            links.push(link);
        }
    }
    links
}

/// Points the toast activator at this exe, when `activator_claim` allows it. An installed copy
/// always may. Any other build may only when `should_claim` allows it. A registration that cannot
/// be read is left unchanged, so the write is skipped.
fn register_com_server(exe: &str, install_roots: &[PathBuf]) {
    let subkey_wide = wide(COM_SERVER_SUBKEY);
    let existing = match read_registered_command(&subkey_wide) {
        Ok(command) => command
            .as_deref()
            .and_then(command_exe)
            .map(|owner| canonical_exe(&owner)),
        Err(status) => {
            log::info!(
                "Windows notifier: could not read the toast activator registration ({:?}), leaving it unchanged",
                status
            );
            return;
        }
    };
    let this_exe = canonical_exe(Path::new(exe));
    if !activator_claim(
        &this_exe,
        install_roots,
        existing.as_deref(),
        resolve_path,
        Path::exists,
    ) {
        if let Some(owner) = existing.as_deref() {
            log::info!(
                "Windows notifier: {} owns toast activation, so this build leaves it registered",
                owner.display()
            );
        }
        return;
    }
    if let Some(owner) = existing.as_deref() {
        if !should_claim(Some(owner), &this_exe, Path::exists) {
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

/// Repairs the Start Menu links that already name this exe. `installed` lists the readable links
/// that name it, in either location. The result is true when one of them is there, stamped or
/// not. A failed stamp is logged and does not change the result. See `found_after_repair`.
fn repair_installed_links(exe_path: &str, installed: &[PathBuf]) -> bool {
    found_after_repair(
        installed,
        |link| shortcut_is_stamped(&link.to_string_lossy()),
        |link| {
            let stamped = install_shortcut(&link.to_string_lossy(), exe_path).is_ok();
            if !stamped {
                log::warn!(
                    "Windows notifier: could not stamp installed Start Menu shortcut {:?}",
                    link
                );
            }
            stamped
        },
    )
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
    let this_exe = canonical_exe(Path::new(exe_path));
    let from_install = is_installed_exe(&this_exe, &roots, resolve_path);
    if repair_ends_registration(repaired, from_install, &this_exe, &per_user_state) {
        if from_install && matches!(per_user_state, LinkState::Unreadable) {
            log::info!(
                "Windows notifier: could not read {:?} to see which exe it names, leaving it unchanged",
                link
            );
        }
        return;
    }
    if let Err(refusal) = check_existing_links(
        &this_exe,
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
        if from_install && !should_claim(Some(target.as_path()), &this_exe, Path::exists) {
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
    args.iter().find_map(|arg| {
        let (network_id, target, msgid) = parse_launch_url(arg)?;
        Some(Activation {
            network_id,
            target,
            msgid,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::{expand_environment, link_state_from_target, LinkState};

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
