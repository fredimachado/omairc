#pragma once

#include "irctyping.h"

#include <QDateTime>
#include <QHash>
#include <QString>

class IrcTypingPublisher
{
public:
    bool shouldSend(const QString& target,
                    IrcTypingPhase phase,
                    const QDateTime& now) const;
    void recordSent(const QString& target,
                    IrcTypingPhase phase,
                    const QDateTime& now);
    void noteMessageSent(const QString& target);
    bool wasPublishing(const QString& target) const noexcept;
    void reset();

private:
    struct TargetClock
    {
        QDateTime lastSentAt;
        IrcTypingPhase lastPhase = IrcTypingPhase::Done;
        bool suppressDone = false;
    };

    QHash<QString, TargetClock> m_targets;
};
