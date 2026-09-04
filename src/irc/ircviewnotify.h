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
        notify.typing = true;
        notify.rearmTyping = true;
        return notify;
    }

    static IrcViewNotify chat()
    {
        IrcViewNotify notify;
        notify.conversations = true;
        notify.messages = true;
        notify.typing = true;
        notify.rearmTyping = true;
        return notify;
    }

    static IrcViewNotify topic()
    {
        IrcViewNotify notify;
        notify.selection = true;
        notify.typing = true;
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
    if (!channelNamesSyncing(reducer, selected))
        return notify;
    notify.members = IrcMemberSurface::None;
    notify.selection = false;
    notify.nick.clear();
    return notify;
}
