#include "omaircpaths.h"

#include <QStandardPaths>

QString omaircConfigRoot()
{
    const QString xdg = QString::fromUtf8(qgetenv("XDG_CONFIG_HOME"));
    if (!xdg.isEmpty())
        return xdg;
    return QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation);
}

QString omaircStateRoot()
{
    const QString xdg = QString::fromUtf8(qgetenv("XDG_STATE_HOME"));
    if (!xdg.isEmpty())
        return xdg;
    return QStandardPaths::writableLocation(QStandardPaths::GenericStateLocation);
}
