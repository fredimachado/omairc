#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QTest>

#include "omaircpaths.h"

class OmaircPathsTest : public QObject
{
    Q_OBJECT

private slots:
    void xdgOverridesPlatformRoots();
    void unsetXdgUsesGenericLocations();
    void macOsRootsMatchHomebrewZap();
    void macBundleContentsDirFollowsSymlink();
};

void OmaircPathsTest::xdgOverridesPlatformRoots()
{
    const QByteArray previousConfig = qgetenv("XDG_CONFIG_HOME");
    const QByteArray previousState = qgetenv("XDG_STATE_HOME");
    qputenv("XDG_CONFIG_HOME", "/tmp/omairc-xdg-config");
    qputenv("XDG_STATE_HOME", "/tmp/omairc-xdg-state");
    QCOMPARE(omaircConfigRoot(), QStringLiteral("/tmp/omairc-xdg-config"));
    QCOMPARE(omaircStateRoot(), QStringLiteral("/tmp/omairc-xdg-state"));
    if (previousConfig.isEmpty())
        qunsetenv("XDG_CONFIG_HOME");
    else
        qputenv("XDG_CONFIG_HOME", previousConfig);
    if (previousState.isEmpty())
        qunsetenv("XDG_STATE_HOME");
    else
        qputenv("XDG_STATE_HOME", previousState);
}

void OmaircPathsTest::unsetXdgUsesGenericLocations()
{
    const QByteArray previousConfig = qgetenv("XDG_CONFIG_HOME");
    const QByteArray previousState = qgetenv("XDG_STATE_HOME");
    qunsetenv("XDG_CONFIG_HOME");
    qunsetenv("XDG_STATE_HOME");
    QCOMPARE(omaircConfigRoot(),
             QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation));
    QCOMPARE(omaircStateRoot(),
             QStandardPaths::writableLocation(QStandardPaths::GenericStateLocation));
    if (previousConfig.isEmpty())
        qunsetenv("XDG_CONFIG_HOME");
    else
        qputenv("XDG_CONFIG_HOME", previousConfig);
    if (previousState.isEmpty())
        qunsetenv("XDG_STATE_HOME");
    else
        qputenv("XDG_STATE_HOME", previousState);
}

void OmaircPathsTest::macOsRootsMatchHomebrewZap()
{
#ifndef Q_OS_MACOS
    QSKIP("Homebrew zap roots are macOS QStandardPaths");
#else
    const QByteArray previousConfig = qgetenv("XDG_CONFIG_HOME");
    const QByteArray previousState = qgetenv("XDG_STATE_HOME");
    qunsetenv("XDG_CONFIG_HOME");
    qunsetenv("XDG_STATE_HOME");
    QCOMPARE(omaircConfigRoot(),
             QDir::homePath() + QLatin1String("/Library/Preferences"));
    QCOMPARE(omaircStateRoot(),
             QDir::homePath() + QLatin1String("/Library/Preferences/State"));
    QVERIFY(!omaircStateRoot().contains(QLatin1String("Application Support")));
    if (previousConfig.isEmpty())
        qunsetenv("XDG_CONFIG_HOME");
    else
        qputenv("XDG_CONFIG_HOME", previousConfig);
    if (previousState.isEmpty())
        qunsetenv("XDG_STATE_HOME");
    else
        qputenv("XDG_STATE_HOME", previousState);
#endif
}

void OmaircPathsTest::macBundleContentsDirFollowsSymlink()
{
    QCOMPARE(omaircMacBundleContentsDir(
                 QStringLiteral("/Applications/Foo.app/Contents/MacOS/foo")),
             QStringLiteral("/Applications/Foo.app/Contents"));
    QCOMPARE(omaircMacBundleContentsDir(QStringLiteral("/tmp/not-an-app/omairc")),
             QString());
    QCOMPARE(omaircMacBundleContentsDir(QString()), QString());

#ifndef Q_OS_UNIX
    return;
#else
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString macOS = dir.filePath(QStringLiteral("omairc.app/Contents/MacOS"));
    QVERIFY(QDir().mkpath(macOS));
    const QString exe = macOS + QLatin1String("/omairc");
    QFile file(exe);
    QVERIFY(file.open(QIODevice::WriteOnly));
    QVERIFY(file.write("x") > 0);
    file.close();
    QVERIFY(file.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner
                                | QFileDevice::ExeOwner | QFileDevice::ReadUser
                                | QFileDevice::ExeUser));
    const QString link = dir.filePath(QStringLiteral("omairc"));
    QVERIFY(QFile::link(exe, link));
    const QString contents = dir.filePath(QStringLiteral("omairc.app/Contents"));
    QCOMPARE(omaircMacBundleContentsDir(link), QFileInfo(contents).canonicalFilePath());
    QCOMPARE(omaircMacBundleContentsDir(exe), QFileInfo(contents).canonicalFilePath());
#endif
}

int runOmaircPathsTests(int argc, char **argv)
{
    OmaircPathsTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_omaircpaths.moc"
