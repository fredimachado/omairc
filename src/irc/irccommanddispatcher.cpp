#include "irccommanddispatcher.h"

#include "ircchannelmode.h"
#include "irccasemapping.h"
#include "irceventreducer.h"
#include "ircjointarget.h"
#include "ircserverfeatures.h"
#include "ircsession.h"
#include "ircwiretext.h"

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

bool muteTargetIsUsable(const QString& target, const IrcServerFeatures& features)
{
    if (target.isEmpty())
        return false;
    if (features.isChannel(utf8(target)))
        return true;
    return ignoreNickIsUsable(target, features);
}
}

IrcCommandDispatcher::IrcCommandDispatcher(IrcEventReducer& reducer,
                                           IrcIgnoreStore& ignores,
                                           IrcMuteStore& mutes,
                                           IrcHighlightStore& highlights,
                                           Host host)
    : m_reducer(reducer)
    , m_ignores(ignores)
    , m_mutes(mutes)
    , m_highlights(highlights)
    , m_host(std::move(host))
{
}

std::optional<IrcCommandDispatcher::QuietSend>
IrcCommandDispatcher::quietSendFor(IrcCommand::Verb verb)
{
    switch (verb) {
    case IrcCommand::Verb::Msg:
        return QuietSend{QuietWire::Privmsg, QuietTarget::Nick};
    case IrcCommand::Verb::Notice:
        return QuietSend{QuietWire::Notice, QuietTarget::Any};
    default:
        return std::nullopt;
    }
}

IrcCommandOutcome IrcCommandDispatcher::dispatch(const IrcCommand& command,
                                                 IrcComposerSurface surface)
{
    if (command.verb == IrcCommand::Verb::Empty)
        return IrcCommandOutcome::Sent;
    if (command.verb == IrcCommand::Verb::Unknown)
        return IrcCommandOutcome::Unsupported;
    if (!command.allowedOn(surface))
        return IrcCommandOutcome::WrongScope;

    if (command.verb == IrcCommand::Verb::Say)
        return m_host.sendSelectedMessage(command.argument);

    if (command.verb == IrcCommand::Verb::Action) {
        IrcSession *session = m_host.selectedSession();
        if (!session || !m_host.selected())
            return IrcCommandOutcome::WrongScope;
        const bool sent = session->sendAction(m_host.selectedTarget(),
                                              command.argument);
        if (sent) {
            m_host.rememberOpenDirect(session->networkId(),
                                      m_host.selectedTarget());
            m_host.noteNickDelivery(session->networkId(), m_host.selectedTarget());
            m_host.echoLocal(IrcMessageKind::Action, command.argument);
            m_host.clearTypingTarget();
            m_host.noteLocalActivity();
            m_host.unawayAfterChat(session);
        }
        return sent ? IrcCommandOutcome::Sent : IrcCommandOutcome::Refused;
    }

    if (command.verb == IrcCommand::Verb::Query)
        return dispatchQuery(command, surface);

    if (quietSendFor(command.verb))
        return dispatchQuietSend(command, surface);

    if (command.verb == IrcCommand::Verb::Mode)
        return dispatchMode(command, surface);

    if (command.verb == IrcCommand::Verb::Op
        || command.verb == IrcCommand::Verb::Deop
        || command.verb == IrcCommand::Verb::Voice
        || command.verb == IrcCommand::Verb::Devoice
        || command.verb == IrcCommand::Verb::Ban) {
        return dispatchChannelModeWrapper(command, surface);
    }

    if (command.verb == IrcCommand::Verb::Ns
        || command.verb == IrcCommand::Verb::Cs
        || command.verb == IrcCommand::Verb::Znc) {
        return dispatchServiceMsg(command, surface);
    }

    if (command.verb == IrcCommand::Verb::Raw)
        return dispatchRaw(command, surface);

    if (command.verb == IrcCommand::Verb::Whois)
        return m_host.dispatchWhois(command, surface);

    if (command.verb == IrcCommand::Verb::Ping
        || command.verb == IrcCommand::Verb::Time
        || command.verb == IrcCommand::Verb::Version) {
        return m_host.dispatchCtcp(command, surface);
    }

    if (command.verb == IrcCommand::Verb::Clear)
        return m_host.clearSurface(surface);

    if (command.verb == IrcCommand::Verb::Close) {
        if (!m_host.selectedIsCloseableDirect())
            return IrcCommandOutcome::WrongScope;
        m_host.dropSelectedDirectAndReselect();
        return IrcCommandOutcome::Sent;
    }

    if (command.verb == IrcCommand::Verb::Topic)
        return setSelectedTopic(command.argument);

    if (command.verb == IrcCommand::Verb::Ignore
        || command.verb == IrcCommand::Verb::Unignore
        || command.verb == IrcCommand::Verb::Ignored) {
        return dispatchIgnore(command, surface);
    }

    if (command.verb == IrcCommand::Verb::Monitor
        || command.verb == IrcCommand::Verb::Unmonitor
        || command.verb == IrcCommand::Verb::Monitored) {
        return m_host.dispatchMonitor(command, surface);
    }

    if (command.verb == IrcCommand::Verb::Mute
        || command.verb == IrcCommand::Verb::Unmute
        || command.verb == IrcCommand::Verb::Muted) {
        return dispatchMute(command, surface);
    }

    if (command.verb == IrcCommand::Verb::Highlight
        || command.verb == IrcCommand::Verb::Unhighlight
        || command.verb == IrcCommand::Verb::Highlights) {
        return dispatchHighlight(command, surface);
    }

    if (command.verb == IrcCommand::Verb::Help)
        return dispatchHelp(surface);

    if (command.verb == IrcCommand::Verb::Autoaway)
        return m_host.dispatchAutoaway(command, surface);

    if (command.verb == IrcCommand::Verb::Pref)
        return dispatchPref(command, surface);

    if (command.verb == IrcCommand::Verb::List)
        return m_host.dispatchList(command, surface);

    if (command.verb == IrcCommand::Verb::Status)
        return m_host.dispatchStatus(command, surface);

    if (command.verb == IrcCommand::Verb::Avatar)
        return m_host.dispatchAvatar(command, surface);

    IrcSession *active = m_host.sessionFor(surface);
    if (!active) {
        if (surface == IrcComposerSurface::Conversation && !m_host.selected()
                && m_host.hasNetworks()) {
            return IrcCommandOutcome::Refused;
        }
        return IrcCommandOutcome::NotConnected;
    }
    if (active->state() != IrcSession::State::Registered
            && command.verb != IrcCommand::Verb::Quit) {
        return IrcCommandOutcome::NotConnected;
    }

    bool sent = false;
    switch (command.verb) {
    case IrcCommand::Verb::Join: {
        const IrcServerFeatures& features =
            m_reducer.serverFeatures(active->networkId());
        std::optional<QVector<IrcJoinTarget>> targets;
        if (command.argument.isEmpty()) {
            const std::optional<IrcPendingInvite> pending = active->pendingInvite();
            if (!pending)
                return IrcCommandOutcome::Refused;
            const std::optional<IrcJoinTarget> target =
                IrcJoinTarget::make(pending->channel, std::nullopt, features);
            if (!target)
                return IrcCommandOutcome::Refused;
            targets = QVector<IrcJoinTarget>{*target};
        } else {
            targets = ircParseJoinTargets(command.argument, features);
            if (!targets)
                return IrcCommandOutcome::Refused;
        }
        sent = true;
        for (const IrcJoinTarget& target : *targets) {
            const bool wrote = active->join(target);
            if (wrote) {
                m_cancelledPendingJoins.erase(
                    m_reducer.conversationKey(active->networkId(),
                                              target.channel()));
            }
            sent = wrote && sent;
        }
        if (sent)
            m_host.openJoinedChannel(active->networkId(),
                                     targets->constLast().channel());
        break;
    }
    case IrcCommand::Verb::Part: {
        QString channel = firstToken(command.argument);
        if (channel.isEmpty()) {
            if (!m_host.selected() || !m_host.isChannel())
                return IrcCommandOutcome::WrongScope;
            if (m_host.selected()->networkId != m_host.queryNetworkId(surface))
                return IrcCommandOutcome::Refused;
            channel = m_host.selectedTarget();
            if (channel.isEmpty())
                return IrcCommandOutcome::Refused;
        }
        const IrcConversationKey key =
            m_reducer.conversationKey(active->networkId(), channel);
        const IrcConversationState *conversation = m_reducer.find(key);
        const bool joined = conversation
            && conversation->channel()
            && conversation->channel()->joined;
        if (m_host.dismissChannel(active->networkId(), channel)) {
            if (joined)
                active->part(channel);
            else
                m_cancelledPendingJoins.insert(key);
            sent = true;
            break;
        }
        sent = active->part(channel);
        break;
    }
    case IrcCommand::Verb::Kick: {
        const QString first = firstToken(command.argument);
        if (first.isEmpty())
            return IrcCommandOutcome::Refused;
        const QString networkId = m_host.queryNetworkId(surface);
        if (m_reducer.serverFeatures(networkId).isChannel(utf8(first))) {
            const QString afterChannel = restAfterFirstToken(command.argument);
            const QString nick = firstToken(afterChannel);
            if (nick.isEmpty())
                return IrcCommandOutcome::Refused;
            sent = active->kick(first, nick, restAfterFirstToken(afterChannel));
            break;
        }
        if (!m_host.selected() || !m_host.isChannel())
            return IrcCommandOutcome::WrongScope;
        if (m_host.selected()->networkId != networkId)
            return IrcCommandOutcome::Refused;
        const QString channel = m_host.selectedTarget();
        sent = !channel.isEmpty()
            && active->kick(channel, first, restAfterFirstToken(command.argument));
        break;
    }
    case IrcCommand::Verb::Invite: {
        const QString nick = firstToken(command.argument);
        if (nick.isEmpty())
            return IrcCommandOutcome::WrongScope;
        const QString networkId = m_host.queryNetworkId(surface);
        const IrcServerFeatures& features = m_reducer.serverFeatures(networkId);
        if (features.isChannel(utf8(nick)))
            return IrcCommandOutcome::Refused;
        const QString rest = restAfterFirstToken(command.argument);
        QString channel;
        if (rest.isEmpty()) {
            if (surface == IrcComposerSurface::Status || !m_host.selected()
                    || !m_host.isChannel()) {
                return IrcCommandOutcome::WrongScope;
            }
            if (m_host.selected()->networkId != networkId)
                return IrcCommandOutcome::Refused;
            channel = m_host.selectedTarget();
        } else {
            channel = firstToken(rest);
            if (!features.isChannel(utf8(channel)) || !restAfterFirstToken(rest).isEmpty())
                return IrcCommandOutcome::Refused;
        }
        sent = !channel.isEmpty() && active->invite(nick, channel);
        break;
    }
    case IrcCommand::Verb::Nick:
        sent = !command.argument.isEmpty() && active->changeNick(command.argument);
        break;
    case IrcCommand::Verb::Quit:
        sent = active->quit(command.argument);
        break;
    case IrcCommand::Verb::Away:
        sent = active->setAway(command.argument);
        if (sent) {
            if (command.argument.trimmed().isEmpty())
                m_host.noteAwayCleared(active->networkId());
            else
                m_host.noteManualAway(active->networkId());
        }
        break;
    case IrcCommand::Verb::Back:
        sent = active->clearAway();
        if (sent)
            m_host.noteAwayCleared(active->networkId());
        break;
    default:
        return IrcCommandOutcome::Unsupported;
    }
    return sent ? IrcCommandOutcome::Sent : IrcCommandOutcome::Refused;
}

void IrcCommandDispatcher::noteCancelled(const IrcConversationKey& key)
{
    m_cancelledPendingJoins.insert(key);
}

bool IrcCommandDispatcher::takeCancelledSelfJoin(const IrcConversationKey& key)
{
    return m_cancelledPendingJoins.erase(key) > 0;
}

void IrcCommandDispatcher::forgetNetwork(const QString& networkId)
{
    for (auto it = m_cancelledPendingJoins.begin();
         it != m_cancelledPendingJoins.end(); ) {
        if (it->networkId == networkId)
            it = m_cancelledPendingJoins.erase(it);
        else
            ++it;
    }
}

IrcCommandOutcome IrcCommandDispatcher::dispatchQuery(const IrcCommand& command,
                                                      IrcComposerSurface surface)
{
    const QString nick = firstToken(command.argument);
    const QString rest = restAfterFirstToken(command.argument);
    if (nick.isEmpty())
        return IrcCommandOutcome::Refused;

    const QString networkId = m_host.queryNetworkId(surface);
    if (networkId.isEmpty()) {
        if (surface == IrcComposerSurface::Conversation)
            return IrcCommandOutcome::WrongScope;
        return IrcCommandOutcome::Refused;
    }

    if (m_reducer.serverFeatures(networkId).isChannel(utf8(nick)))
        return IrcCommandOutcome::Refused;

    if (!rest.isEmpty()) {
        IrcSession *session = m_host.sessionForNetwork(networkId);
        if (!session || session->state() != IrcSession::State::Registered)
            return IrcCommandOutcome::NotConnected;
    }

    const IrcConversationKey key = m_reducer.conversationKey(networkId, nick);
    if (!m_reducer.ensureConversation(key, nick, IrcConversationCause::UserOpen))
        return IrcCommandOutcome::Refused;
    m_host.rememberOpenDirect(networkId, nick);
    m_host.reloadConversations();
    m_host.selectConversation(networkId, nick);
    if (rest.isEmpty())
        return IrcCommandOutcome::Sent;
    return m_host.sendSelectedMessage(rest);
}

IrcCommandOutcome IrcCommandDispatcher::dispatchQuietSend(const IrcCommand& command,
                                                          IrcComposerSurface surface)
{
    const std::optional<QuietSend> spec = quietSendFor(command.verb);
    if (!spec)
        return IrcCommandOutcome::Unsupported;

    const QString target = firstToken(command.argument);
    const QString body = restAfterFirstToken(command.argument);
    if (target.isEmpty() || body.isEmpty())
        return IrcCommandOutcome::Refused;

    const QString networkId = m_host.queryNetworkId(surface);
    if (networkId.isEmpty()) {
        if (surface == IrcComposerSurface::Conversation)
            return IrcCommandOutcome::WrongScope;
        return IrcCommandOutcome::Refused;
    }

    if (spec->target == QuietTarget::Nick
        && m_reducer.serverFeatures(networkId).isChannel(utf8(target))) {
        return IrcCommandOutcome::Refused;
    }

    IrcSession *session = m_host.sessionForNetwork(networkId);
    if (!session || session->state() != IrcSession::State::Registered)
        return IrcCommandOutcome::NotConnected;

    const bool sent = spec->wire == QuietWire::Privmsg
        ? session->sendPrivmsg(target, body)
        : session->sendNotice(target, body);
    if (!sent)
        return IrcCommandOutcome::Refused;

    m_host.noteNickDelivery(networkId, target);
    const IrcConversationKey key = m_reducer.conversationKey(networkId, target);
    m_reducer.ensureConversation(key, target, IrcConversationCause::QuietSend);
    m_host.echoIfPresent(session, target, body, spec->wire);
    if (spec->wire == QuietWire::Privmsg) {
        m_host.noteLocalActivity();
        m_host.unawayAfterChat(session);
    }
    return IrcCommandOutcome::Sent;
}

IrcCommandOutcome IrcCommandDispatcher::dispatchMode(const IrcCommand& command,
                                                     IrcComposerSurface surface)
{
    if (command.argument.isEmpty())
        return IrcCommandOutcome::Refused;

    const QString networkId = m_host.queryNetworkId(surface);
    if (networkId.isEmpty()) {
        if (surface == IrcComposerSurface::Conversation)
            return IrcCommandOutcome::WrongScope;
        return IrcCommandOutcome::Refused;
    }

    const std::optional<IrcChannelModeRequest> request =
        IrcChannelModeRequest::parse(command.argument,
                                     m_reducer.serverFeatures(networkId));
    if (!request)
        return IrcCommandOutcome::Refused;

    IrcSession *session = m_host.sessionForNetwork(networkId);
    if (!session || session->state() != IrcSession::State::Registered)
        return IrcCommandOutcome::NotConnected;
    return session->sendChannelMode(*request) ? IrcCommandOutcome::Sent
                                              : IrcCommandOutcome::Refused;
}

IrcCommandOutcome IrcCommandDispatcher::dispatchChannelModeWrapper(
    const IrcCommand& command, IrcComposerSurface surface)
{
    if (!m_host.selected() || !m_host.isChannel())
        return IrcCommandOutcome::WrongScope;
    if (m_host.selected()->networkId != m_host.queryNetworkId(surface))
        return IrcCommandOutcome::Refused;

    const QString token = firstToken(command.argument);
    if (token.isEmpty() || !restAfterFirstToken(command.argument).isEmpty())
        return IrcCommandOutcome::Refused;

    QString modes;
    switch (command.verb) {
    case IrcCommand::Verb::Op:
        modes = QStringLiteral("+o");
        break;
    case IrcCommand::Verb::Deop:
        modes = QStringLiteral("-o");
        break;
    case IrcCommand::Verb::Voice:
        modes = QStringLiteral("+v");
        break;
    case IrcCommand::Verb::Devoice:
        modes = QStringLiteral("-v");
        break;
    case IrcCommand::Verb::Ban:
        modes = QStringLiteral("+b");
        break;
    default:
        return IrcCommandOutcome::Unsupported;
    }

    QString parameter = token;
    if (command.verb == IrcCommand::Verb::Ban
        && !token.contains(QLatin1Char('!'))
        && !token.contains(QLatin1Char('@'))) {
        parameter = token + QStringLiteral("!*@*");
    }

    IrcCommand mode;
    mode.verb = IrcCommand::Verb::Mode;
    mode.argument = QStringLiteral("%1 %2 %3")
                        .arg(m_host.selectedTarget(), modes, parameter);
    return dispatchMode(mode, surface);
}

IrcCommandOutcome IrcCommandDispatcher::dispatchServiceMsg(const IrcCommand& command,
                                                           IrcComposerSurface surface)
{
    QString nick;
    switch (command.verb) {
    case IrcCommand::Verb::Ns:
        nick = QStringLiteral("NickServ");
        break;
    case IrcCommand::Verb::Cs:
        nick = QStringLiteral("ChanServ");
        break;
    case IrcCommand::Verb::Znc:
        nick = QStringLiteral("*status");
        break;
    default:
        return IrcCommandOutcome::Unsupported;
    }
    IrcCommand msg = command;
    msg.verb = IrcCommand::Verb::Msg;
    msg.argument = command.argument.isEmpty()
        ? nick
        : nick + QLatin1Char(' ') + command.argument;
    return dispatchQuietSend(msg, surface);
}

IrcCommandOutcome IrcCommandDispatcher::dispatchRaw(const IrcCommand& command,
                                                    IrcComposerSurface surface)
{
    if (command.argument.isEmpty())
        return IrcCommandOutcome::Refused;

    const QString networkId = m_host.queryNetworkId(surface);
    if (networkId.isEmpty()) {
        if (surface == IrcComposerSurface::Conversation)
            return IrcCommandOutcome::WrongScope;
        return IrcCommandOutcome::Refused;
    }

    IrcSession *session = m_host.sessionForNetwork(networkId);
    if (!session || session->state() != IrcSession::State::Registered)
        return IrcCommandOutcome::NotConnected;
    return session->sendRaw(command.argument) ? IrcCommandOutcome::Sent
                                              : IrcCommandOutcome::Refused;
}

IrcCommandOutcome IrcCommandDispatcher::dispatchIgnore(const IrcCommand& command,
                                                       IrcComposerSurface surface)
{
    const QString networkId = m_host.queryNetworkId(surface);
    if (networkId.isEmpty()) {
        if (surface == IrcComposerSurface::Conversation && !m_host.selected())
            return IrcCommandOutcome::WrongScope;
        return IrcCommandOutcome::Refused;
    }
    IrcSession *session = m_host.sessionFor(surface);
    if (!session || session->state() == IrcSession::State::Idle
        || session->state() == IrcSession::State::Failed) {
        return IrcCommandOutcome::NotConnected;
    }

    const IrcServerFeatures& features = m_reducer.serverFeatures(networkId);
    const IrcCaseMapping& mapping = features.caseMapping();
    QString text;
    if (command.verb == IrcCommand::Verb::Ignored) {
        if (!command.argument.isEmpty())
            return IrcCommandOutcome::Refused;
        const QStringList nicks = m_ignores.listed(networkId, mapping);
        text = nicks.isEmpty()
            ? QStringLiteral("Not ignoring anyone")
            : QStringLiteral("Ignoring: %1").arg(nicks.join(QStringLiteral(", ")));
    } else {
        const QString nick = firstToken(command.argument);
        if (!restAfterFirstToken(command.argument).isEmpty()
            || !ignoreNickIsUsable(nick, features)) {
            return IrcCommandOutcome::Refused;
        }
        if (command.verb == IrcCommand::Verb::Ignore) {
            const bool added = m_ignores.add(networkId, nick, mapping);
            text = added ? QStringLiteral("Ignoring %1").arg(nick)
                         : QStringLiteral("Already ignoring %1").arg(nick);
        } else {
            const bool removed = m_ignores.remove(networkId, nick, mapping);
            text = removed ? QStringLiteral("No longer ignoring %1").arg(nick)
                           : QStringLiteral("Not ignoring %1").arg(nick);
        }
    }
    m_host.record(IrcStatusEntry::outcome(networkId, text));
    return IrcCommandOutcome::Sent;
}

IrcCommandOutcome IrcCommandDispatcher::dispatchMute(const IrcCommand& command,
                                                     IrcComposerSurface surface)
{
    const QString networkId = m_host.queryNetworkId(surface);
    if (networkId.isEmpty()) {
        if (surface == IrcComposerSurface::Conversation && !m_host.selected())
            return IrcCommandOutcome::WrongScope;
        return IrcCommandOutcome::Refused;
    }
    IrcSession *session = m_host.sessionFor(surface);
    if (!session || session->state() == IrcSession::State::Idle
        || session->state() == IrcSession::State::Failed) {
        return IrcCommandOutcome::NotConnected;
    }

    const IrcServerFeatures& features = m_reducer.serverFeatures(networkId);
    const IrcCaseMapping& mapping = features.caseMapping();
    QString text;
    if (command.verb == IrcCommand::Verb::Muted) {
        if (!command.argument.isEmpty())
            return IrcCommandOutcome::Refused;
        const QStringList targets = m_mutes.listed(networkId, mapping);
        text = targets.isEmpty()
            ? QStringLiteral("Not muting anything")
            : QStringLiteral("Muted: %1").arg(targets.join(QStringLiteral(", ")));
    } else {
        QString target = firstToken(command.argument);
        if (!restAfterFirstToken(command.argument).isEmpty())
            return IrcCommandOutcome::Refused;
        if (target.isEmpty()) {
            if (surface == IrcComposerSurface::Status || !m_host.selected())
                return IrcCommandOutcome::WrongScope;
            target = m_host.selectedTarget();
        }
        if (!muteTargetIsUsable(target, features))
            return IrcCommandOutcome::Refused;
        if (command.verb == IrcCommand::Verb::Mute) {
            const bool added = m_host.applyMute(networkId, target, true);
            text = added ? QStringLiteral("Muted %1").arg(target)
                         : QStringLiteral("Already muted %1").arg(target);
        } else {
            const bool removed = m_host.applyMute(networkId, target, false);
            text = removed ? QStringLiteral("No longer muted %1").arg(target)
                           : QStringLiteral("Not muted %1").arg(target);
        }
    }
    m_host.record(IrcStatusEntry::outcome(networkId, text));
    return IrcCommandOutcome::Sent;
}

IrcCommandOutcome IrcCommandDispatcher::dispatchHighlight(const IrcCommand& command,
                                                          IrcComposerSurface surface)
{
    const QString networkId = m_host.queryNetworkId(surface);
    if (networkId.isEmpty()) {
        if (surface == IrcComposerSurface::Conversation && !m_host.selected())
            return IrcCommandOutcome::WrongScope;
        return IrcCommandOutcome::Refused;
    }
    IrcSession *session = m_host.sessionFor(surface);
    if (!session || session->state() == IrcSession::State::Idle
        || session->state() == IrcSession::State::Failed) {
        return IrcCommandOutcome::NotConnected;
    }

    const IrcCaseMapping& mapping =
        m_reducer.serverFeatures(networkId).caseMapping();
    QString text;
    if (command.verb == IrcCommand::Verb::Highlights) {
        if (!command.argument.isEmpty())
            return IrcCommandOutcome::Refused;
        const QStringList words = m_highlights.listed(networkId, mapping);
        text = words.isEmpty()
            ? QStringLiteral("No highlight words")
            : QStringLiteral("Highlights: %1").arg(words.join(QStringLiteral(", ")));
    } else {
        const QString word = firstToken(command.argument);
        if (word.isEmpty() || !restAfterFirstToken(command.argument).isEmpty())
            return IrcCommandOutcome::Refused;
        if (command.verb == IrcCommand::Verb::Highlight) {
            const bool added = m_highlights.add(networkId, word, mapping);
            text = added ? QStringLiteral("Highlighting %1").arg(word)
                         : QStringLiteral("Already highlighting %1").arg(word);
        } else {
            const bool removed = m_highlights.remove(networkId, word, mapping);
            text = removed ? QStringLiteral("No longer highlighting %1").arg(word)
                           : QStringLiteral("Not highlighting %1").arg(word);
        }
        m_host.syncHighlightWords(networkId);
    }
    m_host.record(IrcStatusEntry::outcome(networkId, text));
    return IrcCommandOutcome::Sent;
}

IrcCommandOutcome IrcCommandDispatcher::dispatchHelp(IrcComposerSurface surface)
{
    QString networkId = m_host.queryNetworkId(surface);
    if (networkId.isEmpty())
        networkId = m_host.statusNetworkId();
    if (networkId.isEmpty())
        return IrcCommandOutcome::Refused;

    QStringList names;
    for (const IrcVerbSpec& row : IrcVerbTable::all())
        names.append(QLatin1Char('/') + row.name);
    const QString text =
        QStringLiteral("Commands: %1. Empty /join joins the latest invite.")
            .arg(names.join(QStringLiteral(", ")));
    if (surface == IrcComposerSurface::Conversation) {
        if (!m_host.selected())
            return IrcCommandOutcome::WrongScope;
        m_host.apply(IrcWhoisTranscriptEvent{*m_host.selected(), text});
        return IrcCommandOutcome::Sent;
    }
    m_host.record(IrcStatusEntry::outcome(networkId, text));
    return IrcCommandOutcome::Sent;
}

IrcCommandOutcome IrcCommandDispatcher::echoPrefFeedback(IrcComposerSurface surface,
                                                         const QString& text)
{
    if (surface == IrcComposerSurface::Conversation) {
        if (m_host.selected())
            m_host.apply(IrcWhoisTranscriptEvent{*m_host.selected(), text});
        return IrcCommandOutcome::Sent;
    }
    if (!m_host.statusNetworkId().isEmpty())
        m_host.record(IrcStatusEntry::outcome(m_host.statusNetworkId(), text));
    return IrcCommandOutcome::Sent;
}

IrcCommandOutcome IrcCommandDispatcher::dispatchPref(const IrcCommand& command,
                                                     IrcComposerSurface surface)
{
    const IrcPrefRequest request = ircParsePrefArgument(command.argument);
    if (request.kind == IrcPrefKind::Usage) {
        const IrcVerbSpec *spec = IrcVerbTable::find(IrcCommand::Verb::Pref);
        return echoPrefFeedback(surface, spec ? spec->usage : ircPrefUsage());
    }

    auto enabledFor = [this](IrcPrefName name) {
        return m_host.prefEnabled(name);
    };
    auto applyPref = [this](IrcPrefName name, bool enabled) {
        m_host.prefApply(name, enabled);
    };

    if (request.kind == IrcPrefKind::Set)
        applyPref(request.name, request.enabled);

    if (request.kind == IrcPrefKind::QueryAll) {
        return echoPrefFeedback(
            surface,
            ircFormatPrefList(m_host.prefEnabled(IrcPrefName::Directs),
                              m_host.prefEnabled(IrcPrefName::Avatars),
                              m_host.prefEnabled(IrcPrefName::Unread)));
    }
    if (request.kind == IrcPrefKind::QueryOne) {
        return echoPrefFeedback(
            surface, ircFormatPrefQuery(request.name, enabledFor(request.name)));
    }
    return echoPrefFeedback(
        surface, ircFormatPrefState(request.name, enabledFor(request.name)));
}

IrcCommandOutcome IrcCommandDispatcher::setSelectedTopic(const QString& topic)
{
    if (!m_host.selected() || !m_host.isChannel())
        return IrcCommandOutcome::WrongScope;
    if (topic.isEmpty())
        return IrcCommandOutcome::Sent;
    IrcSession *session = m_host.selectedSession();
    if (!session || session->state() != IrcSession::State::Registered)
        return IrcCommandOutcome::NotConnected;
    return session->setTopic(m_host.selectedTarget(), topic)
        ? IrcCommandOutcome::Sent
        : IrcCommandOutcome::Refused;
}
