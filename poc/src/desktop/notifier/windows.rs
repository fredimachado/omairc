//! Windows Toast notifications. Port of `windowsnotifications.cpp`.
//!
//! Toasts rest on two per-user records that every build of Omairc shares: an AUMID-stamped
//! Start Menu shortcut, and the COM activator registered to the exe that handles a click.
//! The installed copy owns both. A launch takes them only when nothing is recorded, when the
//! record already names this exe, or when the recorded exe no longer exists (see
//! `util::should_claim`). A `cargo run` build therefore still shows toasts through the
//! installed shortcut, but it leaves the activator and the Start Menu link to that copy, so
//! clicking a toast raises the installed app rather than the dev build.

use std::collections::VecDeque;
use std::ffi::OsStr;
use std::os::windows::prelude::OsStrExt;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use async_channel::Sender;
use std::mem::ManuallyDrop;
use std::path::{Path, PathBuf};

use windows::core::{GUID, HSTRING, Interface, PCWSTR};
use windows::Win32::Foundation::{CLASS_E_NOAGGREGATION, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ToastActivatorCLSID;
use windows::Win32::System::Com::{
    CoAddRefServerProcess, CoCreateInstance, CoRegisterClassObject, CoReleaseServerProcess,
    CoRevokeClassObject, CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER, IClassFactory,
    IClassFactory_Impl, IPersistFile, STGM_READ, REGCLS_MULTIPLEUSE,
};
use windows::Win32::System::Com::StructuredStorage::{
    InitPropVariantFromCLSID, PropVariantClear, PROPVARIANT, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
};
use windows::Win32::System::Variant::{VT_CLSID, VT_LPWSTR};
use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};
use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
use windows::Win32::System::Threading::{CreateEventW, SetEvent};
use windows::Win32::UI::Notifications::{
    INotificationActivationCallback, INotificationActivationCallback_Impl,
    NOTIFICATION_USER_INPUT_DATA,
};
use windows::Win32::UI::Shell::{IShellLinkW, SHStrDupW, ShellLink, SLGP_RAWPATH};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MsgWaitForMultipleObjects, PeekMessageW, QS_ALLINPUT, TranslateMessage,
    MSG, PM_REMOVE,
};
use windows_implement::implement;
use windows::Data::Xml::Dom::XmlDocument;
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager, ToastNotifier};

use super::util::{
    build_launch_url, build_toast_xml, command_exe, conversation_tag, parse_launch_url,
    should_claim,
};
use super::{Activation, Notifier};

const AUMID: &str = "FrediMachado.Omairc";
const ACTIVATOR_CLSID: GUID = GUID::from_u128(0xDBE38477_5F87_42A1_AFA1_11FA12F2E5E5);

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
                    log::warn!("Windows notifier: could not wake the toast thread: {error:?}");
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

fn same_exe_path(left: &str, right: &str) -> bool {
    let left = Path::new(left);
    let right = Path::new(right);
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left
            .to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy()),
    }
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
    let mut target = [0u16; 260];
    if unsafe {
        shell_link.GetPath(&mut target, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32)
    }
    .is_err()
    {
        return None;
    }
    let len = target.iter().position(|&ch| ch == 0).unwrap_or(target.len());
    Some(String::from_utf16_lossy(&target[..len]))
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
        if same_exe_path(&target, exe_path) {
            links.push(link);
        }
    }
    links
}

/// The activator's `LocalServer32` value, which Windows launches when a toast is clicked.
const COM_SERVER_SUBKEY: &str =
    "SOFTWARE\\Classes\\CLSID\\{DBE38477-5F87-42A1-AFA1-11FA12F2E5E5}\\LocalServer32";

/// The exe the activator's `LocalServer32` value names, if it names one.
fn registered_com_server() -> Option<PathBuf> {
    let key = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(COM_SERVER_SUBKEY)
        .ok()?;
    let command: String = key.get_value("").ok()?;
    command_exe(&command)
}

/// Points the activator at `exe`, unless another exe that is still on disk owns it.
fn register_com_server(exe: &str) {
    let existing = registered_com_server();
    if !should_claim(existing.as_deref(), Path::new(exe), Path::exists) {
        log::info!(
            "Windows notifier: the toast activator stays registered to {:?}",
            existing
        );
        return;
    }
    let command = format!("\"{}\"", exe.replace('/', "\\"));
    let subkey_wide = wide(COM_SERVER_SUBKEY);
    let command_wide = wide(&command);
    unsafe {
        let _ = RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey_wide.as_ptr()),
            None,
            REG_SZ.0,
            Some(command_wide.as_ptr() as *const _),
            (command_wide.len() * 2) as u32,
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

fn ensure_notification_registration(exe_path: &str) {
    register_com_server(exe_path);

    let installed = installed_shortcut_paths(exe_path);
    for link in &installed {
        if shortcut_is_stamped(&link.to_string_lossy()) {
            return;
        }
    }
    for link in &installed {
        let link_path = link.to_string_lossy();
        if install_shortcut(&link_path, exe_path).is_ok() {
            return;
        }
        log::warn!(
            "Windows notifier: could not stamp installed Start Menu shortcut {:?}",
            link
        );
    }

    let Some(programs) = dirs::home_dir().map(|home| {
        home.join("AppData/Roaming/Microsoft/Windows/Start Menu/Programs")
    }) else {
        return;
    };
    let _ = std::fs::create_dir_all(&programs);
    let link = programs.join("Omairc.lnk");
    let owner = if link.is_file() {
        shortcut_target(&link.to_string_lossy()).map(PathBuf::from)
    } else {
        None
    };
    if !should_claim(owner.as_deref(), Path::new(exe_path), Path::exists) {
        log::info!(
            "Windows notifier: {:?} stays with the copy it names, {:?}",
            link,
            owner
        );
        return;
    }
    if let Err(error) = install_shortcut(&link.to_string_lossy(), exe_path) {
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
            TranslateMessage(&msg);
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
                log::warn!("Windows notifier: could not unregister the toast activator: {error:?}");
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
