#pragma once

#include "ircmessage.h"

#include <QDateTime>
#include <QMetaType>
#include <QString>

#include <vector>

// Which reply produced the batch. Bouncer playback is held until the channel
// is joined, then spliced even if the anchor is already gone. Chathistory
// still drops when there is no anchor. Targets is a CHATHISTORY TARGETS
// answer: names, not transcript lines.
enum class IrcHistoryKind
{
    ChatHistory,
    BouncerPlayback,
    ChatHistoryTargets,
};

// One target named by CHATHISTORY TARGETS. latest is the server's time of
// that target's newest stored message, when the reply included one.
struct IrcHistoryTarget
{
    QString name;
    QDateTime latest;
};

// One completed replay batch. The lines are still raw protocol so the
// translator can reuse the same rules it applies to live traffic.
struct IrcHistoryBatch
{
    QString target;
    std::vector<IrcMessage> lines;
    IrcHistoryKind kind = IrcHistoryKind::ChatHistory;
    // True for CHATHISTORY BEFORE pages requested while reading older lines.
    bool olderPage = false;
    // Set for a CHATHISTORY TARGETS answer. lines stays empty.
    std::vector<IrcHistoryTarget> targets;
};
Q_DECLARE_METATYPE(IrcHistoryBatch)
