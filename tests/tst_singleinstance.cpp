#include <QCoreApplication>
#include <QProcess>
#include <QProcessEnvironment>
#include <QSignalSpy>
#include <QTest>

#include "singleinstance.h"

class SingleInstanceTest : public QObject
{
    Q_OBJECT

private slots:
    void primaryAcquireSucceeds();
    void secondaryNotifyEmitsActivation();
    void socketNameIsPlatformSafe();
};

void SingleInstanceTest::primaryAcquireSucceeds()
{
    SingleInstance primary;
    QVERIFY(primary.acquireOrNotify());
    QVERIFY(primary.isPrimary());
    QVERIFY(primary.acquireOrNotify());
}

void SingleInstanceTest::secondaryNotifyEmitsActivation()
{
    SingleInstance primary;
    QVERIFY(primary.acquireOrNotify());
    QVERIFY(primary.isPrimary());

    QSignalSpy spy(&primary, &SingleInstance::activationRequested);

    QProcessEnvironment env = QProcessEnvironment::systemEnvironment();
    env.insert(QStringLiteral("OMAIRC_TEST_SI_SECONDARY"), QStringLiteral("1"));

    QProcess child;
    child.setProcessEnvironment(env);
    child.start(QCoreApplication::applicationFilePath(), {});
    QVERIFY2(child.waitForStarted(5000), qPrintable(child.errorString()));
    QVERIFY2(child.waitForFinished(5000), qPrintable(child.errorString()));
    QCOMPARE(child.exitCode(), 0);

    QTRY_COMPARE(spy.count(), 1);
}

void SingleInstanceTest::socketNameIsPlatformSafe()
{
    const QString name = SingleInstance::socketPath();
#ifdef Q_OS_WIN
    QCOMPARE(name, QStringLiteral("omairc"));
#else
    QVERIFY(name.endsWith(QLatin1String("/omairc.sock")));
#endif
}

int runSingleInstanceTests(int argc, char **argv)
{
    SingleInstanceTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_singleinstance.moc"
