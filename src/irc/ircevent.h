#pragma once

#include "ircpresence.h"
#include "ircserverfeatures.h"
#include "irctyping.h"

#include <QChar>
#include <QDateTime>
#include <QString>
#include <QStringList>

#include <optional>
#include <variant>
#include <vector>

struct IrcConversationKey
{
    QString networkId;
    QString normalizedTarget;

    friend bool operator==(const IrcConversationKey& left,
                           const IrcConversationKey& right)
    {
        return left.networkId == right.networkId
            && left.normalizedTarget == right.normalizedTarget;
    }

    friend bool operator!=(const IrcConversationKey& left,
                           const IrcConversationKey& right)
    {
        return !(left == right);
    }

    friend bool operator<(const IrcConversationKey& left,
                          const IrcConversationKey& right)
    {
        if (left.networkId != right.networkId)
            return left.networkId < right.networkId;
        return left.normalizedTarget < right.normalizedTarget;
    }
};

struct IrcWelcomeEvent
{
    QString networkId;
    QString currentNick;
};

struct IrcMessageEvent
{
    IrcConversationKey conversation;
    QString author;
    QString body;
    QDateTime timestamp;
    QString target;
};

struct IrcNoticeEvent
{
    IrcConversationKey conversation;
    QString author;
    QString body;
    QDateTime timestamp;
    QString target;
};

struct IrcActionEvent
{
    IrcConversationKey conversation;
    QString author;
    QString body;
    QDateTime timestamp;
    QString target;
};

struct IrcJoinEvent
{
    QString networkId;
    QString channel;
    QString nick;
};

struct IrcPartEvent
{
    QString networkId;
    QString channel;
    QString nick;
    QString reason;
};

struct IrcQuitEvent
{
    QString networkId;
    QString nick;
    QString reason;
};

struct IrcNickEvent
{
    QString networkId;
    QString oldNick;
    QString newNick;
};

struct IrcKickEvent
{
    QString networkId;
    QString channel;
    QString target;
    QString author;
    QString reason;
};

struct IrcTopicEvent
{
    QString networkId;
    QString channel;
    QString topic;
    QString author;
};

struct IrcName
{
    QString nick;
    IrcPrefixSet ranks;
};

struct IrcNamesEvent
{
    QString networkId;
    QString channel;
    std::vector<IrcName> names;
    bool complete = true;
};

struct IrcModeEvent
{
    QString networkId;
    QString target;
    QString author;
    QString mode;
    QStringList arguments;
};

struct IrcAwayEvent
{
    QString networkId;
    QString nick;
    std::optional<IrcAway> away;
};

struct IrcMemberStatusEvent
{
    QString networkId;
    QString nick;
    QString status;
};

struct IrcTypingEvent
{
    IrcConversationKey conversation;
    QString nick;
    IrcTypingPhase phase = IrcTypingPhase::Active;
    QDateTime receivedAt;
};

using IrcEvent = std::variant<
    IrcWelcomeEvent,
    IrcMessageEvent,
    IrcNoticeEvent,
    IrcActionEvent,
    IrcJoinEvent,
    IrcPartEvent,
    IrcQuitEvent,
    IrcNickEvent,
    IrcKickEvent,
    IrcTopicEvent,
    IrcNamesEvent,
    IrcModeEvent,
    IrcAwayEvent,
    IrcMemberStatusEvent,
    IrcTypingEvent>;
