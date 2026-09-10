#pragma once

#include "ircmessage.h"

#include <QDateTime>
#include <QString>

#include <optional>

enum class IrcLogSource { Server, Client, Local };
enum class IrcLogSeverity { Trace, Info, Alert };

class IrcWhoisLine final
{
public:
    enum class Progress { Detail, Terminal, Failed };

    const QString& nick() const noexcept;
    const QString& text() const noexcept;
    Progress progress() const noexcept;
    bool terminal() const noexcept;

private:
    friend class IrcStatusEntry;
    IrcWhoisLine(QString nick, QString text, Progress progress);

    QString m_nick;
    QString m_text;
    Progress m_progress = Progress::Detail;
};

class IrcStatusEntry
{
public:
    static IrcStatusEntry incoming(const QString& networkId, const IrcMessage& message);
    static IrcStatusEntry outgoing(const QString& networkId, const QByteArray& line);
    static IrcStatusEntry lifecycle(const QString& networkId,
                                    IrcLogSeverity severity,
                                    const QString& label,
                                    const QString& text);
    static IrcStatusEntry outcome(const QString& networkId, const QString& text);

    IrcStatusEntry() = delete;

    QString networkId() const;
    QDateTime timestamp() const;
    IrcLogSource source() const;
    IrcLogSeverity severity() const;
    QString label() const;
    QString text() const;
    const IrcWhoisLine *whoisLine() const noexcept;

private:
    explicit IrcStatusEntry(QString networkId,
                            QDateTime timestamp,
                            IrcLogSource source,
                            IrcLogSeverity severity,
                            QString label,
                            QString text,
                            std::optional<IrcWhoisLine> whoisLine = {});

    QString m_networkId;
    QDateTime m_timestamp;
    IrcLogSource m_source;
    IrcLogSeverity m_severity;
    QString m_label;
    QString m_text;
    std::optional<IrcWhoisLine> m_whoisLine;
};
