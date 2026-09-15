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

// A stored hint is retained until its clock-specific hold expires: Active
// for ircTypingActiveHoldMs (6s), Paused for ircTypingPausedHoldMs (30s).
// Done never stores a hint. pruneExpiredTyping drops a hint when this is
// false. Showing the indicator for the paused hold follows the IRCv3
// typing client-tag recommendation that clients SHOULD assume the sender
// is still typing until at least 30 seconds have elapsed since the last
// typing=paused notification:
// https://ircv3.net/specs/client-tags/typing
bool ircTypingHintRetained(const IrcTypingHint& hint, const QDateTime& now) noexcept;
// Paint while the hint is retained. Active and Paused both show; they
// differ only in hold duration.
bool ircTypingShowsIndicator(const IrcTypingHint& hint, const QDateTime& now) noexcept;
QDateTime ircTypingExpiresAt(const IrcTypingHint& hint);
std::optional<IrcTypingPhase> ircTypingPhaseFromTag(const QString& value);
QByteArray ircTypingTagmsg(const QString& target, IrcTypingPhase phase);
