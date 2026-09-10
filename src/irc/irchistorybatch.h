#pragma once

#include "ircmessage.h"

#include <QMetaType>
#include <QString>

#include <vector>

// One completed CHATHISTORY reply batch. The lines are still raw protocol so
// the translator can reuse the same rules it applies to live traffic.
struct IrcHistoryBatch
{
    QString target;
    std::vector<IrcMessage> lines;
};
Q_DECLARE_METATYPE(IrcHistoryBatch)
