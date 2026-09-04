#include "irctypingpublisher.h"

bool IrcTypingPublisher::shouldSend(const QString& target,
                                    IrcTypingPhase phase,
                                    const QDateTime& now) const
{
    if (phase == IrcTypingPhase::Paused)
        return false;

    const auto found = m_targets.constFind(target);
    if (found == m_targets.cend())
        return phase == IrcTypingPhase::Active;

    const TargetClock& clock = found.value();
    if (phase == IrcTypingPhase::Done && clock.suppressDone)
        return false;
    if (phase == IrcTypingPhase::Done && clock.lastPhase != IrcTypingPhase::Active)
        return false;
    if (clock.lastSentAt.isValid()
        && clock.lastSentAt.msecsTo(now) < ircTypingSendIntervalMs) {
        return false;
    }
    return true;
}

void IrcTypingPublisher::recordSent(const QString& target,
                                    IrcTypingPhase phase,
                                    const QDateTime& now)
{
    TargetClock& clock = m_targets[target];
    clock.lastSentAt = now;
    clock.lastPhase = phase;
    if (phase != IrcTypingPhase::Done)
        clock.suppressDone = false;
}

void IrcTypingPublisher::noteMessageSent(const QString& target)
{
    m_targets[target].suppressDone = true;
}

bool IrcTypingPublisher::wasPublishing(const QString& target) const noexcept
{
    const auto found = m_targets.constFind(target);
    return found != m_targets.cend()
        && found.value().lastPhase == IrcTypingPhase::Active;
}

void IrcTypingPublisher::reset()
{
    m_targets.clear();
}
