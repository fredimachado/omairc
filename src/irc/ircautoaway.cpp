#include "ircautoaway.h"

#include <QtGlobal>

namespace
{
QString firstToken(const QString& argument)
{
    const int space = argument.indexOf(QLatin1Char(' '));
    return space < 0 ? argument : argument.left(space);
}

QString restAfterFirstToken(const QString& argument)
{
    const int space = argument.indexOf(QLatin1Char(' '));
    return space < 0 ? QString() : argument.mid(space + 1).trimmed();
}

QString formatEnabled(const IrcAutoawayConfig& config, bool includeDefaultWithOneShot)
{
    if (!config.oneShotReason.isEmpty()) {
        QString text = QStringLiteral("Auto-away in %1: %2 (this time only)")
                           .arg(ircFormatAutoawayDuration(config.timeoutSeconds),
                                config.oneShotReason);
        if (includeDefaultWithOneShot && !config.defaultReason.isEmpty())
            text += QStringLiteral(", reason: %1").arg(config.defaultReason);
        return text;
    }
    QString text = QStringLiteral("Auto-away %1")
                       .arg(ircFormatAutoawayDuration(config.timeoutSeconds));
    if (!config.defaultReason.isEmpty())
        text += QStringLiteral(", reason: %1").arg(config.defaultReason);
    return text;
}

QString formatDisabled(const IrcAutoawayConfig& config, bool includeTimeout)
{
    QString text = QStringLiteral("Auto-away off");
    if (includeTimeout && config.timeoutSeconds > 0)
        text += QStringLiteral(", %1").arg(ircFormatAutoawayDuration(config.timeoutSeconds));
    if (!config.oneShotReason.isEmpty())
        text += QStringLiteral(": %1 (this time only)").arg(config.oneShotReason);
    if (!config.defaultReason.isEmpty())
        text += QStringLiteral(", reason: %1").arg(config.defaultReason);
    return text;
}
}

std::optional<int> ircParseAutoawayDuration(const QString& token)
{
    if (token.isEmpty())
        return std::nullopt;

    int i = 0;
    const int n = token.size();
    while (i < n && token.at(i).isDigit())
        ++i;
    if (i == 0)
        return std::nullopt;
    if (i < n && token.at(i) == QLatin1Char('.')) {
        ++i;
        const int fractionStart = i;
        while (i < n && token.at(i).isDigit())
            ++i;
        if (i == fractionStart)
            return std::nullopt;
    }

    const QString number = token.left(i);
    QChar suffix;
    if (i < n) {
        if (i + 1 != n)
            return std::nullopt;
        suffix = token.at(i).toLower();
        if (suffix != QLatin1Char('s') && suffix != QLatin1Char('m')
            && suffix != QLatin1Char('h')) {
            return std::nullopt;
        }
    }

    bool ok = false;
    const double value = number.toDouble(&ok);
    if (!ok || value < 0)
        return std::nullopt;

    double seconds = value;
    if (suffix.isNull() || suffix == QLatin1Char('m'))
        seconds = value * 60.0;
    else if (suffix == QLatin1Char('h'))
        seconds = value * 3600.0;

    if (seconds > double(ircAutoawayMaxTimeoutSeconds))
        return std::nullopt;
    const int rounded = qRound(seconds);
    if (rounded < 0 || rounded > ircAutoawayMaxTimeoutSeconds)
        return std::nullopt;
    return rounded;
}

IrcAutoawayRequest ircParseAutoawayArgument(const QString& argument)
{
    IrcAutoawayRequest request;
    const QString trimmed = argument.trimmed();
    if (trimmed.isEmpty()) {
        request.kind = IrcAutoawayKind::Query;
        return request;
    }

    const QString token = firstToken(trimmed);
    const QString rest = restAfterFirstToken(trimmed);
    const QString folded = token.toLower();

    if (folded == QLatin1String("off") || folded == QLatin1String("0")) {
        if (!rest.isEmpty())
            return request;
        request.kind = IrcAutoawayKind::Disable;
        return request;
    }
    if (folded == QLatin1String("on")) {
        if (!rest.isEmpty())
            return request;
        request.kind = IrcAutoawayKind::EnableOn;
        return request;
    }
    if (folded == QLatin1String("reason")) {
        if (rest.isEmpty()) {
            request.kind = IrcAutoawayKind::ClearDefaultReason;
            return request;
        }
        request.kind = IrcAutoawayKind::SetDefaultReason;
        request.text = rest;
        return request;
    }

    const std::optional<int> seconds = ircParseAutoawayDuration(token);
    if (!seconds || *seconds < ircAutoawayMinTimeoutSeconds)
        return request;
    request.kind = IrcAutoawayKind::SetTimeout;
    request.timeoutSeconds = *seconds;
    request.text = rest;
    return request;
}

QString ircFormatAutoawayDuration(int seconds)
{
    if (seconds >= 3600 && seconds % 3600 == 0) {
        const int hours = seconds / 3600;
        return hours == 1 ? QStringLiteral("1 hour")
                          : QStringLiteral("%1 hours").arg(hours);
    }
    if (seconds >= 60 && seconds % 60 == 0) {
        const int minutes = seconds / 60;
        return minutes == 1 ? QStringLiteral("1 minute")
                            : QStringLiteral("%1 minutes").arg(minutes);
    }
    return seconds == 1 ? QStringLiteral("1 second")
                        : QStringLiteral("%1 seconds").arg(seconds);
}

QString ircFormatAutoawayQuery(const IrcAutoawayConfig& config)
{
    if (!config.enabled)
        return formatDisabled(config, true);
    return formatEnabled(config, true);
}

QString ircFormatAutoawayConfirmation(const IrcAutoawayConfig& config)
{
    if (!config.enabled)
        return formatDisabled(config, false);
    return formatEnabled(config, false);
}

int ircAutoawayGraceSeconds(int timeoutSeconds)
{
    if (timeoutSeconds <= 0)
        return 0;
    return qBound(5, timeoutSeconds / 20, 10);
}
