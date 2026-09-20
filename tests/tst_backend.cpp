#include <QCoreApplication>
#include <QTest>

#include "backend.h"

#ifdef Q_OS_LINUX
#include <QDBusAbstractAdaptor>
#include <QDBusConnection>
#include <QDBusMessage>
#include <QElapsedTimer>
#include <QProcess>
#include <QSignalSpy>
#include <QStringList>
#include <QVariantMap>
#include <QVector>

namespace {
void setMissingSessionBus()
{
    qputenv("DBUS_SESSION_BUS_ADDRESS", "unix:path=/tmp/omairc-no-such-session-bus");
}

void drainSessionBus()
{
    QDBusMessage ping = QDBusMessage::createMethodCall(
        QStringLiteral("org.freedesktop.DBus"),
        QStringLiteral("/org/freedesktop/DBus"),
        QStringLiteral("org.freedesktop.DBus"),
        QStringLiteral("GetNameOwner"));
    ping << QStringLiteral("org.freedesktop.Notifications");
    QDBusConnection::sessionBus().call(ping, QDBus::Block, 2000);
    QCoreApplication::sendPostedEvents();
    QCoreApplication::processEvents();
}
}

class FakeNotifications : public QObject
{
    Q_OBJECT

public:
    struct Call {
        QString appName;
        uint replacesId = 0;
        QString appIcon;
        QString summary;
        QString body;
        QStringList actions;
        QVariantMap hints;
        int expireTimeout = 0;
    };

    explicit FakeNotifications(QObject *parent = nullptr) : QObject(parent) {}

    QVector<Call> calls;
    uint nextId = 1;

    uint notify(const QString &appName, uint replacesId, const QString &appIcon,
                const QString &summary, const QString &body,
                const QStringList &actions, const QVariantMap &hints,
                int expireTimeout)
    {
        calls.push_back({appName, replacesId, appIcon, summary, body, actions,
                         hints, expireTimeout});
        return nextId++;
    }
};

class NotificationsAdaptor : public QDBusAbstractAdaptor
{
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "org.freedesktop.Notifications")

public:
    explicit NotificationsAdaptor(FakeNotifications *parent)
        : QDBusAbstractAdaptor(parent)
    {
    }

public slots:
    uint Notify(const QString &appName, uint replacesId, const QString &appIcon,
                const QString &summary, const QString &body,
                const QStringList &actions, const QVariantMap &hints,
                int expireTimeout)
    {
        return static_cast<FakeNotifications *>(parent())->notify(
            appName, replacesId, appIcon, summary, body, actions, hints,
            expireTimeout);
    }
};

class BackendNotifyTest : public QObject
{
    Q_OBJECT

private slots:
    void initTestCase();
    void cleanupTestCase();
    void notifyRegistersDefaultActionAndReplacesSameConversation();
    void actionInvokedEmitsNetworkTargetMsgid();
    void notifyDesktopWithoutSessionBusDoesNotCrash();
    void notifyDesktopAfterDisconnectDoesNotCrash();

private:
    bool ensurePrivateBus();
    bool registerNotifications();
    void stopPrivateBus();

    QProcess m_daemon;
    FakeNotifications m_notifications;
    QByteArray m_previousBus;
    QString m_sessionConnectionName;
    bool m_hadBus = false;
    bool m_busReady = false;
};

void BackendNotifyTest::initTestCase()
{
    m_hadBus = qEnvironmentVariableIsSet("DBUS_SESSION_BUS_ADDRESS");
    m_previousBus = qgetenv("DBUS_SESSION_BUS_ADDRESS");
}

void BackendNotifyTest::cleanupTestCase()
{
    stopPrivateBus();
    if (!m_sessionConnectionName.isEmpty()) {
        QDBusConnection::disconnectFromBus(m_sessionConnectionName);
        m_sessionConnectionName.clear();
    }
    if (m_hadBus)
        qputenv("DBUS_SESSION_BUS_ADDRESS", m_previousBus);
    else
        qunsetenv("DBUS_SESSION_BUS_ADDRESS");
}

bool BackendNotifyTest::ensurePrivateBus()
{
    if (m_busReady)
        return true;

    m_daemon.setProcessChannelMode(QProcess::SeparateChannels);
    m_daemon.start(QStringLiteral("dbus-daemon"), {
        QStringLiteral("--session"),
        QStringLiteral("--nofork"),
        QStringLiteral("--nopidfile"),
        QStringLiteral("--print-address"),
    });
    if (!m_daemon.waitForStarted(5000))
        return false;
    if (!m_daemon.waitForReadyRead(5000))
        return false;
    QByteArray address = m_daemon.readAllStandardOutput().trimmed();
    const int newline = address.indexOf('\n');
    if (newline >= 0)
        address = address.left(newline).trimmed();
    if (address.isEmpty())
        return false;
    qputenv("DBUS_SESSION_BUS_ADDRESS", address);
    if (!registerNotifications())
        return false;
    m_busReady = true;
    return true;
}

bool BackendNotifyTest::registerNotifications()
{
    QDBusConnection bus = QDBusConnection::sessionBus();
    if (!bus.isConnected())
        return false;
    m_sessionConnectionName = bus.name();
    new NotificationsAdaptor(&m_notifications);
    if (!bus.registerObject(QStringLiteral("/org/freedesktop/Notifications"),
                            &m_notifications,
                            QDBusConnection::ExportAdaptors)) {
        return false;
    }
    return bus.registerService(QStringLiteral("org.freedesktop.Notifications"));
}

void BackendNotifyTest::stopPrivateBus()
{
    if (m_daemon.state() != QProcess::NotRunning) {
        m_daemon.terminate();
        if (!m_daemon.waitForFinished(2000))
            m_daemon.kill();
        m_daemon.waitForFinished(2000);
    }
    m_busReady = false;
}

void BackendNotifyTest::notifyRegistersDefaultActionAndReplacesSameConversation()
{
    if (!ensurePrivateBus())
        QSKIP("dbus-daemon --session is unavailable");

    m_notifications.calls.clear();
    m_notifications.nextId = 1;

    Backend backend;
    backend.notifyDesktop(QStringLiteral("alice"), QStringLiteral("fred: ping"),
                          QStringLiteral("omarchy"), QStringLiteral("#omarchy"),
                          QStringLiteral("mid-1"));
    QTRY_COMPARE(m_notifications.calls.size(), 1);
    drainSessionBus();
    const FakeNotifications::Call first = m_notifications.calls.at(0);
    QCOMPARE(first.appName, QStringLiteral("Omairc"));
    QCOMPARE(first.replacesId, uint(0));
    QCOMPARE(first.appIcon, QStringLiteral("omairc"));
    QCOMPARE(first.summary, QStringLiteral("alice"));
    QCOMPARE(first.body, QStringLiteral("fred: ping"));
    QCOMPARE(first.actions, (QStringList{QStringLiteral("default"),
                                         QStringLiteral("Open")}));
    QVERIFY(first.hints.isEmpty());
    QCOMPARE(first.expireTimeout, -1);

    backend.notifyDesktop(QStringLiteral("alice"), QStringLiteral("fred: again"),
                          QStringLiteral("omarchy"), QStringLiteral("#omarchy"),
                          QStringLiteral("mid-2"));
    QTRY_COMPARE(m_notifications.calls.size(), 2);
    QCOMPARE(m_notifications.calls.at(1).replacesId, uint(1));
    QCOMPARE(m_notifications.calls.at(1).body, QStringLiteral("fred: again"));
}

void BackendNotifyTest::actionInvokedEmitsNetworkTargetMsgid()
{
    if (!ensurePrivateBus())
        QSKIP("dbus-daemon --session is unavailable");

    m_notifications.calls.clear();
    m_notifications.nextId = 10;
    Backend backend;
    backend.notifyDesktop(QStringLiteral("alice"), QStringLiteral("secret"),
                          QStringLiteral("omarchy"), QStringLiteral("anna"),
                          QStringLiteral("dm-7"));
    QTRY_COMPARE(m_notifications.calls.size(), 1);
    drainSessionBus();
    QCOMPARE(m_notifications.calls.at(0).replacesId, uint(0));

    QSignalSpy spy(&backend, &Backend::notificationActivated);
    QDBusMessage signal = QDBusMessage::createSignal(
        QStringLiteral("/org/freedesktop/Notifications"),
        QStringLiteral("org.freedesktop.Notifications"),
        QStringLiteral("ActionInvoked"));
    signal << uint(10) << QStringLiteral("default");
    QVERIFY(QDBusConnection::sessionBus().send(signal));
    QTRY_COMPARE(spy.count(), 1);
    QCOMPARE(spy.at(0).at(0).toString(), QStringLiteral("omarchy"));
    QCOMPARE(spy.at(0).at(1).toString(), QStringLiteral("anna"));
    QCOMPARE(spy.at(0).at(2).toString(), QStringLiteral("dm-7"));
}

void BackendNotifyTest::notifyDesktopWithoutSessionBusDoesNotCrash()
{
    setMissingSessionBus();
    QElapsedTimer timer;
    timer.start();
    Backend backend;
    backend.notifyDesktop(QStringLiteral("alice"), QStringLiteral("hello"),
                          QStringLiteral("omarchy"), QStringLiteral("#omarchy"),
                          QStringLiteral("mid-1"));
    QVERIFY(timer.elapsed() < 2000);
}

void BackendNotifyTest::notifyDesktopAfterDisconnectDoesNotCrash()
{
    stopPrivateBus();
    setMissingSessionBus();
    QElapsedTimer timer;
    timer.start();
    Backend backend;
    backend.notifyDesktop(QStringLiteral("alice"), QStringLiteral("hello"));
    QVERIFY(timer.elapsed() < 2000);
}
#endif

#if !defined(Q_OS_LINUX) && !defined(Q_OS_MACOS)
class BackendNotifySmokeTest : public QObject
{
    Q_OBJECT

private slots:
    void notifyDesktopDoesNotCrash();
};

void BackendNotifySmokeTest::notifyDesktopDoesNotCrash()
{
    Backend backend;
    backend.notifyDesktop(QStringLiteral("alice"), QStringLiteral("hello"),
                          QStringLiteral("omarchy"), QStringLiteral("#omarchy"),
                          QStringLiteral("mid-1"));
    backend.notifyDesktop(QStringLiteral("bob"), QStringLiteral("ping"));
}
#endif

int runBackendTests(int argc, char **argv)
{
#ifdef Q_OS_LINUX
    BackendNotifyTest test;
    return QTest::qExec(&test, argc, argv);
#elif defined(Q_OS_MACOS)
    return 0;
#else
    BackendNotifySmokeTest test;
    return QTest::qExec(&test, argc, argv);
#endif
}

#include "tst_backend.moc"
