#pragma once

#include "ircmessage.h"

#include <QMetaType>
#include <QString>

#include <vector>

// Which reply produced the batch. Bouncer playback is held until the channel
// is joined, then spliced even if the anchor is already gone. Chathistory
// still drops when there is no anchor.
enum class IrcHistoryKind
{
    ChatHistory,
    BouncerPlayback,
};

// One completed replay batch. The lines are still raw protocol so the
// translator can reuse the same rules it applies to live traffic.
struct IrcHistoryBatch
{
    QString target;
    std::vector<IrcMessage> lines;
    IrcHistoryKind kind = IrcHistoryKind::ChatHistory;
};
Q_DECLARE_METATYPE(IrcHistoryBatch)
