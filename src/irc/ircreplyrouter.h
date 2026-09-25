#pragma once

#include "irccommand.h"
#include "irccapability.h"
#include "ircevent.h"
#include "ircstatusentry.h"

#include <QHash>
#include <QStringList>

#include <functional>
#include <map>
#include <optional>
#include <variant>

class IrcEventReducer;
class IrcSession;
struct IrcMessage;

// Owns WHOIS, CTCP, labeled-reply, and own-metadata watch state and the
// routing of their replies. Arming a watch and routing the reply live in
// one place; echoing a transcript line or a status outcome stays a
// callback into the controller, which still publishes the models.
class IrcReplyRouter
{
public:
    // Host callbacks the controller implements. They are the surfaces the
    // router must not own: model application, the status console, selection,
    // session lookup, and capability lookup.
    struct Host {
        std::function<void(const IrcWhoisTranscriptEvent&)> apply;
        std::function<void(const IrcStatusEntry&)> record;
        std::function<std::optional<IrcConversationKey>()> selected;
        std::function<QString(const QString& networkId)> selfNick;
        std::function<void(const QString& networkId, const QString& url)> persistAvatarUrl;
        std::function<IrcCapabilitySet(const QString& networkId)> capabilities;
        std::function<QString(IrcComposerSurface surface)> networkIdFor;
        std::function<IrcSession *(IrcComposerSurface surface)> sessionFor;
        std::function<IrcSession *()> selectedSession;
        std::function<QString()> selectedTarget;
        std::function<bool()> selectedIsCloseableDirect;
        std::function<bool()> hasNetworks;
    };

    IrcReplyRouter(IrcEventReducer& reducer, Host host);

    // One forget for WHOIS, CTCP, labeled, and own-metadata watches. It is
    // what discard, forget-network, and the welcome event call.
    void forget(const QString& networkId);

    void noteNickDelivery(const QString& networkId, const QString& target);
    // Routes WHOIS, CTCP, labeled, and own-metadata entries. The INVITE
    // inbox branch stays on the controller.
    void routeStatusEntry(const IrcStatusEntry& entry);
    void routeOwnMetadataFail(const QString& networkId, const IrcMessage& message);
    // The controller's reducer publishes IrcMemberMetadataEvent for every
    // metadata line; the watch correlation lives here.
    void routeOwnMetadataReply(const QString& networkId,
                               const QString& nick,
                               const QString& key,
                               const QString& value);
    void requestLabelFinished(const QString& networkId, const QString& requestLabel);

    IrcCommandOutcome dispatchWhois(const IrcCommand& command,
                                    IrcComposerSurface surface);
    IrcCommandOutcome dispatchCtcp(const IrcCommand& command,
                                   IrcComposerSurface surface);
    IrcCommandOutcome dispatchStatus(const IrcCommand& command,
                                     IrcComposerSurface surface);
    IrcCommandOutcome dispatchAvatar(const IrcCommand& command,
                                     IrcComposerSurface surface);

private:
    struct IrcWhoisWatchKey
    {
        QString networkId;
        QString normalizedNick;

        friend bool operator<(const IrcWhoisWatchKey& left,
                              const IrcWhoisWatchKey& right)
        {
            if (left.networkId != right.networkId)
                return left.networkId < right.networkId;
            return left.normalizedNick < right.normalizedNick;
        }
    };
    struct IrcWhoisStatusOnly {};
    using IrcWhoisDestination = std::variant<IrcWhoisStatusOnly, IrcConversationKey>;
    struct IrcWhoisWatch
    {
        IrcWhoisDestination destination;
        bool failedIsAmbiguous = false;
        bool metadataEmitted = false;
    };
    struct IrcLabeledWatchKey
    {
        QString networkId;
        QString requestLabel;

        friend bool operator<(const IrcLabeledWatchKey& left,
                              const IrcLabeledWatchKey& right)
        {
            if (left.networkId != right.networkId)
                return left.networkId < right.networkId;
            return left.requestLabel < right.requestLabel;
        }
    };
    enum class IrcLabeledWatchKind { Whois, Ctcp };
    struct IrcLabeledWatch
    {
        IrcLabeledWatchKind kind = IrcLabeledWatchKind::Whois;
        IrcWhoisDestination destination;
        bool metadataEmitted = false;
    };
    struct IrcCtcpWatchKey
    {
        QString networkId;
        QString normalizedNick;
        QString command;

        friend bool operator<(const IrcCtcpWatchKey& left,
                              const IrcCtcpWatchKey& right)
        {
            if (left.networkId != right.networkId)
                return left.networkId < right.networkId;
            if (left.normalizedNick != right.normalizedNick)
                return left.normalizedNick < right.normalizedNick;
            return left.command < right.command;
        }
    };
    using IrcCtcpDestination = IrcWhoisDestination;
    struct IrcCtcpWatch
    {
        IrcCtcpDestination destination;
    };
    struct IrcOwnMetadataWatch
    {
        IrcWhoisDestination destination;
        enum class Kind { Set, Clear };
        Kind kind = Kind::Set;
        QString value;
    };

    std::optional<IrcWhoisWatchKey> whoisWatchKey(const QString& networkId,
                                                  const QString& nick) const;
    bool sendWhois(IrcSession& session,
                   const QString& nick,
                   IrcWhoisDestination destination);
    QString ctcpQueryName(IrcCommand::Verb verb) const;
    std::optional<IrcCtcpWatchKey> ctcpWatchKey(const QString& networkId,
                                                const QString& nick,
                                                const QString& command) const;
    bool sendCtcpQuery(IrcSession& session,
                       const QString& nick,
                       const QString& command,
                       const QString& argument,
                       IrcCtcpDestination destination);
    void routeWhoisLine(const QString& networkId, const IrcWhoisLine& line);
    void routeLabeledWhois(const QString& networkId,
                           const QString& requestLabel,
                           const IrcWhoisLine& line);
    void routeLabeledCtcp(const QString& networkId,
                          const QString& requestLabel,
                          const IrcCtcpReplyLine& line,
                          const QString& text);
    void routeLabeledStandardReply(const IrcStatusEntry& entry);
    void routeCtcpReply(const QString& networkId,
                        const IrcCtcpReplyLine& line,
                        const QString& text);
    void forgetLabeledWatches(const QString& networkId, IrcLabeledWatchKind kind);
    QStringList whoisMetadataLines(const QString& networkId,
                                   const QString& nick) const;
    IrcCommandOutcome dispatchOwnMetadataClear(IrcSession *session,
                                               const QString& metadataKey);
    IrcCommandOutcome dispatchOwnMetadataSet(IrcSession *session,
                                             const QString& metadataKey,
                                             const QString& value);
    void armOwnMetadataWatch(const QString& networkId,
                             const QString& metadataKey,
                             IrcOwnMetadataWatch::Kind kind,
                             const QString& value);
    void echoOwnMetadataOutcome(const QString& networkId,
                                const IrcWhoisDestination& destination,
                                const QString& text);
    IrcCommandOutcome echoMetadataCommandFeedback(IrcComposerSurface surface,
                                                  const QString& networkId,
                                                  const QString& text);
    void routeOwnMetadataError(const IrcStatusEntry& entry);

    IrcEventReducer& m_reducer;
    Host m_host;
    std::map<IrcWhoisWatchKey, IrcWhoisWatch> m_whoisWatches;
    std::map<IrcCtcpWatchKey, IrcCtcpWatch> m_ctcpWatches;
    std::map<IrcLabeledWatchKey, IrcLabeledWatch> m_labeledWatches;
    QHash<QString, QHash<QString, IrcOwnMetadataWatch>> m_ownMetadataWatches;
};
