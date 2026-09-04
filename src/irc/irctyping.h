#pragma once

#include <QByteArray>
#include <QDateTime>
#include <QString>

#include <optional>

enum class IrcTypingClock
{
    Active,
    Paused,
};

enum class IrcTypingPhase
{
    Active,
    Paused,
    Done,
};

inline constexpr int ircTypingActiveHoldMs = 6000;
inline constexpr int ircTypingPausedHoldMs = 30000;
inline constexpr int ircTypingSendIntervalMs = 3000;

struct IrcTypingHint
{
    IrcTypingClock clock = IrcTypingClock::Active;
    QDateTime receivedAt;
    QString displayNick;

    static std::optional<IrcTypingHint> stored(IrcTypingPhase phase,
                                               const QDateTime& receivedAt,
                                               const QString& displayNick);
};

bool ircIsTyping(const IrcTypingHint& hint, const QDateTime& now) noexcept;
QDateTime ircTypingExpiresAt(const IrcTypingHint& hint);
std::optional<IrcTypingPhase> ircTypingPhaseFromTag(const QString& value);
QByteArray ircTypingTagmsg(const QString& target, IrcTypingPhase phase);
