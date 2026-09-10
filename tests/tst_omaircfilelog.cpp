#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QTemporaryDir>
#include <QTest>

#include "omaircfilelog.h"

#include <memory>

class OmaircFileLogTest : public QObject
{
    Q_OBJECT

private slots:
    void writeProducesOneFormattedLine();
    void secondWriteAppends();
    void defaultPathUsesXdgStateHome();
    void constructWithoutWriteLeavesPathMissing();

private:
    std::unique_ptr<QTemporaryDir> m_dir;
};

void OmaircFileLogTest::writeProducesOneFormattedLine()
{
    m_dir = std::make_unique<QTemporaryDir>();
    QVERIFY(m_dir->isValid());
    const QString path = m_dir->filePath(QStringLiteral("omairc.log"));
    OmaircFileLog log(path);

    DiagnosticRecord record;
    record.when = QDateTime::fromString(
        QStringLiteral("2026-09-10T04:29:00.000Z"), Qt::ISODateWithMs);
    record.severity = DiagnosticSeverity::Error;
    record.text = QStringLiteral(
        "Could not load the Omairc interface; resource available: false");
    log.write(record);

    QFile file(path);
    QVERIFY(file.open(QIODevice::ReadOnly | QIODevice::Text));
    QCOMPARE(QString::fromUtf8(file.readAll()),
             QStringLiteral(
                 "2026-09-10T04:29:00.000Z error "
                 "Could not load the Omairc interface; resource available: false\n"));
}

void OmaircFileLogTest::secondWriteAppends()
{
    m_dir = std::make_unique<QTemporaryDir>();
    QVERIFY(m_dir->isValid());
    const QString path = m_dir->filePath(QStringLiteral("omairc.log"));
    OmaircFileLog log(path);

    DiagnosticRecord first;
    first.when = QDateTime::fromString(
        QStringLiteral("2026-09-10T04:29:00.000Z"), Qt::ISODateWithMs);
    first.severity = DiagnosticSeverity::Warning;
    first.text = QStringLiteral("first");
    log.write(first);

    DiagnosticRecord second;
    second.when = QDateTime::fromString(
        QStringLiteral("2026-09-10T04:29:01.000Z"), Qt::ISODateWithMs);
    second.severity = DiagnosticSeverity::Error;
    second.text = QStringLiteral("second");
    log.write(second);

    QFile file(path);
    QVERIFY(file.open(QIODevice::ReadOnly | QIODevice::Text));
    QCOMPARE(QString::fromUtf8(file.readAll()),
             QStringLiteral("2026-09-10T04:29:00.000Z warning first\n"
                            "2026-09-10T04:29:01.000Z error second\n"));
}

void OmaircFileLogTest::defaultPathUsesXdgStateHome()
{
    m_dir = std::make_unique<QTemporaryDir>();
    QVERIFY(m_dir->isValid());
    qputenv("XDG_STATE_HOME", m_dir->path().toUtf8());
    QCOMPARE(OmaircFileLog::defaultPath(),
             QDir(m_dir->path()).filePath(QStringLiteral("omairc/omairc.log")));
}

void OmaircFileLogTest::constructWithoutWriteLeavesPathMissing()
{
    m_dir = std::make_unique<QTemporaryDir>();
    QVERIFY(m_dir->isValid());
    const QString path = m_dir->filePath(QStringLiteral("omairc.log"));
    {
        OmaircFileLog log(path);
        QCOMPARE(log.path(), path);
    }
    QVERIFY(!QFileInfo::exists(path));
}

int runOmaircFileLogTests(int argc, char **argv)
{
    OmaircFileLogTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_omaircfilelog.moc"
