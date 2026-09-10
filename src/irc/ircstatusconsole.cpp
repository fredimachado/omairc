#include "ircstatusconsole.h"

#include "ircsession.h"
#include "ircsessionmanager.h"

#include <utility>

namespace
{
QString stateLabel(IrcSession::State state)
{
    switch (state) {
    case IrcSession::State::Idle:
        return QStringLiteral("offline");
    case IrcSession::State::Connecting:
        return QStringLiteral("connecting");
    case IrcSession::State::CapLs:
    case IrcSession::State::CapReq:
    case IrcSession::State::Sasl:
        return QStringLiteral("negotiating");
    case IrcSession::State::Registering:
        return QStringLiteral("registering");
    case IrcSession::State::Registered:
        return QStringLiteral("registered");
    case IrcSession::State::Closing:
        return QStringLiteral("closing");
    case IrcSession::State::Reconnecting:
        return QStringLiteral("reconnecting");
    case IrcSession::State::Failed:
        return QStringLiteral("failed");
    }
    return QStringLiteral("offline");
}

QString stateText(IrcSession::State state)
{
    switch (state) {
    case IrcSession::State::Idle:
        return QStringLiteral("Disconnected");
    case IrcSession::State::Connecting:
        return QStringLiteral("Connecting");
    case IrcSession::State::CapLs:
        return QStringLiteral("Capability discovery");
    case IrcSession::State::CapReq:
        return QStringLiteral("Requesting capabilities");
    case IrcSession::State::Sasl:
        return QStringLiteral("Authenticating");
    case IrcSession::State::Registering:
        return QStringLiteral("Registering");
    case IrcSession::State::Registered:
        return QStringLiteral("Registered");
    case IrcSession::State::Closing:
        return QStringLiteral("Disconnecting");
    case IrcSession::State::Reconnecting:
        return QStringLiteral("Reconnecting");
    case IrcSession::State::Failed:
        return QStringLiteral("Connection failed");
    }
    return QStringLiteral("Disconnected");
}

}

IrcStatusConsole::IrcStatusConsole(IrcSessionManager& sessions,
                                   Dispatch dispatch,
                                   QObject *parent)
    : QObject(parent)
    , m_sessions(sessions)
    , m_lines(m_log)
    , m_dispatch(std::move(dispatch))
{
    connect(&m_log, &IrcNetworkLog::appended, this,
            [this](const QString& networkId, int) { noteLogChanged(networkId); });
    connect(&m_log, &IrcNetworkLog::trimmed, this,
            [this](const QString& networkId, int) { noteLogChanged(networkId); });
    connect(&m_log, &IrcNetworkLog::cleared, this,
            [this](const QString& networkId) { noteLogChanged(networkId); });
}

QAbstractItemModel *IrcStatusConsole::lines()
{
    return &m_lines;
}

bool IrcStatusConsole::isOpen() const
{
    return m_open;
}

int IrcStatusConsole::alerts() const
{
    if (m_open)
        return 0;
    return alertsFor(m_networkId);
}

int IrcStatusConsole::alertsFor(const QString &networkId) const
{
    if (networkId.isEmpty())
        return 0;
    if (m_open && m_networkId == networkId)
        return 0;
    return m_log.alertsSinceSeen(networkId);
}

QString IrcStatusConsole::networkId() const
{
    return m_networkId;
}

void IrcStatusConsole::observe(IrcSession *session)
{
    if (!session || m_observed.contains(session->networkId()))
        return;

    m_observed.insert(session->networkId());
    connect(session, &IrcSession::statusEntry, this, [this](const IrcStatusEntry& entry) {
        m_log.append(entry);
    });
    connect(session, &IrcSession::stateChanged, this,
            [this, session](IrcSession::State) { recordLifecycle(session); });
    connect(session, &IrcSession::registered, this,
            [this, session](const QString& networkId) {
        m_log.append(IrcStatusEntry::lifecycle(
            networkId, IrcLogSeverity::Info, QStringLiteral("registered"),
            QStringLiteral("Registered as %1").arg(session->nick())));
    });
    connect(session, &IrcSession::reconnectScheduled, this,
            [this](const QString& networkId, int delayMilliseconds, int attempt) {
        m_log.append(IrcStatusEntry::lifecycle(
            networkId, IrcLogSeverity::Info, QStringLiteral("reconnect"),
            QStringLiteral("Reconnect attempt %1 in %2 ms")
                .arg(attempt)
                .arg(delayMilliseconds)));
    });
    connect(session, &IrcSession::errorOccurred, this,
            [this](const QString& networkId, IrcSession::ErrorKind, const QString& message) {
        m_log.append(IrcStatusEntry::lifecycle(
            networkId, IrcLogSeverity::Alert, QStringLiteral("error"), message));
    });

    if (m_networkId.isEmpty())
        setNetwork(session->networkId());
}

void IrcStatusConsole::forget(const QString& networkId)
{
    m_observed.remove(networkId);
    m_log.forget(networkId);
    if (m_networkId == networkId) {
        m_lines.show(QString());
        m_networkId.clear();
        emit networkChanged();
        emit alertsChanged();
    }
}

void IrcStatusConsole::setNetwork(const QString& networkId)
{
    if (m_networkId == networkId)
        return;
    m_networkId = networkId;
    m_lines.show(networkId);
    if (m_open)
        m_log.markSeen(m_networkId);
    emit networkChanged();
    emit alertsChanged();
}

void IrcStatusConsole::setOpen(bool open)
{
    if (m_open == open)
        return;
    m_open = open;
    if (m_open)
        m_log.markSeen(m_networkId);
    emit openChanged();
    emit alertsChanged();
}

bool IrcStatusConsole::submit(const QString& input)
{
    const IrcCommand command = IrcCommand::parse(input);
    if (command.verb == IrcCommand::Verb::Empty)
        return false;

    const IrcCommandOutcome outcome = m_dispatch(command);
    if (outcome != IrcCommandOutcome::Sent && !m_networkId.isEmpty()) {
        m_log.append(IrcStatusEntry::outcome(
            m_networkId, ircCommandOutcomeText(outcome, command)));
    }
    return true;
}

bool IrcStatusConsole::clearLog()
{
    if (m_networkId.isEmpty())
        return false;
    m_log.clear(m_networkId);
    return true;
}

IrcSession *IrcStatusConsole::boundSession() const
{
    return m_sessions.findSession(m_networkId);
}

void IrcStatusConsole::recordLifecycle(IrcSession *session)
{
    if (!session)
        return;
    m_log.append(IrcStatusEntry::lifecycle(
        session->networkId(),
        session->state() == IrcSession::State::Failed
            ? IrcLogSeverity::Alert
            : IrcLogSeverity::Info,
        stateLabel(session->state()),
        stateText(session->state())));
}

void IrcStatusConsole::noteLogChanged(const QString& networkId)
{
    if (m_open && networkId == m_networkId)
        m_log.markSeen(m_networkId);
    emit alertsChanged();
}

