//! Desktop notifications on macOS through `UNUserNotificationCenter`. Without a bundle
//! identifier (`cargo run`), notifications stay disabled like the Qt app.

use std::sync::OnceLock;

use async_channel::Sender;
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, Allocated};
use objc2_foundation::{
    ns_string, NSArray, NSDictionary, NSObject, NSObjectProtocol, NSBundle, NSString,
};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
    UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};

use super::util::macos_request_identifier;
use super::{Activation, Notifier, NullNotifier};

static ACTIVATION_TX: OnceLock<Sender<Activation>> = OnceLock::new();

fn bundle_identifier() -> Option<Retained<NSString>> {
    NSBundle::mainBundle().bundleIdentifier()
}

fn notifications_available() -> bool {
    if std::env::var("OMAIRC_SKIP_NOTIFICATIONS").is_ok_and(|value| !value.is_empty()) {
        return false;
    }
    bundle_identifier().is_some()
}

fn string_from_user_info(user_info: &NSDictionary, key: &NSString) -> String {
    let value = unsafe { user_info.objectForKey_unchecked(key) }
        .and_then(|value| value.downcast_ref::<NSString>());
    value.map(|value| value.to_string()).unwrap_or_default()
}

fn post_activation(network_id: String, target: String, msgid: String) {
    if network_id.is_empty() || target.is_empty() {
        return;
    }
    if let Some(tx) = ACTIVATION_TX.get() {
        let _ = tx.send_blocking(Activation {
            network_id,
            target,
            msgid,
        });
    }
}

define_class!(
    #[unsafe(super = NSObject)]
    struct NotificationDelegate;

    unsafe impl NSObjectProtocol for NotificationDelegate {}

    unsafe impl UNUserNotificationCenterDelegate for NotificationDelegate {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion_handler: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion_handler.call(
                UNNotificationPresentationOptions::Banner | UNNotificationPresentationOptions::Sound,
            );
        }

        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            completion_handler: &block2::DynBlock<dyn Fn()>,
        ) {
            let user_info = response.notification().request().content().userInfo();
            let network_id = string_from_user_info(&user_info, ns_string!("networkId"));
            let target = string_from_user_info(&user_info, ns_string!("target"));
            let msgid = string_from_user_info(&user_info, ns_string!("msgid"));
            post_activation(network_id, target, msgid);
            completion_handler.call(());
        }
    }
);

impl NotificationDelegate {
    fn new() -> Retained<Self> {
        let allocated = NotificationDelegate::alloc();
        unsafe { msg_send![super(allocated), init] }
    }
}

struct MacNotifier {
    delegate: Retained<NotificationDelegate>,
}

impl Notifier for MacNotifier {
    fn notify(
        &self,
        summary: &str,
        body: &str,
        network_id: &str,
        target: &str,
        msgid: &str,
    ) {
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(summary));
        content.setBody(&NSString::from_str(body));
        let keys = [
            ns_string!("networkId"),
            ns_string!("target"),
            ns_string!("msgid"),
        ];
        let values = [
            NSString::from_str(network_id),
            NSString::from_str(target),
            NSString::from_str(msgid),
        ];
        let user_info = NSDictionary::from_retained_objects(&keys, &values);
        // SAFETY: plist-compatible NSString keys/values, same payload as Qt.
        unsafe {
            content.setUserInfo(&user_info);
        }

        let identifier = NSString::from_str(&macos_request_identifier(network_id, target));
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &identifier,
            &content,
            None,
        );
        let identifiers = NSArray::from_retained_objects(&[&identifier]);
        center.removePendingNotificationRequestsWithIdentifiers(&identifiers);
        center.removeDeliveredNotificationsWithIdentifiers(&identifiers);
        let block = RcBlock::new(|_error: *mut objc2_foundation::NSError| {});
        center.addNotificationRequest_withCompletionHandler(&request, Some(&block));
    }
}

pub fn open(tx: Sender<Activation>) -> Box<dyn Notifier> {
    if !notifications_available() {
        return Box::new(NullNotifier);
    }
    let _ = ACTIVATION_TX.set(tx);
    let center = UNUserNotificationCenter::currentNotificationCenter();
    let delegate = NotificationDelegate::new();
    center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    let options = UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound;
    let block = RcBlock::new(
        |_granted: objc2_foundation::Bool, _error: *mut objc2_foundation::NSError| {},
    );
    center.requestAuthorizationWithOptions_completionHandler(options, &block);
    Box::new(MacNotifier { delegate })
}
