#pragma once

#ifdef Q_OS_LINUX
#include <QDBusConnection>
#include <QByteArray>
#include <QFile>
#include <QString>

// sessionBus() autolaunches when DBUS_SESSION_BUS_ADDRESS is empty, and can
// block for minutes on a unix:path that does not exist. Notifications and
// portal reads are no-ops without a bus; do not wait for one.
inline QDBusConnection linuxSessionBusIfPresent()
{
    const QByteArray address = qgetenv("DBUS_SESSION_BUS_ADDRESS");
    if (address.isEmpty())
        return QDBusConnection(QStringLiteral("omairc-no-session-bus"));

    const QByteArray prefix = QByteArrayLiteral("unix:path=");
    if (address.startsWith(prefix)) {
        QByteArray path = address.mid(prefix.size());
        const int comma = path.indexOf(',');
        if (comma >= 0)
            path.truncate(comma);
        if (path.isEmpty() || !QFile::exists(QString::fromLocal8Bit(path)))
            return QDBusConnection(QStringLiteral("omairc-no-session-bus"));
    }
    return QDBusConnection::sessionBus();
}
#endif
