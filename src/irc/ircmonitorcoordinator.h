#pragma once

#include "irccommand.h"
#include "ircmonitor.h"
#include "ircmute.h"
#include "ircstatusentry.h"

#include <QHash>
#include <QSet>
#include <QStringList>

#include <functional>
#include <optional>

class IrcEventReducer;
class IrcSession;
struct IrcMessage;
struct IrcConversationKey;

// Owns MONITOR subscription state and per-nick presence tracking. The
// coordinator reads the persisted IrcMonitorStore and reports presence
// through the host; online notifications (signal plus inbox) stay on the
// controller.
class IrcMonitorCoordinator
{
public:
    // Host callbacks the controller implements: the status console, the
    // monitorArrived signal plus inbox append, and session/selection lookup.
    struct Host {
        std::function<void(const IrcStatusEntry&)> record;
        std::function<void(const QString& networkId,
                           const QString& display,
                           const QString& body,
                           bool newlyOnline)> notify;
        std::function<QString(IrcComposerSurface surface)> networkIdFor;
        std::function<IrcSession *(IrcComposerSurface surface)> sessionFor;
        std::function<IrcSession *(const QString& networkId)> sessionForNetwork;
        std::function<std::optional<IrcConversationKey>()> selected;
    };

    IrcMonitorCoordinator(IrcEventReducer& reducer,
                          IrcMonitorStore& monitors,
                          IrcMuteStore& mutes,
                          Host host);

    // Session-scoped presence state. It clears when registration ends, on
    // discard, and on registration re-entry; the persisted nick list is the
    // store's and forgets only in forgetNetworkState.
    void forgetPresence(const QString& networkId);

    void subscribeMonitors(const QString& networkId);
    void handleMonitorPresence(const QString& networkId,
                               const IrcMessage& message,
                               bool online);
    void handleMonitorListFull(const QString& networkId,
                               const IrcMessage& message);
    IrcCommandOutcome dispatchMonitor(const IrcCommand& command,
                                      IrcComposerSurface surface);

private:
    enum class Presence { Unknown, Online, Offline };

    QString monitorDisplayNick(const QString& networkId,
                               const QString& nick) const;
    bool monitorNotifyMuted(const QString& networkId,
                            const QString& nick) const;

    IrcEventReducer& m_reducer;
    IrcMonitorStore& m_monitors;
    IrcMuteStore& m_mutes;
    Host m_host;
    QSet<QString> m_monitorSubscribed;
    QHash<QString, QHash<QString, Presence>> m_monitorPresence;
};
