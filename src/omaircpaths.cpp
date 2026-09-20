#include "omaircpaths.h"

#include <QFileInfo>
#include <QStandardPaths>

#ifdef Q_OS_MACOS
#  include <QDir>
#  include <QFile>
#  include <limits.h>
#  include <mach-o/dyld.h>

static void prependEnvPath(const char *name, const QByteArray &dir)
{
    const QByteArray current = qgetenv(name);
    if (current.isEmpty() || current == dir)
        qputenv(name, dir);
    else if (!current.startsWith(dir + ":"))
        qputenv(name, dir + ":" + current);
}
#endif

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

QString omaircMacBundleContentsDir(const QString &executablePath)
{
    if (executablePath.isEmpty())
        return {};
    QString resolved = executablePath;
    const QFileInfo info(executablePath);
    if (info.exists()) {
        const QString canonical = info.canonicalFilePath();
        if (!canonical.isEmpty())
            resolved = canonical;
    }
    const auto needle = QLatin1String("/Contents/MacOS/");
    const int idx = resolved.lastIndexOf(needle);
    if (idx < 0)
        return {};
    return resolved.left(idx) + QLatin1String("/Contents");
}

QString omaircApplyMacBundleQtPaths()
{
#ifdef Q_OS_MACOS
    uint32_t size = PATH_MAX;
    QByteArray buf(static_cast<int>(size), Qt::Uninitialized);
    if (_NSGetExecutablePath(buf.data(), &size) != 0) {
        buf.resize(static_cast<int>(size));
        if (_NSGetExecutablePath(buf.data(), &size) != 0)
            return {};
    }
    const QString contents = omaircMacBundleContentsDir(
        QString::fromLocal8Bit(buf.constData()));
    if (contents.isEmpty())
        return {};
    const QString plugins = contents + QLatin1String("/PlugIns");
    const QString qml = contents + QLatin1String("/Resources/qml");
    if (QDir(plugins).exists())
        prependEnvPath("QT_PLUGIN_PATH", QFile::encodeName(plugins));
    if (QDir(qml).exists()) {
        prependEnvPath("QML2_IMPORT_PATH", QFile::encodeName(qml));
        prependEnvPath("QML_IMPORT_PATH", QFile::encodeName(qml));
    }
    return contents;
#else
    return {};
#endif
}
