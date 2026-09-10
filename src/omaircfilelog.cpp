#include "omaircfilelog.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QMutex>
#include <QMutexLocker>
#include <QStandardPaths>

namespace {

OmaircFileLog *g_installed = nullptr;
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

    const bool created = !QFileInfo::exists(path);
    QFile file(path);
    if (!file.open(QIODevice::WriteOnly | QIODevice::Append))
        return false;
    if (created)
        file.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner);
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
    if (g_installed) {
        DiagnosticRecord record;
        if (recordFromQt(type, message, &record))
            g_installed->write(record);
    }
    if (g_previous)
        g_previous(type, context, message);
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
    if (g_installed == this)
        return;
    if (g_installed)
        return;
    g_previous = qInstallMessageHandler(qtHandler);
    g_installed = this;
}

void OmaircFileLog::uninstall()
{
    if (g_installed != this)
        return;
    qInstallMessageHandler(g_previous);
    g_installed = nullptr;
    g_previous = nullptr;
}
