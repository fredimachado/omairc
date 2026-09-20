#pragma once

#include <QtGlobal>

#ifdef Q_OS_LINUX
#include <QDBusConnection>
#include <QByteArray>
#include <QFile>
#include <QString>

// sessionBus() can block for minutes when DBUS_SESSION_BUS_ADDRESS names a
// unix:path that does not exist, or when the address is autolaunch:.
// Notifications and portal reads already no-op without a connected bus.
// An unset address still goes through sessionBus() so X11/autolaunch
// fallbacks keep working in a real session.
inline QDBusConnection linuxSessionBusIfPresent()
{
    const QByteArray address = qgetenv("DBUS_SESSION_BUS_ADDRESS");
    if (address.startsWith("autolaunch:"))
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
