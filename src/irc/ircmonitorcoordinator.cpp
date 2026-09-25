#include "ircmonitorcoordinator.h"

#include "irccasemapping.h"
#include "irceventreducer.h"
#include "ircserverfeatures.h"
#include "ircsession.h"
#include "ircwiretext.h"

#include <QDateTime>

namespace
{
std::string utf8(const QString& value)
{
    const QByteArray bytes = value.toUtf8();
    return std::string(bytes.constData(), std::size_t(bytes.size()));
}

QString firstToken(const QString& argument)
{
    const int space = argument.indexOf(QLatin1Char(' '));
    return space < 0 ? argument : argument.left(space);
}

QString restAfterFirstToken(const QString& argument)
{
    const int space = argument.indexOf(QLatin1Char(' '));
    return space < 0 ? QString() : argument.mid(space + 1).trimmed();
}

QString parameter(const IrcMessage& message, std::size_t index)
{
    if (index >= message.parameters.size())
        return {};
    return ircWireText(message.parameters[index]);
}

QString foldedNick(const IrcCaseMapping& mapping, const QString& nick)
{
    const std::string folded = mapping.normalize(utf8(nick));
    return QString::fromUtf8(folded.data(), qsizetype(folded.size()));
}

QString monitorTargetNick(const QString& target)
{
    const int bang = target.indexOf(QLatin1Char('!'));
    const QString nick = bang < 0 ? target : target.left(bang);
    return nick.trimmed();
}

QStringList monitorTargetNicks(const IrcMessage& message)
{
    if (message.parameters.size() < 2)
        return {};
    const QString trailing = parameter(message, message.parameters.size() - 1);
    QStringList nicks;
    for (const QString& target :
         trailing.split(QLatin1Char(','), Qt::SkipEmptyParts)) {
        const QString nick = monitorTargetNick(target);
        if (!nick.isEmpty())
            nicks.append(nick);
    }
    return nicks;
}

QString monitorListFullText(const QString& limit, const QString& targets)
{
    QString text = QStringLiteral("Monitor list is full");
    if (!limit.isEmpty())
        text += QStringLiteral(" (%1)").arg(limit);
    if (!targets.isEmpty()) {
        QString shown = targets;
        shown.replace(QLatin1Char(','), QStringLiteral(", "));
        text += QStringLiteral(": %1").arg(shown);
    }
    return text + QLatin1Char('.');
}

bool ignoreNickIsUsable(const QString& nick, const IrcServerFeatures& features)
{
    if (nick.isEmpty())
        return false;
    if (features.isChannel(utf8(nick)))
        return false;
    if (nick.contains(QLatin1Char('!')) || nick.contains(QLatin1Char('@'))
        || nick.contains(QLatin1Char('*')) || nick.contains(QLatin1Char(','))) {
        return false;
    }
    return true;
}
}

IrcMonitorCoordinator::IrcMonitorCoordinator(IrcEventReducer& reducer,
                                             IrcMonitorStore& monitors,
                                             IrcMuteStore& mutes,
                                             Host host)
    : m_reducer(reducer)
    , m_monitors(monitors)
    , m_mutes(mutes)
    , m_host(std::move(host))
{
}

void IrcMonitorCoordinator::forgetPresence(const QString& networkId)
{
    m_monitorSubscribed.remove(networkId);
    m_monitorPresence.remove(networkId);
}

QString IrcMonitorCoordinator::monitorDisplayNick(const QString& networkId,
                                                  const QString& nick) const
{
    const IrcCaseMapping& mapping =
        m_reducer.serverFeatures(networkId).caseMapping();
    for (const QString& stored : m_monitors.nicks(networkId)) {
        if (mapping.equals(utf8(stored), utf8(nick)))
            return stored;
    }
    return nick;
}

bool IrcMonitorCoordinator::monitorNotifyMuted(const QString& networkId,
                                               const QString& nick) const
{
    const IrcCaseMapping& mapping =
        m_reducer.serverFeatures(networkId).caseMapping();
    if (m_mutes.contains(networkId, nick, mapping))
        return true;
    const IrcConversationState *conversation =
        m_reducer.find(m_reducer.conversationKey(networkId, nick));
    return conversation && conversation->muted;
}

void IrcMonitorCoordinator::subscribeMonitors(const QString& networkId)
{
    if (m_monitorSubscribed.contains(networkId))
        return;
    const IrcServerFeatures& features = m_reducer.serverFeatures(networkId);
    if (!features.monitorAdvertised())
        return;
    IrcSession *session = m_host.sessionForNetwork(networkId);
    if (!session || session->state() != IrcSession::State::Registered)
        return;

    m_monitorSubscribed.insert(networkId);
    m_monitorPresence.remove(networkId);

    QStringList nicks = m_monitors.listed(networkId, features.caseMapping());
    if (const std::optional<std::size_t> limit = features.monitorLimit()) {
        if (nicks.size() > int(*limit))
            nicks = nicks.mid(0, int(*limit));
    }
    if (nicks.isEmpty())
        return;
    session->sendMonitor(QLatin1Char('+'), nicks);
}

void IrcMonitorCoordinator::handleMonitorPresence(const QString& networkId,
                                                  const IrcMessage& message,
                                                  bool online)
{
    const IrcServerFeatures& features = m_reducer.serverFeatures(networkId);
    const IrcCaseMapping& mapping = features.caseMapping();
    QHash<QString, Presence>& states = m_monitorPresence[networkId];
    for (const QString& nick : monitorTargetNicks(message)) {
        if (!m_monitors.contains(networkId, nick, mapping))
            continue;
        const QString key = foldedNick(mapping, nick);
        const Presence previous = states.value(key, Presence::Unknown);
        const Presence next = online ? Presence::Online : Presence::Offline;
        states.insert(key, next);
        if (previous == Presence::Unknown || previous == next)
            continue;
        const QString display = monitorDisplayNick(networkId, nick);
        const QString body = online ? QStringLiteral("is online")
                                    : QStringLiteral("is offline");
        m_host.record(IrcStatusEntry::outcome(
            networkId, QStringLiteral("%1 %2").arg(display, body)));
        if (monitorNotifyMuted(networkId, nick))
            continue;
        m_host.notify(networkId, display, body, online && previous != Presence::Online);
    }
}

void IrcMonitorCoordinator::handleMonitorListFull(const QString& networkId,
                                                  const IrcMessage& message)
{
    const QString limit = parameter(message, 1);
    const QString targets = parameter(message, 2);
    const IrcCaseMapping& mapping =
        m_reducer.serverFeatures(networkId).caseMapping();
    for (const QString& nick :
         targets.split(QLatin1Char(','), Qt::SkipEmptyParts)) {
        const QString trimmed = monitorTargetNick(nick);
        if (trimmed.isEmpty())
            continue;
        m_monitors.remove(networkId, trimmed, mapping);
        m_monitorPresence[networkId].remove(foldedNick(mapping, trimmed));
    }
    m_host.record(IrcStatusEntry::outcome(
        networkId, monitorListFullText(limit, targets)));
}

IrcCommandOutcome IrcMonitorCoordinator::dispatchMonitor(const IrcCommand& command,
                                                         IrcComposerSurface surface)
{
    const QString networkId = m_host.networkIdFor(surface);
    if (networkId.isEmpty()) {
        if (surface == IrcComposerSurface::Conversation && !m_host.selected())
            return IrcCommandOutcome::WrongScope;
        return IrcCommandOutcome::Refused;
    }
    IrcSession *session = m_host.sessionFor(surface);
    if (!session || session->state() != IrcSession::State::Registered)
        return IrcCommandOutcome::NotConnected;

    const IrcServerFeatures& features = m_reducer.serverFeatures(networkId);
    const IrcCaseMapping& mapping = features.caseMapping();
    const bool advertised = features.monitorAdvertised();

    QString text;
    if (command.verb == IrcCommand::Verb::Monitored) {
        if (!command.argument.isEmpty())
            return IrcCommandOutcome::Refused;
        const QStringList nicks = m_monitors.listed(networkId, mapping);
        if (nicks.isEmpty()) {
            text = QStringLiteral("Not watching anyone");
        } else {
            QStringList parts;
            const QHash<QString, Presence> states =
                m_monitorPresence.value(networkId);
            for (const QString& nick : nicks) {
                const Presence presence =
                    states.value(foldedNick(mapping, nick),
                                 Presence::Unknown);
                const char *state = "unknown";
                if (presence == Presence::Online)
                    state = "online";
                else if (presence == Presence::Offline)
                    state = "offline";
                parts.append(QStringLiteral("%1 (%2)").arg(
                    nick, QLatin1String(state)));
            }
            text = QStringLiteral("Watching: %1").arg(
                parts.join(QStringLiteral(", ")));
        }
        m_host.record(IrcStatusEntry::outcome(networkId, text));
        return IrcCommandOutcome::Sent;
    }

    const QString nick = firstToken(command.argument);
    if (!restAfterFirstToken(command.argument).isEmpty()
        || !ignoreNickIsUsable(nick, features)) {
        return IrcCommandOutcome::Refused;
    }

    if (!advertised) {
        m_host.record(IrcStatusEntry::outcome(
            networkId,
            QStringLiteral("This network does not support MONITOR.")));
        return IrcCommandOutcome::Sent;
    }

    if (command.verb == IrcCommand::Verb::Monitor) {
        if (m_monitors.contains(networkId, nick, mapping)) {
            text = QStringLiteral("Already watching %1").arg(nick);
        } else if (const std::optional<std::size_t> limit = features.monitorLimit();
                   limit
                   && m_monitors.listed(networkId, mapping).size()
                       >= int(*limit)) {
            m_host.record(IrcStatusEntry::outcome(
                networkId,
                monitorListFullText(QString::number(qulonglong(*limit)), nick)));
            return IrcCommandOutcome::Sent;
        } else if (m_monitors.add(networkId, nick, mapping)) {
            if (!session->sendMonitor(QLatin1Char('+'), {nick})) {
                m_monitors.remove(networkId, nick, mapping);
                return IrcCommandOutcome::Refused;
            }
            text = QStringLiteral("Watching %1").arg(nick);
        } else {
            text = QStringLiteral("Already watching %1").arg(nick);
        }
    } else {
        const bool removed = m_monitors.remove(networkId, nick, mapping);
        if (removed) {
            m_monitorPresence[networkId].remove(foldedNick(mapping, nick));
            session->sendMonitor(QLatin1Char('-'), {nick});
            text = QStringLiteral("No longer watching %1").arg(nick);
        } else {
            text = QStringLiteral("Not watching %1").arg(nick);
        }
    }
    m_host.record(IrcStatusEntry::outcome(networkId, text));
    return IrcCommandOutcome::Sent;
}
