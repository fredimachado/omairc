#pragma once

#include "irccommand.h"
#include "ircevent.h"
#include "irchighlight.h"
#include "ircignore.h"
#include "ircmute.h"
#include "ircpref.h"
#include "irceventreducer.h"
#include "ircstatusentry.h"

#include <QSet>
#include <QStringList>

#include <functional>
#include <optional>
#include <set>

class IrcSession;
struct IrcConversationKey;
struct IrcMessage;

// Owns slash-command routing: every verb decision, with the side effects it
// cannot own performed through the host. Join, part, and away decisions stay
// here; the controller only performs them. Ignore, mute, and highlight
// decisions live here too, over the stores the controller persists.
class IrcCommandDispatcher
{
public:
    enum class QuietWire { Privmsg, Notice };
    enum class QuietTarget { Nick, Any };
    struct QuietSend {
        QuietWire wire;
        QuietTarget target;
    };

    // Host callbacks the controller implements: session and selection
    // lookup, model and console side effects, and the preference setters.
    struct Host {
        std::function<QString(IrcComposerSurface surface)> queryNetworkId;
        std::function<IrcSession *(IrcComposerSurface surface)> sessionFor;
        std::function<IrcSession *(const QString& networkId)> sessionForNetwork;
        std::function<IrcSession *()> selectedSession;
        std::function<QString()> selectedTarget;
        std::function<bool()> isChannel;
        std::function<bool()> selectedIsCloseableDirect;
        std::function<std::optional<IrcConversationKey>()> selected;
        std::function<bool()> hasNetworks;
        std::function<QString()> statusNetworkId;

        std::function<void(IrcMessageKind kind, const QString& body)> echoLocal;
        std::function<void(const QString& networkId, const QString& channel)>
            openJoinedChannel;
        std::function<bool(const QString& networkId, const QString& channel)>
            dismissChannel;
        std::function<void()> dropSelectedDirectAndReselect;
        std::function<IrcCommandOutcome(IrcComposerSurface surface)> clearSurface;
        std::function<void(const QString& networkId, const QString& target)>
            rememberOpenDirect;
        std::function<void(const QString& networkId, const QString& target)>
            noteNickDelivery;
        std::function<void()> noteLocalActivity;
        std::function<void(IrcSession *session)> unawayAfterChat;
        std::function<void(const QString& networkId)> noteManualAway;
        std::function<void(const QString& networkId)> noteAwayCleared;
        std::function<void()> clearTypingTarget;
        std::function<IrcCommandOutcome(const QString& body)> sendSelectedMessage;
        std::function<void(IrcSession *session,
                           const QString& target,
                           const QString& body,
                           QuietWire wire)> echoIfPresent;
        std::function<bool(const QString& networkId,
                           const QString& target,
                           bool muted)> applyMute;
        std::function<void(const QString& networkId)> syncHighlightWords;
        std::function<void()> reloadConversations;
        std::function<void(const QString& networkId, const QString& target)>
            selectConversation;
        std::function<void(const IrcWhoisTranscriptEvent&)> apply;
        std::function<void(const IrcStatusEntry&)> record;
        std::function<IrcCommandOutcome(const IrcCommand& command,
                                        IrcComposerSurface surface)> dispatchList;
        std::function<IrcCommandOutcome(const IrcCommand& command,
                                        IrcComposerSurface surface)> dispatchAutoaway;
        std::function<IrcCommandOutcome(const IrcCommand& command,
                                        IrcComposerSurface surface)> dispatchWhois;
        std::function<IrcCommandOutcome(const IrcCommand& command,
                                        IrcComposerSurface surface)> dispatchCtcp;
        std::function<IrcCommandOutcome(const IrcCommand& command,
                                        IrcComposerSurface surface)> dispatchStatus;
        std::function<IrcCommandOutcome(const IrcCommand& command,
                                        IrcComposerSurface surface)> dispatchAvatar;
        std::function<IrcCommandOutcome(const IrcCommand& command,
                                        IrcComposerSurface surface)> dispatchMonitor;
        std::function<bool(IrcPrefName name)> prefEnabled;
        std::function<void(IrcPrefName name, bool enabled)> prefApply;
    };

    IrcCommandDispatcher(IrcEventReducer& reducer,
                         IrcIgnoreStore& ignores,
                         IrcMuteStore& mutes,
                         IrcHighlightStore& highlights,
                         Host host);

    IrcCommandOutcome dispatch(const IrcCommand& command,
                               IrcComposerSurface surface);

    // Pending self-join cancellations, shared with the inbound self-join
    // branch in the controller's handleMessage.
    void noteCancelled(const IrcConversationKey& key);
    bool takeCancelledSelfJoin(const IrcConversationKey& key);
    void forgetNetwork(const QString& networkId);

private:
    static std::optional<QuietSend> quietSendFor(IrcCommand::Verb verb);

    IrcCommandOutcome dispatchQuery(const IrcCommand& command,
                                    IrcComposerSurface surface);
    IrcCommandOutcome dispatchQuietSend(const IrcCommand& command,
                                        IrcComposerSurface surface);
    IrcCommandOutcome dispatchMode(const IrcCommand& command,
                                   IrcComposerSurface surface);
    IrcCommandOutcome dispatchChannelModeWrapper(const IrcCommand& command,
                                                 IrcComposerSurface surface);
    IrcCommandOutcome dispatchServiceMsg(const IrcCommand& command,
                                         IrcComposerSurface surface);
    IrcCommandOutcome dispatchRaw(const IrcCommand& command,
                                  IrcComposerSurface surface);
    IrcCommandOutcome dispatchIgnore(const IrcCommand& command,
                                     IrcComposerSurface surface);
    IrcCommandOutcome dispatchMute(const IrcCommand& command,
                                   IrcComposerSurface surface);
    IrcCommandOutcome dispatchHighlight(const IrcCommand& command,
                                        IrcComposerSurface surface);
    IrcCommandOutcome dispatchHelp(IrcComposerSurface surface);
    IrcCommandOutcome dispatchPref(const IrcCommand& command,
                                   IrcComposerSurface surface);
    IrcCommandOutcome echoPrefFeedback(IrcComposerSurface surface,
                                       const QString& text);
    IrcCommandOutcome setSelectedTopic(const QString& topic);

    IrcEventReducer& m_reducer;
    IrcIgnoreStore& m_ignores;
    IrcMuteStore& m_mutes;
    IrcHighlightStore& m_highlights;
    Host m_host;
    std::set<IrcConversationKey> m_cancelledPendingJoins;
};
