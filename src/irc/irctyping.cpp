#include "irctyping.h"

#include <QLatin1Char>
#include <QLatin1String>

std::optional<IrcTypingHint> IrcTypingHint::stored(IrcTypingPhase phase,
                                                   const QDateTime& receivedAt,
                                                   const QString& displayNick)
{
    if (phase == IrcTypingPhase::Done)
        return std::nullopt;
    IrcTypingHint hint;
    hint.clock = phase == IrcTypingPhase::Paused
        ? IrcTypingClock::Paused
        : IrcTypingClock::Active;
    hint.receivedAt = receivedAt;
    hint.displayNick = displayNick;
    return hint;
}

QDateTime ircTypingExpiresAt(const IrcTypingHint& hint)
{
    const int holdMs = hint.clock == IrcTypingClock::Paused
        ? ircTypingPausedHoldMs
        : ircTypingActiveHoldMs;
    return hint.receivedAt.addMSecs(holdMs);
}

bool ircIsTyping(const IrcTypingHint& hint, const QDateTime& now) noexcept
{
    if (!hint.receivedAt.isValid() || !now.isValid())
        return false;
    return now < ircTypingExpiresAt(hint);
}

std::optional<IrcTypingPhase> ircTypingPhaseFromTag(const QString& value)
{
    if (value.compare(QLatin1String("active"), Qt::CaseInsensitive) == 0)
        return IrcTypingPhase::Active;
    if (value.compare(QLatin1String("paused"), Qt::CaseInsensitive) == 0)
        return IrcTypingPhase::Paused;
    if (value.compare(QLatin1String("done"), Qt::CaseInsensitive) == 0)
        return IrcTypingPhase::Done;
    return std::nullopt;
}

QByteArray ircTypingTagmsg(const QString& target, IrcTypingPhase phase)
{
    if (target.isEmpty())
        return {};
    for (const QChar character : target) {
        if (character == QLatin1Char(' ')
            || character == QLatin1Char('\r')
            || character == QLatin1Char('\n')) {
            return {};
        }
    }

    const char *value = "active";
    if (phase == IrcTypingPhase::Paused)
        value = "paused";
    else if (phase == IrcTypingPhase::Done)
        value = "done";

    QByteArray line("@+typing=");
    line += value;
    line += " TAGMSG ";
    line += target.toUtf8();
    line += "\r\n";
    return line;
}
