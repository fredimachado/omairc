#pragma once

#include "ircevent.h"
#include "irceventreducer.h"

#include <QString>

#include <optional>
#include <variant>

enum class IrcMemberSurface {
    None,
    Reset,
    Row,
};

struct IrcViewNotify {
    bool conversations = false;
    bool messages = false;
    IrcMemberSurface members = IrcMemberSurface::None;
    QString nick;
    bool selection = false;
    bool typing = false;
    bool rearmTyping = false;

    static IrcViewNotify none() { return {}; }

    static IrcViewNotify resetAll()
    {
        IrcViewNotify notify;
        notify.conversations = true;
        notify.messages = true;
        notify.members = IrcMemberSurface::Reset;
        notify.selection = true;
        notify.rearmTyping = true;
        return notify;
    }

    static IrcViewNotify chat()
    {
        IrcViewNotify notify;
        notify.conversations = true;
        notify.messages = true;
        notify.rearmTyping = true;
        return notify;
    }

    static IrcViewNotify topic()
    {
        IrcViewNotify notify;
        notify.selection = true;
        notify.rearmTyping = true;
        return notify;
    }

    static IrcViewNotify memberRow(QString normalizedNick)
    {
        IrcViewNotify notify;
        notify.members = IrcMemberSurface::Row;
        notify.nick = std::move(normalizedNick);
        return notify;
    }

    static IrcViewNotify typingOnly()
    {
        IrcViewNotify notify;
        notify.typing = true;
        notify.rearmTyping = true;
        return notify;
    }

    IrcViewNotify withoutListResets() const
    {
        IrcViewNotify notify;
        notify.typing = typing;
        return notify;
    }
};

struct IrcViewClassifier {
    const IrcEventReducer& reducer;

    IrcViewNotify operator()(const IrcWelcomeEvent&) const
    {
        return IrcViewNotify::resetAll();
    }

    IrcViewNotify operator()(const IrcMessageEvent&) const
    {
        return IrcViewNotify::chat();
    }
    IrcViewNotify operator()(const IrcNoticeEvent&) const
    {
        return IrcViewNotify::chat();
    }
    IrcViewNotify operator()(const IrcActionEvent&) const
    {
        return IrcViewNotify::chat();
    }

    IrcViewNotify operator()(const IrcJoinEvent&) const
    {
        return IrcViewNotify::resetAll();
    }
    IrcViewNotify operator()(const IrcPartEvent&) const
    {
        return IrcViewNotify::resetAll();
    }
    IrcViewNotify operator()(const IrcQuitEvent&) const
    {
        return IrcViewNotify::resetAll();
    }
    IrcViewNotify operator()(const IrcNickEvent&) const
    {
        return IrcViewNotify::resetAll();
    }
    IrcViewNotify operator()(const IrcKickEvent&) const
    {
        return IrcViewNotify::resetAll();
    }
    IrcViewNotify operator()(const IrcModeEvent&) const
    {
        return IrcViewNotify::resetAll();
    }

    IrcViewNotify operator()(const IrcTopicEvent&) const
    {
        return IrcViewNotify::topic();
    }

    IrcViewNotify operator()(const IrcNamesEvent& event) const
    {
        if (!event.complete)
            return IrcViewNotify::none();
        IrcViewNotify notify = IrcViewNotify::resetAll();
        notify.rearmTyping = false;
        return notify;
    }

    IrcViewNotify operator()(const IrcAwayEvent& event) const
    {
        return IrcViewNotify::memberRow(
            reducer.conversationKey(event.networkId, event.nick).normalizedTarget);
    }

    IrcViewNotify operator()(const IrcMemberStatusEvent& event) const
    {
        return IrcViewNotify::memberRow(
            reducer.conversationKey(event.networkId, event.nick).normalizedTarget);
    }

    IrcViewNotify operator()(const IrcSelfAwayEvent&) const
    {
        return IrcViewNotify::none();
    }

    IrcViewNotify operator()(const IrcTypingEvent&) const
    {
        return IrcViewNotify::typingOnly();
    }
};

struct IrcViewConversationKey {
    const IrcEventReducer& reducer;
    const std::optional<IrcConversationKey>& selected;

    std::optional<IrcConversationKey> operator()(const IrcWelcomeEvent&) const
    {
        return selected;
    }
    std::optional<IrcConversationKey> operator()(const IrcMessageEvent& event) const
    {
        return event.conversation;
    }
    std::optional<IrcConversationKey> operator()(const IrcNoticeEvent& event) const
    {
        return event.conversation;
    }
    std::optional<IrcConversationKey> operator()(const IrcActionEvent& event) const
    {
        return event.conversation;
    }
    std::optional<IrcConversationKey> operator()(const IrcJoinEvent& event) const
    {
        return reducer.conversationKey(event.networkId, event.channel);
    }
    std::optional<IrcConversationKey> operator()(const IrcPartEvent& event) const
    {
        return reducer.conversationKey(event.networkId, event.channel);
    }
    std::optional<IrcConversationKey> operator()(const IrcQuitEvent&) const
    {
        return selected;
    }
    std::optional<IrcConversationKey> operator()(const IrcNickEvent&) const
    {
        return selected;
    }
    std::optional<IrcConversationKey> operator()(const IrcKickEvent& event) const
    {
        return reducer.conversationKey(event.networkId, event.channel);
    }
    std::optional<IrcConversationKey> operator()(const IrcTopicEvent& event) const
    {
        return reducer.conversationKey(event.networkId, event.channel);
    }
    std::optional<IrcConversationKey> operator()(const IrcNamesEvent& event) const
    {
        return reducer.conversationKey(event.networkId, event.channel);
    }
    std::optional<IrcConversationKey> operator()(const IrcModeEvent& event) const
    {
        return reducer.conversationKey(event.networkId, event.target);
    }
    std::optional<IrcConversationKey> operator()(const IrcAwayEvent&) const
    {
        return selected;
    }
    std::optional<IrcConversationKey> operator()(const IrcSelfAwayEvent&) const
    {
        return selected;
    }
    std::optional<IrcConversationKey> operator()(const IrcMemberStatusEvent&) const
    {
        return selected;
    }
    std::optional<IrcConversationKey> operator()(const IrcTypingEvent& event) const
    {
        return event.conversation;
    }
};

inline std::optional<IrcConversationKey> viewConversationKey(
    const IrcEvent& event,
    const IrcEventReducer& reducer,
    const std::optional<IrcConversationKey>& selected)
{
    return std::visit(IrcViewConversationKey{reducer, selected}, event);
}

inline bool channelNamesSyncing(const IrcEventReducer& reducer,
                                const std::optional<IrcConversationKey>& key)
{
    if (!key)
        return false;
    const IrcConversationState *conversation = reducer.find(*key);
    const IrcChannelState *channel = conversation ? conversation->channel() : nullptr;
    return channel && channel->namesSyncing;
}

inline IrcViewNotify classifyViewNotify(
    const IrcEvent& event,
    const IrcEventReducer& reducer,
    const std::optional<IrcConversationKey>& selected)
{
    IrcViewNotify notify = std::visit(IrcViewClassifier{reducer}, event);
    if (channelNamesSyncing(reducer, viewConversationKey(event, reducer, selected)))
        return notify.withoutListResets();
    return notify;
}
