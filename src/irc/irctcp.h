#pragma once

#include <QString>

#include <optional>

struct IrcCtcpRequest
{
    QString command;
    QString argument;
};

std::optional<IrcCtcpRequest> parseCtcpRequest(const QString& body);
QString ctcpPayload(const IrcCtcpRequest& request);
