#pragma once

#include "ircmessage.h"

#include <QByteArray>
#include <QDateTime>
#include <QList>
#include <QString>
#include <QStringView>

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

class IrcCtcpReplyLine final
{
public:
    const QString& nick() const noexcept;
    const QString& command() const noexcept;

private:
    friend class IrcStatusEntry;
    IrcCtcpReplyLine(QString nick, QString command);

    QString m_nick;
    QString m_command;
};

class IrcStatusEntry
{
public:
    static QList<IrcStatusEntry> incomingAll(const QString& networkId,
                                            const IrcMessage& message,
                                            QStringView channelTypes = {});
    static IrcStatusEntry incoming(const QString& networkId,
                                  const IrcMessage& message,
                                  QStringView channelTypes = {});
    static IrcStatusEntry outgoing(const QString& networkId,
                                  const QByteArray& line,
                                  QStringView channelTypes = {});
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
    QString requestLabel() const;
    void setRequestLabel(QString requestLabel);
    QString text() const;
    const IrcWhoisLine *whoisLine() const noexcept;
    const IrcCtcpReplyLine *ctcpReply() const noexcept;

private:
    static IrcStatusEntry buildDefaultIncoming(const QString& networkId,
                                               const IrcMessage& message,
                                               QStringView channelTypes);
    explicit IrcStatusEntry(QString networkId,
                            QDateTime timestamp,
                            IrcLogSource source,
                            IrcLogSeverity severity,
                            QString label,
                            QString text,
                            std::optional<IrcWhoisLine> whoisLine = {},
                            std::optional<IrcCtcpReplyLine> ctcpReply = {});

    QString m_networkId;
    QDateTime m_timestamp;
    IrcLogSource m_source;
    IrcLogSeverity m_severity;
    QString m_label;
    QString m_requestLabel;
    QString m_text;
    std::optional<IrcWhoisLine> m_whoisLine;
    std::optional<IrcCtcpReplyLine> m_ctcpReply;
};

bool ircStatusKeepsIncoming(const IrcMessage& message,
                            const QString& currentNick,
                            QStringView channelTypes);
bool ircStatusKeepsOutgoing(const QByteArray& line);
