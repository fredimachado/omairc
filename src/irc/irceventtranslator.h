#pragma once

#include "ircevent.h"
#include "ircmessage.h"
#include "ircserverfeatures.h"

#include <QString>

#include <functional>
#include <optional>
#include <vector>

// Display name of a joined channel buffer when the sender is a member of it.
// An empty result leaves a nick-targeted line on the direct-message path.
using IrcJoinedMemberChannel =
    std::function<QString(const QString& channel, const QString& sender)>;

struct IrcHistoryBatch;

std::optional<IrcConversationKey> ircConversationFor(
    const QString& networkId,
    const QString& target,
    const IrcMessage& message,
    const QString& currentNick,
    const IrcServerFeatures& features);

class IrcEventTranslator
{
public:
    static std::vector<IrcEvent> translate(
        const QString& networkId,
        const QString& currentNick,
        const IrcServerFeatures& features,
        const IrcMessage& message,
        const IrcJoinedMemberChannel& joinedMemberChannel = {});
    static std::optional<IrcHistoryEvent> translateHistory(
        const QString& networkId,
        const QString& currentNick,
        const IrcServerFeatures& features,
        const IrcHistoryBatch& batch,
        const IrcJoinedMemberChannel& joinedMemberChannel = {});
};
