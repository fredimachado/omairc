#pragma once

#include "ircautoaway.h"
#include "irccommand.h"
#include "ircevent.h"
#include "ircstatusentry.h"

#include <QObject>
#include <QSet>
#include <QTimer>

#include <functional>
#include <optional>

class IrcSession;

// Owns the auto-away runtime: the idle and grace timers, the tripped state,
// and the per-network auto/manual away sets, plus the application event
// filter that treats input as activity. The runtime installs itself on
// QCoreApplication in its constructor and removes itself in its destructor,
// matching the controller's previous behavior. Away decisions stay here; the
// controller only records status lines and keeps m_unawaySent. The
// /autoaway dispatch lives here too, and the dispatcher calls it.
class IrcAutoawayRuntime : public QObject
{
public:
    // Host callbacks the controller implements: session lookup, self-away
    // queries, unaway bookkeeping, status recording, and transcript echoing.
    struct Host {
        std::function<IrcSession *(const QString& networkId)> findSession;
        std::function<QStringList()> networkIds;
        std::function<bool(const QString& networkId)> selfAway;
        std::function<void(const QString& networkId)> markUnawaySent;
        std::function<void(const QString& networkId, const QString& text)>
            recordStatus;
        std::function<QString(IrcComposerSurface surface)> queryNetworkId;
        std::function<QString()> consoleNetworkId;
        std::function<std::optional<IrcConversationKey>()> selectedKey;
        std::function<void(const IrcEvent& event)> applyEvent;
    };

    IrcAutoawayRuntime(Host host);
    ~IrcAutoawayRuntime() override;

    void setEphemeral(bool ephemeral);
    void loadStored();
    IrcCommandOutcome dispatchAutoaway(const IrcCommand& command,
                                       IrcComposerSurface surface);
    void noteLocalActivity();
    void noteManualAway(const QString& networkId);
    void noteAwayCleared(const QString& networkId);
    void onSessionRegistered(IrcSession *session);
    // Clears the auto-away and manual-away sets for one network. The
    // controller keeps m_unawaySent and clears it beside this call.
    void forgetNetwork(const QString& networkId);

#ifdef OMAIRC_TEST
    void fireAutoawayIdleForTest();
    void fireAutoawayGraceForTest();
    int autoawayIdleIntervalMsForTest() const;
    bool autoawayIdleIsActiveForTest() const;
    int autoawayGraceIntervalMsForTest() const;
    bool autoawayGraceIsActiveForTest() const;
#endif

private:
    bool eventFilter(QObject *watched, QEvent *event) override;
    void saveAutoaway() const;
    void armAutoawayIdle();
    void stopAutoawayTimers();
    void onAutoawayIdle();
    void onAutoawayGrace();
    QString autoawayReason() const;
    void recordAutoawayStatus(const QString& networkId, const QString& text);
    bool markSessionAutoAway(IrcSession *session);
    void refreshAutoAwayReason();
    void tripAutoaway();
    void clearAutoAwayNetworks(bool logCleared = true);
    IrcCommandOutcome echoAutoawayFeedback(IrcComposerSurface surface,
                                           const QString& text);
    IrcCommandOutcome echoAutoawayUsage(IrcComposerSurface surface);

    Host m_host;
    bool m_ephemeral = false;
    IrcAutoawayConfig m_autoaway;
    QTimer m_autoawayIdle;
    QTimer m_autoawayGrace;
    bool m_autoawayGraceArmed = false;
    bool m_autoawayTripped = false;
    QSet<QString> m_autoAwayNetworks;
    QSet<QString> m_manualAwayNetworks;
};
