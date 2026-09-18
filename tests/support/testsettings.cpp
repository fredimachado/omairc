#include "testsettings.h"

#include <QSettings>

void TestSettings::isolate(const QString &configRoot)
{
#if defined(Q_OS_WIN) || defined(Q_OS_MACOS)
    QSettings::setDefaultFormat(QSettings::IniFormat);
    QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, configRoot);
#endif
#ifndef Q_OS_WIN
    qputenv("XDG_CONFIG_HOME", configRoot.toUtf8());
#endif
#if !defined(Q_OS_WIN) && !defined(Q_OS_MACOS)
    QSettings::setPath(QSettings::NativeFormat, QSettings::UserScope, configRoot);
#endif
}
