#include "omaircpaths.h"

#include <QStandardPaths>

// Config and state follow $XDG_* when set, otherwise Qt Generic* locations.
// On macOS that is ~/Library/Preferences and ~/Library/Preferences/State,
// not Application Support. Homebrew zap must match these roots.

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
