#pragma once

#include "ircmessage.h"

#include <QDateTime>
#include <QString>
#include <QStringView>

enum class IrcLogSource { Server, Client, Local };
enum class IrcLogSeverity { Trace, Info, Alert };

class IrcStatusEntry
{
public:
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
    QString text() const;

private:
    explicit IrcStatusEntry(QString networkId,
                            QDateTime timestamp,
                            IrcLogSource source,
                            IrcLogSeverity severity,
                            QString label,
                            QString text);

    QString m_networkId;
    QDateTime m_timestamp;
    IrcLogSource m_source;
    IrcLogSeverity m_severity;
    QString m_label;
    QString m_text;
};
