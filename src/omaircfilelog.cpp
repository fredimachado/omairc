#include "omaircfilelog.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QMutex>
#include <QMutexLocker>
#include <QReadWriteLock>
#include <QStandardPaths>

namespace {

QReadWriteLock g_handlerLock;
OmaircFileLog *g_occupant = nullptr;
QString g_path;
QtMessageHandler g_previous = nullptr;
QMutex g_writeMutex;
thread_local bool t_inWrite = false;

QString flattenText(QString text)
{
    text.replace(QLatin1Char('\n'), QLatin1Char(' '));
    text.replace(QLatin1Char('\r'), QLatin1Char(' '));
    return text;
}

QByteArray formatLine(const DiagnosticRecord &record)
{
    const QString when = record.when.toUTC().toString(Qt::ISODateWithMs);
    const QString severity = record.severity == DiagnosticSeverity::Warning
        ? QStringLiteral("warning")
        : QStringLiteral("error");
    return (when + QLatin1Char(' ') + severity + QLatin1Char(' ')
            + flattenText(record.text) + QLatin1Char('\n'))
        .toUtf8();
}

bool appendLine(const QString &path, const QByteArray &line)
{
    const QString dir = QFileInfo(path).absolutePath();
    if (!QDir().mkpath(dir))
        return false;

    QFile file(path);
    if (!file.open(QIODevice::WriteOnly | QIODevice::Append))
        return false;
    if (!file.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner))
        return false;
    return file.write(line) == line.size();
}

bool recordFromQt(QtMsgType type, const QString &message, DiagnosticRecord *out)
{
    switch (type) {
    case QtWarningMsg:
        out->severity = DiagnosticSeverity::Warning;
        break;
    case QtCriticalMsg:
    case QtFatalMsg:
        out->severity = DiagnosticSeverity::Error;
        break;
    case QtDebugMsg:
    case QtInfoMsg:
        return false;
    }
    out->when = QDateTime::currentDateTimeUtc();
    out->text = message;
    return true;
}

void qtHandler(QtMsgType type, const QMessageLogContext &context, const QString &message)
{
    QString path;
    QtMessageHandler previous = nullptr;
    {
        QReadLocker locker(&g_handlerLock);
        path = g_path;
        previous = g_previous;
    }
    if (!path.isEmpty() && !t_inWrite) {
        DiagnosticRecord record;
        if (recordFromQt(type, message, &record)) {
            t_inWrite = true;
            {
                QMutexLocker locker(&g_writeMutex);
                appendLine(path, formatLine(record));
            }
            t_inWrite = false;
        }
    }
    if (previous)
        previous(type, context, message);
}

}

OmaircFileLog::OmaircFileLog()
    : OmaircFileLog(defaultPath())
{
}

OmaircFileLog::OmaircFileLog(QString path)
    : m_path(std::move(path))
{
}

OmaircFileLog::~OmaircFileLog()
{
    uninstall();
}

QString OmaircFileLog::defaultPath()
{
    QString root = QStandardPaths::writableLocation(QStandardPaths::GenericStateLocation);
    if (root.isEmpty())
        root = QStandardPaths::writableLocation(QStandardPaths::TempLocation);
    return QDir(root).filePath(QStringLiteral("omairc/omairc.log"));
}

const QString &OmaircFileLog::path() const
{
    return m_path;
}

void OmaircFileLog::write(const DiagnosticRecord &record)
{
    if (t_inWrite)
        return;

    t_inWrite = true;
    {
        QMutexLocker locker(&g_writeMutex);
        appendLine(m_path, formatLine(record));
    }
    t_inWrite = false;
}

void OmaircFileLog::install()
{
    QWriteLocker locker(&g_handlerLock);
    if (g_occupant)
        return;
    g_path = m_path;
    g_previous = qInstallMessageHandler(qtHandler);
    g_occupant = this;
}

void OmaircFileLog::uninstall()
{
    QtMessageHandler previous = nullptr;
    {
        QWriteLocker locker(&g_handlerLock);
        if (g_occupant != this)
            return;
        previous = g_previous;
        g_occupant = nullptr;
        g_previous = nullptr;
        g_path.clear();
    }
    qInstallMessageHandler(previous);
}
