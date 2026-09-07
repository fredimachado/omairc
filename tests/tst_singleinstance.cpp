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

int runSingleInstanceTests(int argc, char **argv)
{
    SingleInstanceTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_singleinstance.moc"
