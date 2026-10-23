//! Windows Toast notifications. Port of `windowsnotifications.cpp` (show path and launch URL).

use std::collections::VecDeque;
use std::ffi::OsStr;
use std::os::windows::prelude::OsStrExt;
use std::sync::mpsc::{self, Sender as ThreadSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use async_channel::Sender;
use windows::core::HSTRING;
use windows::Data::Xml::Dom::XmlDocument;
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager, ToastNotifier};

use super::util::{build_launch_url, build_toast_xml, conversation_tag, parse_launch_url};
use super::{Activation, Notifier};

const AUMID: &str = "FrediMachado.Omairc";

struct ToastWork {
    summary: String,
    body: String,
    network_id: String,
    target: String,
    msgid: String,
}

struct WindowsNotifier {
    wake: ThreadSender<()>,
    enabled: Arc<std::sync::atomic::AtomicBool>,
}

static TOAST_QUEUE: OnceLock<Mutex<VecDeque<ToastWork>>> = OnceLock::new();

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
            let _ = self.wake.send(());
        }
    }
}

fn register_com_server(exe: &str) {
    use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    let subkey =
        "SOFTWARE\\Classes\\CLSID\\{DBE38477-5F87-42A1-AFA1-11FA12F2E5E5}\\LocalServer32";
    let command = format!("\"{}\"", exe.replace('/', "\\"));
    let subkey_wide: Vec<u16> = OsStr::new(subkey).encode_wide().chain([0]).collect();
    let command_wide: Vec<u16> = OsStr::new(&command).encode_wide().chain([0]).collect();
    unsafe {
        let _ = RegSetKeyValueW(
            HKEY_CURRENT_USER,
            windows::core::PCWSTR(subkey_wide.as_ptr()),
            None,
            REG_SZ.0,
            Some(command_wide.as_ptr() as *const _),
            (command_wide.len() * 2) as u32,
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

fn run_notification_thread(wake: mpsc::Receiver<()>, enabled: Arc<std::sync::atomic::AtomicBool>) {
    if let Ok(exe) = std::env::current_exe() {
        register_com_server(&exe.to_string_lossy());
    }
    enabled.store(true, std::sync::atomic::Ordering::Release);
    let queue = TOAST_QUEUE.get_or_init(|| Mutex::new(VecDeque::new()));
    while wake.recv().is_ok() {
        let pending: Vec<ToastWork> = queue.lock().expect("toast queue").drain(..).collect();
        for work in pending {
            let _ = show_toast(&work);
        }
    }
}

pub fn open(_tx: Sender<Activation>) -> Box<dyn Notifier> {
    let (wake_tx, wake_rx) = mpsc::channel();
    let enabled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let enabled_thread = Arc::clone(&enabled);
    thread::spawn(move || run_notification_thread(wake_rx, enabled_thread));
    Box::new(WindowsNotifier {
        wake: wake_tx,
        enabled,
    })
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
