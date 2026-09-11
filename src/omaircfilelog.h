#pragma once

#include <QDateTime>
#include <QString>

#include <QtGlobal>

enum class DiagnosticSeverity : quint8 {
    Warning,
    Error,
};

struct DiagnosticRecord {
    QDateTime when;
    DiagnosticSeverity severity = DiagnosticSeverity::Warning;
    QString text;
};

class OmaircFileLog
{
public:
    OmaircFileLog();
    explicit OmaircFileLog(QString path);
    ~OmaircFileLog();

    OmaircFileLog(const OmaircFileLog &) = delete;
    OmaircFileLog &operator=(const OmaircFileLog &) = delete;

    static QString defaultPath();
    const QString &path() const;
    void write(const DiagnosticRecord &record);
    void install();

private:
    void uninstall();

    QString m_path;
};
