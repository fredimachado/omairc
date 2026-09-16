#pragma once

#include <QDateTime>
#include <QString>

#include <optional>

struct IrcCtcpRequest
{
    QString command;
    QString argument;
};

std::optional<IrcCtcpRequest> parseCtcpRequest(const QString& body);
QString ctcpPayload(const IrcCtcpRequest& request);
QString formatCtcpReplyText(const QString& command,
                            const QString& nick,
                            const QString& argument,
                            const QDateTime& now = QDateTime::currentDateTimeUtc());
