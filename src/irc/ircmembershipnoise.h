#pragma once

#include <QString>

#include <optional>

// How join, part, quit, and nick lines are shown. Folded is the ordinary case.
enum class IrcMembershipNoise { Folded, Every, Hidden };

inline QString ircMembershipNoiseToken(IrcMembershipNoise noise)
{
    switch (noise) {
    case IrcMembershipNoise::Every:
        return QStringLiteral("every");
    case IrcMembershipNoise::Hidden:
        return QStringLiteral("hidden");
    case IrcMembershipNoise::Folded:
        break;
    }
    return QStringLiteral("folded");
}

inline std::optional<IrcMembershipNoise> ircMembershipNoiseFromToken(const QString& token)
{
    const QString folded = token.trimmed().toLower();
    if (folded == QLatin1String("folded"))
        return IrcMembershipNoise::Folded;
    if (folded == QLatin1String("every"))
        return IrcMembershipNoise::Every;
    if (folded == QLatin1String("hidden"))
        return IrcMembershipNoise::Hidden;
    return std::nullopt;
}

inline QString ircShownAccount(bool sameAsNick, const QString& account)
{
    if (account.isEmpty() || account == QLatin1String("*") || sameAsNick)
        return {};
    return account;
}

inline QString ircJoinLine(const QString& nick, const QString& shownAccount)
{
    if (shownAccount.isEmpty())
        return nick + QStringLiteral(" joined");
    return nick + QStringLiteral(" (") + shownAccount + QStringLiteral(") joined");
}

inline QString ircPartLine(const QString& nick)
{
    return nick + QStringLiteral(" left");
}

inline QString ircQuitLine(const QString& nick)
{
    return nick + QStringLiteral(" quit");
}

inline QString ircNickLine(const QString& oldNick, const QString& newNick)
{
    return oldNick + QStringLiteral(" is now ") + newNick;
}
