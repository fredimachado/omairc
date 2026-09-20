#pragma once

#include <QString>

#include <limits>
#include <optional>

constexpr int ircAutoawayMinTimeoutSeconds = 30;
constexpr int ircAutoawayMaxTimeoutSeconds =
    std::numeric_limits<int>::max() / 1000;

enum class IrcAutoawayKind {
    Query,
    Disable,
    EnableOn,
    SetTimeout,
    SetDefaultReason,
    ClearDefaultReason,
    Usage,
};

struct IrcAutoawayRequest
{
    IrcAutoawayKind kind = IrcAutoawayKind::Usage;
    int timeoutSeconds = 0;
    QString text;
};

struct IrcAutoawayConfig
{
    bool enabled = false;
    int timeoutSeconds = 0;
    QString defaultReason;
    QString oneShotReason;
};

std::optional<int> ircParseAutoawayDuration(const QString& token);
IrcAutoawayRequest ircParseAutoawayArgument(const QString& argument);
QString ircFormatAutoawayDuration(int seconds);
QString ircFormatAutoawayQuery(const IrcAutoawayConfig& config);
QString ircFormatAutoawayConfirmation(const IrcAutoawayConfig& config);
int ircAutoawayGraceSeconds(int timeoutSeconds);
