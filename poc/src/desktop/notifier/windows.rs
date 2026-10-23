//! Windows Toast notifications. Port of `windowsnotifications.cpp`.

use std::collections::VecDeque;
use std::ffi::OsStr;
use std::os::windows::prelude::OsStrExt;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use async_channel::Sender;
use windows::core::{GUID, HSTRING, Interface, PCWSTR};
use windows::Win32::Foundation::{CLASS_E_NOAGGREGATION, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ToastActivatorCLSID;
use windows::Win32::System::Com::{
    CoAddRefServerProcess, CoCreateInstance, CoInitializeEx, CoRegisterClassObject,
    CoReleaseServerProcess, CoRevokeClassObject, CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER,
    COINIT_MULTITHREADED, IClassFactory, IClassFactory_Impl, IPersistFile, REGCLS_MULTIPLEUSE,
};
use windows::Win32::System::Com::StructuredStorage::{
    InitPropVariantFromCLSID, InitPropVariantFromStringAsVector, PropVariantClear,
};
use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
use windows::Win32::System::Threading::{CreateEventW, SetEvent};
use windows::Win32::UI::Notifications::{
    INotificationActivationCallback, INotificationActivationCallback_Impl,
    NOTIFICATION_USER_INPUT_DATA,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
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
                unsafe { SetEvent(handle) };
            }
        }
    }
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain([0]).collect()
}

fn register_com_server(exe: &str) {
    let subkey =
        "SOFTWARE\\Classes\\CLSID\\{DBE38477-5F87-42A1-AFA1-11FA12F2E5E5}\\LocalServer32";
    let command = format!("\"{}\"", exe.replace('/', "\\"));
    let subkey_wide = wide(subkey);
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

    let store: IPropertyStore = unsafe { shell_link.cast()? };
    let aumid_wide = wide(AUMID);
    let mut aumid_prop =
        unsafe { InitPropVariantFromStringAsVector(PCWSTR(aumid_wide.as_ptr()))? };
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

    let persist: IPersistFile = unsafe { shell_link.cast()? };
    let link_wide = wide(link_path);
    unsafe { persist.Save(PCWSTR(link_wide.as_ptr()), true) }
}

fn ensure_notification_registration(exe_path: &str) {
    register_com_server(exe_path);
    let programs = dirs::home_dir()
        .map(|home| home.join("AppData/Roaming/Microsoft/Windows/Start Menu/Programs"));
    let Some(programs) = programs else {
        return;
    };
    let link = programs.join("Omairc.lnk");
    if link.exists() {
        return;
    }
    let _ = std::fs::create_dir_all(&programs);
    let _ = install_shortcut(&link.to_string_lossy(), exe_path);
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
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
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
    let queue = TOAST_QUEUE.get_or_init(|| Mutex::new(VecDeque::new()));
    let handles = [stop, toast_event];
    let class_cookie = cookie.ok();
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
            CoRevokeClassObject(class_cookie);
            CoReleaseServerProcess();
        }
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
