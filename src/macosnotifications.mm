#include "macosnotifications.h"

#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>
#import <UserNotifications/UserNotifications.h>

#include <QByteArray>
#include <QMetaObject>
#include <QString>

namespace {
NSString *toNSString(const QString &value)
{
    return [NSString stringWithUTF8String:value.toUtf8().constData()];
}

QString fromNSString(NSString *value)
{
    if (!value)
        return {};
    return QString::fromUtf8([value UTF8String]);
}

NSString *conversationIdentifier(const QString &networkId, const QString &target)
{
    return toNSString(networkId + QLatin1Char('\n') + target);
}

bool notificationsAvailable()
{
    NSString *bundleId = [[NSBundle mainBundle] bundleIdentifier];
    if (!bundleId || bundleId.length == 0)
        return false;
    if (!qEnvironmentVariableIsEmpty("OMAIRC_SKIP_NOTIFICATIONS"))
        return false;
    if (qgetenv("QT_QPA_PLATFORM") == "offscreen")
        return false;
    if (!NSApp)
        return false;
    return true;
}
}

@interface OmaircNotificationDelegate : NSObject <UNUserNotificationCenterDelegate>
@property (nonatomic, assign) MacOsNotifications *owner;
@end

@implementation OmaircNotificationDelegate

- (void)userNotificationCenter:(UNUserNotificationCenter *)center
       didReceiveNotificationResponse:(UNNotificationResponse *)response
                withCompletionHandler:(void (^)(void))completionHandler
{
    MacOsNotifications *owner = self.owner;
    if (!owner) {
        if (completionHandler)
            completionHandler();
        return;
    }

    const NSDictionary *userInfo = response.notification.request.content.userInfo;
    const QString networkId = fromNSString(userInfo[@"networkId"]);
    const QString target = fromNSString(userInfo[@"target"]);
    const QString msgid = fromNSString(userInfo[@"msgid"]);
    if (!networkId.isEmpty() && !target.isEmpty()) {
        QMetaObject::invokeMethod(owner, [owner, networkId, target, msgid]() {
            emit owner->activated(networkId, target, msgid);
        }, Qt::QueuedConnection);
    }
    if (completionHandler)
        completionHandler();
}

- (void)userNotificationCenter:(UNUserNotificationCenter *)center
       willPresentNotification:(UNNotification *)notification
         withCompletionHandler:
             (void (^)(UNNotificationPresentationOptions options))completionHandler
{
    if (completionHandler)
        completionHandler(UNNotificationPresentationOptionBanner
                          | UNNotificationPresentationOptionSound);
}

@end

MacOsNotifications::MacOsNotifications(QObject *parent)
    : QObject(parent)
    , m_enabled(notificationsAvailable())
{
    if (!m_enabled)
        return;

    static OmaircNotificationDelegate *delegate = nil;
    static dispatch_once_t onceToken;
    dispatch_once(&onceToken, ^{
        delegate = [OmaircNotificationDelegate new];
    });
    delegate.owner = this;

    UNUserNotificationCenter *center = [UNUserNotificationCenter currentNotificationCenter];
    center.delegate = delegate;
    [center requestAuthorizationWithOptions:(UNAuthorizationOptionAlert | UNAuthorizationOptionSound)
                          completionHandler:^(BOOL granted, NSError *error) {
        (void)granted;
        (void)error;
    }];
}

MacOsNotifications::~MacOsNotifications()
{
    if (!m_enabled)
        return;

    UNUserNotificationCenter *center = [UNUserNotificationCenter currentNotificationCenter];
    if ([center.delegate isKindOfClass:[OmaircNotificationDelegate class]]) {
        OmaircNotificationDelegate *delegate =
            (OmaircNotificationDelegate *)center.delegate;
        if (delegate.owner == this)
            delegate.owner = nil;
    }
}

void MacOsNotifications::notify(const QString &summary, const QString &body,
                                const QString &networkId, const QString &target,
                                const QString &msgid)
{
    if (!m_enabled)
        return;

    UNUserNotificationCenter *center = [UNUserNotificationCenter currentNotificationCenter];
    if (!center)
        return;

    // packaging/macos/objc-arc.pri enables -fobjc-arc for this translation
    // unit. Keep an MRC release path so a missed flag cannot leak content.
    UNMutableNotificationContent *content =
        [[UNMutableNotificationContent alloc] init];
    content.title = toNSString(summary);
    content.body = toNSString(body);
    content.userInfo = @{
        @"networkId": toNSString(networkId),
        @"target": toNSString(target),
        @"msgid": toNSString(msgid),
    };

    NSString *identifier = conversationIdentifier(networkId, target);
    if (identifier.length == 0)
        identifier = [[NSUUID UUID] UUIDString];

    UNNotificationRequest *request =
        [UNNotificationRequest requestWithIdentifier:identifier
                                             content:content
                                             trigger:nil];
#if !__has_feature(objc_arc)
    [content release];
#endif
    [center removePendingNotificationRequestsWithIdentifiers:@[identifier]];
    [center removeDeliveredNotificationsWithIdentifiers:@[identifier]];
    [center addNotificationRequest:request
             withCompletionHandler:^(NSError *error) {
        (void)error;
    }];
}
