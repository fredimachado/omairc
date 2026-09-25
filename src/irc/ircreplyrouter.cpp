#include "ircreplyrouter.h"

#include "ircavatarurl.h"
#include "irceventreducer.h"
#include "ircsession.h"
#include "ircwiretext.h"

#include <QDateTime>

#include <string>
#include <variant>

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

bool isOwnMetadataClearAlias(const QString& argument)
{
    return firstToken(argument).compare(QStringLiteral("clear"), Qt::CaseInsensitive) == 0
        && restAfterFirstToken(argument).isEmpty();
}

QString ownMetadataClearedMessage(const QString& metadataKey)
{
    if (metadataKey.compare(IrcMetadata::statusKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("Standing status cleared.");
    if (metadataKey.compare(IrcMetadata::avatarKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("Avatar cleared.");
    return {};
}

QString ownMetadataSetMessage(const QString& metadataKey, const QString& value)
{
    if (metadataKey.compare(IrcMetadata::statusKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("Standing status set to %1.").arg(value);
    if (metadataKey.compare(IrcMetadata::avatarKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("Avatar set to %1.").arg(value);
    return {};
}

QString ownMetadataClearFailMessage(const QString& metadataKey, const QString& reason)
{
    if (metadataKey.compare(IrcMetadata::statusKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("Could not clear standing status: %1").arg(reason);
    if (metadataKey.compare(IrcMetadata::avatarKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("Could not clear avatar: %1").arg(reason);
    return {};
}

QString ownMetadataSetFailMessage(const QString& metadataKey, const QString& reason)
{
    if (metadataKey.compare(IrcMetadata::statusKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("Could not set standing status: %1").arg(reason);
    if (metadataKey.compare(IrcMetadata::avatarKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("Could not set avatar: %1").arg(reason);
    return {};
}

QString ownMetadataNoCapMessage(const QString& metadataKey)
{
    if (metadataKey.compare(IrcMetadata::statusKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("This network does not support standing status.");
    if (metadataKey.compare(IrcMetadata::avatarKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("This network does not support avatars.");
    return {};
}

QString ownMetadataNoValueMessage(const QString& metadataKey)
{
    if (metadataKey.compare(IrcMetadata::statusKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("This network does not allow standing status text.");
    if (metadataKey.compare(IrcMetadata::avatarKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("This network does not allow avatar URLs.");
    return {};
}

QString ownMetadataInspectEmptyMessage(const QString& metadataKey)
{
    if (metadataKey.compare(IrcMetadata::statusKey(), Qt::CaseInsensitive) == 0) {
        return QStringLiteral(
            "No standing status. Use /status <text> or /status clear.");
    }
    if (metadataKey.compare(IrcMetadata::avatarKey(), Qt::CaseInsensitive) == 0) {
        return QStringLiteral(
            "No standing avatar. Use /avatar <url|email> or /avatar clear.");
    }
    return {};
}

QString ownMetadataInspectValueMessage(const QString& metadataKey,
                                       const QString& value)
{
    if (metadataKey.compare(IrcMetadata::statusKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("Standing status: %1").arg(value);
    if (metadataKey.compare(IrcMetadata::avatarKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("Standing avatar: %1").arg(value);
    return {};
}

bool copiesLabeledStandardReply(const IrcStatusEntry& entry)
{
    const QString label = entry.label();
    if (label == QLatin1String("FAIL")
        || label == QLatin1String("WARN")
        || label == QLatin1String("NOTE")) {
        return true;
    }
    if (label.size() != 3)
        return false;
    if (!label.at(0).isDigit() || !label.at(1).isDigit() || !label.at(2).isDigit())
        return false;
    return label.at(0) == QLatin1Char('4') || label.at(0) == QLatin1Char('5');
}
}

IrcReplyRouter::IrcReplyRouter(IrcEventReducer& reducer, Host host)
    : m_reducer(reducer)
    , m_host(std::move(host))
{
}

void IrcReplyRouter::forget(const QString& networkId)
{
    if (networkId.isEmpty())
        return;
    for (auto it = m_whoisWatches.begin(); it != m_whoisWatches.end(); ) {
        if (it->first.networkId == networkId)
            it = m_whoisWatches.erase(it);
        else
            ++it;
    }
    forgetLabeledWatches(networkId, IrcLabeledWatchKind::Whois);
    for (auto it = m_ctcpWatches.begin(); it != m_ctcpWatches.end(); ) {
        if (it->first.networkId == networkId)
            it = m_ctcpWatches.erase(it);
        else
            ++it;
    }
    forgetLabeledWatches(networkId, IrcLabeledWatchKind::Ctcp);
    m_ownMetadataWatches.remove(networkId);
}

void IrcReplyRouter::noteNickDelivery(const QString& networkId, const QString& target)
{
    if (m_reducer.serverFeatures(networkId).isChannel(utf8(target)))
        return;
    const std::optional<IrcWhoisWatchKey> key = whoisWatchKey(networkId, target);
    if (!key)
        return;
    auto found = m_whoisWatches.find(*key);
    if (found == m_whoisWatches.end())
        return;
    found->second.failedIsAmbiguous = true;
}

void IrcReplyRouter::routeStatusEntry(const IrcStatusEntry& entry)
{
    routeOwnMetadataError(entry);
    if (!entry.requestLabel().isEmpty()) {
        auto found = m_labeledWatches.find(
            IrcLabeledWatchKey{entry.networkId(), entry.requestLabel()});
        if (found != m_labeledWatches.end()) {
            if (found->second.kind == IrcLabeledWatchKind::Whois) {
                if (const IrcWhoisLine *line = entry.whoisLine())
                    routeLabeledWhois(entry.networkId(), entry.requestLabel(), *line);
                else
                    routeLabeledStandardReply(entry);
            } else if (found->second.kind == IrcLabeledWatchKind::Ctcp) {
                if (const IrcCtcpReplyLine *line = entry.ctcpReply())
                    routeLabeledCtcp(entry.networkId(), entry.requestLabel(), *line,
                                     entry.text());
                else
                    routeLabeledStandardReply(entry);
            }
        }
    } else {
        if (const IrcWhoisLine *line = entry.whoisLine())
            routeWhoisLine(entry.networkId(), *line);
        if (const IrcCtcpReplyLine *line = entry.ctcpReply())
            routeCtcpReply(entry.networkId(), *line, entry.text());
    }
}

void IrcReplyRouter::routeOwnMetadataFail(const QString& networkId,
                                          const IrcMessage& message)
{
    if (parameter(message, 0).compare(QLatin1String("METADATA"), Qt::CaseInsensitive)
        != 0) {
        return;
    }
    auto networkWatches = m_ownMetadataWatches.find(networkId);
    if (networkWatches == m_ownMetadataWatches.end())
        return;

    const QString code = parameter(message, 1).toUpper();
    const bool knownFail =
        code == QLatin1String("KEY_NO_PERMISSION")
        || code == QLatin1String("VALUE_INVALID")
        || code == QLatin1String("RATE_LIMITED")
        || code == QLatin1String("KEY_NOT_SET")
        || code == QLatin1String("LIMIT_REACHED")
        || code == QLatin1String("KEY_INVALID");
    if (!knownFail)
        return;

    QStringList failKeys;
    const std::size_t lastContext =
        message.parameters.size() > 2 ? message.parameters.size() - 1 : 2;
    for (std::size_t index = 2; index < lastContext; ++index) {
        const QString token = parameter(message, index);
        if (!IrcMetadata::isKnownKey(token))
            continue;
        const QString canonical = IrcMetadata::canonicalKey(token);
        if (!failKeys.contains(canonical))
            failKeys.append(canonical);
    }

    const QString description = message.parameters.empty()
        ? code
        : parameter(message, message.parameters.size() - 1);
    const QString reason = description.isEmpty()
        ? code
        : QStringLiteral("%1 %2").arg(code, description);

    const auto finishWatch = [&](const QString& canonical,
                                 IrcOwnMetadataWatch watch) {
        if (watch.kind == IrcOwnMetadataWatch::Kind::Clear
            && code == QLatin1String("KEY_NOT_SET")) {
            echoOwnMetadataOutcome(networkId, watch.destination,
                                   ownMetadataClearedMessage(canonical));
            if (canonical == IrcMetadata::avatarKey())
                m_host.persistAvatarUrl(networkId, QString{});
            return;
        }
        const QString outcome = watch.kind == IrcOwnMetadataWatch::Kind::Clear
            ? ownMetadataClearFailMessage(canonical, reason)
            : ownMetadataSetFailMessage(canonical, reason);
        echoOwnMetadataOutcome(networkId, watch.destination, outcome);
    };

    if (failKeys.isEmpty())
        return;

    for (const QString& canonical : failKeys) {
        auto found = networkWatches->find(canonical);
        if (found == networkWatches->end())
            continue;
        const IrcOwnMetadataWatch watch = *found;
        networkWatches->erase(found);
        finishWatch(canonical, watch);
    }
    if (networkWatches->empty())
        m_ownMetadataWatches.erase(networkWatches);
}

void IrcReplyRouter::requestLabelFinished(const QString& networkId,
                                          const QString& requestLabel)
{
    if (networkId.isEmpty() || requestLabel.isEmpty())
        return;
    m_labeledWatches.erase(IrcLabeledWatchKey{networkId, requestLabel});
}

IrcCommandOutcome IrcReplyRouter::dispatchWhois(const IrcCommand& command,
                                                IrcComposerSurface surface)
{
    QString nick = firstToken(command.argument);
    IrcSession *session = nullptr;
    if (nick.isEmpty()) {
        if (m_host.selectedIsCloseableDirect()) {
            nick = m_host.selectedTarget();
            session = m_host.selectedSession();
        } else if (m_host.selected()) {
            return IrcCommandOutcome::WrongScope;
        } else {
            return IrcCommandOutcome::Refused;
        }
    } else {
        const QString networkId = m_host.networkIdFor(surface);
        if (networkId.isEmpty()) {
            if (surface == IrcComposerSurface::Conversation)
                return IrcCommandOutcome::WrongScope;
            return IrcCommandOutcome::Refused;
        }
        session = m_host.sessionFor(surface);
    }
    if (nick.isEmpty())
        return IrcCommandOutcome::Refused;
    if (!session || session->state() != IrcSession::State::Registered)
        return IrcCommandOutcome::NotConnected;
    IrcWhoisDestination destination{IrcWhoisStatusOnly{}};
    if (surface == IrcComposerSurface::Conversation) {
        const std::optional<IrcConversationKey> selected = m_host.selected();
        if (!selected)
            return IrcCommandOutcome::WrongScope;
        destination = *selected;
    }
    return sendWhois(*session, nick, std::move(destination))
        ? IrcCommandOutcome::Sent
        : IrcCommandOutcome::Refused;
}

QString IrcReplyRouter::ctcpQueryName(IrcCommand::Verb verb) const
{
    switch (verb) {
    case IrcCommand::Verb::Ping:
        return QStringLiteral("PING");
    case IrcCommand::Verb::Time:
        return QStringLiteral("TIME");
    case IrcCommand::Verb::Version:
        return QStringLiteral("VERSION");
    default:
        return {};
    }
}

IrcCommandOutcome IrcReplyRouter::dispatchCtcp(const IrcCommand& command,
                                               IrcComposerSurface surface)
{
    const QString query = ctcpQueryName(command.verb);
    QString nick = firstToken(command.argument);
    if (!restAfterFirstToken(command.argument).isEmpty())
        return IrcCommandOutcome::Refused;

    IrcSession *session = nullptr;
    if (nick.isEmpty()) {
        if (m_host.selectedIsCloseableDirect()) {
            nick = m_host.selectedTarget();
            session = m_host.selectedSession();
        } else if (m_host.selected()) {
            return IrcCommandOutcome::WrongScope;
        } else {
            return IrcCommandOutcome::Refused;
        }
    } else {
        const QString networkId = m_host.networkIdFor(surface);
        if (networkId.isEmpty()) {
            if (surface == IrcComposerSurface::Conversation)
                return IrcCommandOutcome::WrongScope;
            return IrcCommandOutcome::Refused;
        }
        if (m_reducer.serverFeatures(networkId).isChannel(utf8(nick)))
            return IrcCommandOutcome::Refused;
        session = m_host.sessionFor(surface);
    }
    if (nick.isEmpty())
        return IrcCommandOutcome::Refused;
    if (!session || session->state() != IrcSession::State::Registered)
        return IrcCommandOutcome::NotConnected;

    IrcCtcpDestination destination{IrcWhoisStatusOnly{}};
    if (surface == IrcComposerSurface::Conversation) {
        const std::optional<IrcConversationKey> selected = m_host.selected();
        if (!selected)
            return IrcCommandOutcome::WrongScope;
        destination = *selected;
    }

    QString argument;
    if (command.verb == IrcCommand::Verb::Ping)
        argument = QString::number(QDateTime::currentMSecsSinceEpoch());
    return sendCtcpQuery(*session, nick, query, argument, std::move(destination))
        ? IrcCommandOutcome::Sent
        : IrcCommandOutcome::Refused;
}

std::optional<IrcReplyRouter::IrcWhoisWatchKey>
IrcReplyRouter::whoisWatchKey(const QString& networkId, const QString& nick) const
{
    const QString trimmed = nick.trimmed();
    if (networkId.isEmpty() || trimmed.isEmpty())
        return std::nullopt;
    return IrcWhoisWatchKey{
        networkId,
        m_reducer.conversationKey(networkId, trimmed).normalizedTarget,
    };
}

bool IrcReplyRouter::sendWhois(IrcSession& session,
                               const QString& nick,
                               IrcWhoisDestination destination)
{
    const std::optional<IrcWhoisWatchKey> key =
        whoisWatchKey(session.networkId(), nick);
    if (!key)
        return false;
    if (const auto *conversation = std::get_if<IrcConversationKey>(&destination)) {
        if (conversation->networkId != session.networkId())
            return false;
    }

    QString requestLabel = session.startLabeledRequest();
    if (!requestLabel.isEmpty()) {
        m_labeledWatches.insert_or_assign(
            IrcLabeledWatchKey{session.networkId(), requestLabel},
            IrcLabeledWatch{IrcLabeledWatchKind::Whois, destination});
        if (!session.whois(nick, requestLabel)) {
            m_labeledWatches.erase(IrcLabeledWatchKey{session.networkId(), requestLabel});
            session.cancelRequestLabel(requestLabel);
            return false;
        }
        return true;
    }
    m_whoisWatches.insert_or_assign(*key, IrcWhoisWatch{std::move(destination)});
    if (!session.whois(nick)) {
        m_whoisWatches.erase(*key);
        return false;
    }
    return true;
}

void IrcReplyRouter::routeWhoisLine(const QString& networkId, const IrcWhoisLine& line)
{
    const std::optional<IrcWhoisWatchKey> key = whoisWatchKey(networkId, line.nick());
    if (!key)
        return;
    auto found = m_whoisWatches.find(*key);
    if (found == m_whoisWatches.end())
        return;

    if (line.progress() == IrcWhoisLine::Progress::Failed
        && found->second.failedIsAmbiguous)
        return;

    const IrcWhoisDestination destination = found->second.destination;
    const IrcConversationKey *conversation =
        std::get_if<IrcConversationKey>(&destination);
    if (conversation) {
        m_host.apply(IrcWhoisTranscriptEvent{
            *conversation,
            line.text(),
        });
    }

    if (line.progress() == IrcWhoisLine::Progress::Detail
        && !found->second.metadataEmitted) {
        found->second.metadataEmitted = true;
        for (const QString& text : whoisMetadataLines(networkId, line.nick())) {
            m_host.record(IrcStatusEntry::lifecycle(
                networkId, IrcLogSeverity::Info, QStringLiteral("whois"), text));
            if (conversation) {
                m_host.apply(IrcWhoisTranscriptEvent{
                    *conversation,
                    text,
                });
            }
        }
    }

    if (line.terminal())
        m_whoisWatches.erase(found);
}

void IrcReplyRouter::routeLabeledWhois(const QString& networkId,
                                       const QString& requestLabel,
                                       const IrcWhoisLine& line)
{
    auto found = m_labeledWatches.find(IrcLabeledWatchKey{networkId, requestLabel});
    if (found == m_labeledWatches.end()
        || found->second.kind != IrcLabeledWatchKind::Whois) {
        return;
    }

    const IrcWhoisDestination destination = found->second.destination;
    const IrcConversationKey *conversation =
        std::get_if<IrcConversationKey>(&destination);
    if (conversation) {
        m_host.apply(IrcWhoisTranscriptEvent{
            *conversation,
            line.text(),
        });
    }

    if (line.progress() == IrcWhoisLine::Progress::Detail
        && !found->second.metadataEmitted) {
        found->second.metadataEmitted = true;
        for (const QString& text : whoisMetadataLines(networkId, line.nick())) {
            m_host.record(IrcStatusEntry::lifecycle(
                networkId, IrcLogSeverity::Info, QStringLiteral("whois"), text));
            if (conversation) {
                m_host.apply(IrcWhoisTranscriptEvent{
                    *conversation,
                    text,
                });
            }
        }
    }

    if (line.terminal())
        m_labeledWatches.erase(found);
}

void IrcReplyRouter::routeLabeledCtcp(const QString& networkId,
                                      const QString& requestLabel,
                                      const IrcCtcpReplyLine& line,
                                      const QString& text)
{
    auto found = m_labeledWatches.find(IrcLabeledWatchKey{networkId, requestLabel});
    if (found == m_labeledWatches.end()
        || found->second.kind != IrcLabeledWatchKind::Ctcp) {
        return;
    }

    const IrcCtcpDestination destination = found->second.destination;
    m_labeledWatches.erase(found);
    if (const std::optional<IrcCtcpWatchKey> key =
            ctcpWatchKey(networkId, line.nick(), line.command())) {
        m_ctcpWatches.erase(*key);
    }

    if (std::holds_alternative<IrcWhoisStatusOnly>(destination) || text.isEmpty())
        return;
    m_host.apply(IrcWhoisTranscriptEvent{
        std::get<IrcConversationKey>(destination),
        text,
    });
}

void IrcReplyRouter::routeLabeledStandardReply(const IrcStatusEntry& entry)
{
    auto found = m_labeledWatches.find(
        IrcLabeledWatchKey{entry.networkId(), entry.requestLabel()});
    if (found == m_labeledWatches.end())
        return;

    if (copiesLabeledStandardReply(entry)) {
        if (const auto *conversation =
                std::get_if<IrcConversationKey>(&found->second.destination)) {
            if (!entry.text().isEmpty()) {
                m_host.apply(IrcWhoisTranscriptEvent{
                    *conversation,
                    entry.text(),
                });
            }
        }
    }

    if (entry.severity() == IrcLogSeverity::Alert)
        m_labeledWatches.erase(found);
}

void IrcReplyRouter::forgetLabeledWatches(const QString& networkId,
                                          IrcLabeledWatchKind kind)
{
    if (networkId.isEmpty())
        return;
    for (auto it = m_labeledWatches.begin(); it != m_labeledWatches.end(); ) {
        if (it->first.networkId == networkId && it->second.kind == kind)
            it = m_labeledWatches.erase(it);
        else
            ++it;
    }
}

QStringList IrcReplyRouter::whoisMetadataLines(const QString& networkId,
                                               const QString& nick) const
{
    const IrcNickPresence facts = m_reducer.nickPresence(networkId, nick);
    QStringList lines;
    const auto addValue = [&](const QString& key, const QString& pattern) {
        const QString value = facts.metadata(key);
        if (value.isEmpty())
            return;
        lines.append(pattern.arg(nick, value));
    };
    addValue(IrcMetadata::displayNameKey(),
             QStringLiteral("%1 is also known as %2"));
    addValue(IrcMetadata::pronounsKey(), QStringLiteral("%1 pronouns %2"));
    addValue(IrcMetadata::statusKey(), QStringLiteral("%1 status %2"));
    if (facts.isBot()) {
        const QString software = facts.metadata(IrcMetadata::botKey());
        if (software.isEmpty())
            lines.append(QStringLiteral("%1 is a bot").arg(nick));
        else
            lines.append(QStringLiteral("%1 is a bot (%2)").arg(nick, software));
    }
    addValue(IrcMetadata::homepageKey(), QStringLiteral("%1 homepage %2"));
    addValue(IrcMetadata::colorKey(), QStringLiteral("%1 color %2"));
    addValue(IrcMetadata::avatarKey(), QStringLiteral("%1 avatar %2"));
    return lines;
}

std::optional<IrcReplyRouter::IrcCtcpWatchKey>
IrcReplyRouter::ctcpWatchKey(const QString& networkId,
                             const QString& nick,
                             const QString& command) const
{
    const QString trimmed = nick.trimmed();
    const QString verb = command.trimmed().toUpper();
    if (networkId.isEmpty() || trimmed.isEmpty() || verb.isEmpty())
        return std::nullopt;
    return IrcCtcpWatchKey{
        networkId,
        m_reducer.conversationKey(networkId, trimmed).normalizedTarget,
        verb,
    };
}

bool IrcReplyRouter::sendCtcpQuery(IrcSession& session,
                                   const QString& nick,
                                   const QString& command,
                                   const QString& argument,
                                   IrcCtcpDestination destination)
{
    const std::optional<IrcCtcpWatchKey> key =
        ctcpWatchKey(session.networkId(), nick, command);
    if (!key)
        return false;
    if (const auto *conversation = std::get_if<IrcConversationKey>(&destination)) {
        if (conversation->networkId != session.networkId())
            return false;
    }

    QString requestLabel = session.startLabeledRequest();
    if (!requestLabel.isEmpty()) {
        m_labeledWatches.insert_or_assign(
            IrcLabeledWatchKey{session.networkId(), requestLabel},
            IrcLabeledWatch{IrcLabeledWatchKind::Ctcp, destination});
    }
    m_ctcpWatches.insert_or_assign(*key, IrcCtcpWatch{std::move(destination)});
    if (!session.sendCtcp(nick, command, argument, requestLabel)) {
        if (!requestLabel.isEmpty()) {
            m_labeledWatches.erase(IrcLabeledWatchKey{session.networkId(), requestLabel});
            session.cancelRequestLabel(requestLabel);
        }
        m_ctcpWatches.erase(*key);
        return false;
    }
    return true;
}

void IrcReplyRouter::routeCtcpReply(const QString& networkId,
                                    const IrcCtcpReplyLine& line,
                                    const QString& text)
{
    const std::optional<IrcCtcpWatchKey> key =
        ctcpWatchKey(networkId, line.nick(), line.command());
    if (!key)
        return;
    auto found = m_ctcpWatches.find(*key);
    if (found == m_ctcpWatches.end())
        return;

    const IrcCtcpDestination destination = found->second.destination;
    m_ctcpWatches.erase(found);

    if (std::holds_alternative<IrcWhoisStatusOnly>(destination) || text.isEmpty())
        return;
    m_host.apply(IrcWhoisTranscriptEvent{
        std::get<IrcConversationKey>(destination),
        text,
    });
}

IrcCommandOutcome IrcReplyRouter::dispatchStatus(const IrcCommand& command,
                                                 IrcComposerSurface surface)
{
    IrcSession *session = m_host.sessionFor(surface);
    if (!session) {
        if (surface == IrcComposerSurface::Conversation && !m_host.selected()
                && m_host.hasNetworks()) {
            return IrcCommandOutcome::Refused;
        }
        return IrcCommandOutcome::NotConnected;
    }
    if (session->state() != IrcSession::State::Registered)
        return IrcCommandOutcome::NotConnected;

    const QString networkId = session->networkId();

    const IrcCapabilitySet capabilities = m_host.capabilities(networkId);
    if (!capabilities.contains(IrcCapability::MemberMetadata)
            || !capabilities.contains(IrcCapability::Batch)) {
        return echoMetadataCommandFeedback(
            surface, networkId, ownMetadataNoCapMessage(IrcMetadata::statusKey()));
    }

    if (command.argument.isEmpty()) {
        const QString current =
            m_reducer.nickPresence(networkId, session->nick()).status();
        if (current.isEmpty()) {
            return echoMetadataCommandFeedback(
                surface, networkId,
                ownMetadataInspectEmptyMessage(IrcMetadata::statusKey()));
        }
        return echoMetadataCommandFeedback(
            surface, networkId,
            ownMetadataInspectValueMessage(IrcMetadata::statusKey(), current));
    }

    if (isOwnMetadataClearAlias(command.argument))
        return dispatchOwnMetadataClear(session, IrcMetadata::statusKey());

    const int valueBudget = IrcMetadata::effectiveMaxValueBytes(
        session->metadataCapability().maxValueBytes);
    if (valueBudget <= 0) {
        return echoMetadataCommandFeedback(
            surface, networkId,
            ownMetadataNoValueMessage(IrcMetadata::statusKey()));
    }
    const QString clamped = IrcMetadata::clamped(command.argument, valueBudget);
    if (clamped.isEmpty())
        return IrcCommandOutcome::Refused;
    return dispatchOwnMetadataSet(session, IrcMetadata::statusKey(), clamped);
}

IrcCommandOutcome IrcReplyRouter::dispatchAvatar(const IrcCommand& command,
                                                 IrcComposerSurface surface)
{
    IrcSession *session = m_host.sessionFor(surface);
    if (!session) {
        if (surface == IrcComposerSurface::Conversation && !m_host.selected()
                && m_host.hasNetworks()) {
            return IrcCommandOutcome::Refused;
        }
        return IrcCommandOutcome::NotConnected;
    }
    if (session->state() != IrcSession::State::Registered)
        return IrcCommandOutcome::NotConnected;

    const QString networkId = session->networkId();

    const IrcCapabilitySet capabilities = m_host.capabilities(networkId);
    if (!capabilities.contains(IrcCapability::MemberMetadata)
            || !capabilities.contains(IrcCapability::Batch)) {
        return echoMetadataCommandFeedback(
            surface, networkId, ownMetadataNoCapMessage(IrcMetadata::avatarKey()));
    }

    if (command.argument.isEmpty()) {
        const QString current =
            m_reducer.nickPresence(networkId, session->nick()).avatar();
        if (current.isEmpty()) {
            return echoMetadataCommandFeedback(
                surface, networkId,
                ownMetadataInspectEmptyMessage(IrcMetadata::avatarKey()));
        }
        return echoMetadataCommandFeedback(
            surface, networkId,
            ownMetadataInspectValueMessage(IrcMetadata::avatarKey(), current));
    }

    if (isOwnMetadataClearAlias(command.argument))
        return dispatchOwnMetadataClear(session, IrcMetadata::avatarKey());

    const QString resolved = ircAvatarMetadataValue(command.argument);
    if (resolved.isEmpty()) {
        return echoMetadataCommandFeedback(
            surface, networkId,
            QStringLiteral("Avatar must be an HTTPS URL or an email address."));
    }

    const int valueBudget = IrcMetadata::effectiveMaxValueBytes(
        session->metadataCapability().maxValueBytes);
    if (valueBudget <= 0) {
        return echoMetadataCommandFeedback(
            surface, networkId,
            ownMetadataNoValueMessage(IrcMetadata::avatarKey()));
    }
    const QString clamped = IrcMetadata::clamped(resolved, valueBudget);
    if (clamped.isEmpty())
        return IrcCommandOutcome::Refused;
    return dispatchOwnMetadataSet(session, IrcMetadata::avatarKey(), clamped);
}

IrcCommandOutcome IrcReplyRouter::dispatchOwnMetadataClear(IrcSession *session,
                                                           const QString& metadataKey)
{
    if (!session->clearOwnMetadata(metadataKey))
        return IrcCommandOutcome::Refused;
    armOwnMetadataWatch(session->networkId(), metadataKey,
                        IrcOwnMetadataWatch::Kind::Clear, QString{});
    return IrcCommandOutcome::Sent;
}

IrcCommandOutcome IrcReplyRouter::dispatchOwnMetadataSet(IrcSession *session,
                                                         const QString& metadataKey,
                                                         const QString& value)
{
    if (value.isEmpty())
        return IrcCommandOutcome::Refused;
    const std::optional<QString> sent = session->setOwnMetadata(metadataKey, value);
    if (!sent || sent->isEmpty())
        return IrcCommandOutcome::Refused;
    armOwnMetadataWatch(session->networkId(), metadataKey,
                        IrcOwnMetadataWatch::Kind::Set, *sent);
    return IrcCommandOutcome::Sent;
}

IrcCommandOutcome IrcReplyRouter::echoMetadataCommandFeedback(
    IrcComposerSurface surface,
    const QString& networkId,
    const QString& text)
{
    const std::optional<IrcConversationKey> selected = m_host.selected();
    if (selected && selected->networkId == networkId) {
        m_host.apply(IrcWhoisTranscriptEvent{*selected, text});
        return IrcCommandOutcome::Sent;
    }
    if (surface == IrcComposerSurface::Conversation)
        return IrcCommandOutcome::WrongScope;
    m_host.record(IrcStatusEntry::outcome(networkId, text));
    return IrcCommandOutcome::Sent;
}

void IrcReplyRouter::armOwnMetadataWatch(const QString& networkId,
                                         const QString& metadataKey,
                                         IrcOwnMetadataWatch::Kind kind,
                                         const QString& value)
{
    IrcWhoisDestination destination{IrcWhoisStatusOnly{}};
    const std::optional<IrcConversationKey> selected = m_host.selected();
    if (selected && selected->networkId == networkId)
        destination = *selected;
    const QString canonical = IrcMetadata::canonicalKey(metadataKey);
    m_ownMetadataWatches[networkId].insert(
        canonical, IrcOwnMetadataWatch{std::move(destination), kind, value});
}

void IrcReplyRouter::echoOwnMetadataOutcome(const QString& networkId,
                                            const IrcWhoisDestination& destination,
                                            const QString& text)
{
    if (const auto *conversation = std::get_if<IrcConversationKey>(&destination)) {
        m_host.apply(IrcWhoisTranscriptEvent{*conversation, text});
        return;
    }
    const std::optional<IrcConversationKey> selected = m_host.selected();
    if (selected && selected->networkId == networkId) {
        m_host.apply(IrcWhoisTranscriptEvent{*selected, text});
        return;
    }
    m_host.record(IrcStatusEntry::outcome(networkId, text));
}

void IrcReplyRouter::routeOwnMetadataReply(const QString& networkId,
                                           const QString& nick,
                                           const QString& key,
                                           const QString& value)
{
    const QString canonical = IrcMetadata::canonicalKey(key);
    auto networkWatches = m_ownMetadataWatches.find(networkId);
    if (networkWatches == m_ownMetadataWatches.end())
        return;
    auto found = networkWatches->find(canonical);
    if (found == networkWatches->end())
        return;
    const QString self = m_host.selfNick(networkId);
    if (self.isEmpty()
        || !m_reducer.serverFeatures(networkId)
                .caseMapping()
                .equals(utf8(nick), utf8(self))) {
        return;
    }
    if (found->kind == IrcOwnMetadataWatch::Kind::Clear) {
        if (!value.isEmpty())
            return;
        echoOwnMetadataOutcome(networkId, found->destination,
                               ownMetadataClearedMessage(canonical));
        if (canonical == IrcMetadata::avatarKey())
            m_host.persistAvatarUrl(networkId, QString{});
    } else {
        if (value.isEmpty() || found->value != value)
            return;
        echoOwnMetadataOutcome(networkId, found->destination,
                               ownMetadataSetMessage(canonical, value));
        if (canonical == IrcMetadata::avatarKey())
            m_host.persistAvatarUrl(networkId, value);
    }
    networkWatches->erase(found);
    if (networkWatches->empty())
        m_ownMetadataWatches.erase(networkWatches);
}

void IrcReplyRouter::routeOwnMetadataError(const IrcStatusEntry& entry)
{
    const QString networkId = entry.networkId();
    auto networkWatches = m_ownMetadataWatches.find(networkId);
    if (networkWatches == m_ownMetadataWatches.end())
        return;
    const QString label = entry.label();
    if (label != QStringLiteral("764") && label != QStringLiteral("767")
        && label != QStringLiteral("769")) {
        return;
    }
    const QString keyToken = firstToken(entry.text());
    if (keyToken.isEmpty())
        return;
    const QString canonical = IrcMetadata::canonicalKey(keyToken);
    auto found = networkWatches->find(canonical);
    if (found == networkWatches->end())
        return;
    const QString reason = entry.text().isEmpty() ? label : entry.text();
    const QString message = found->kind == IrcOwnMetadataWatch::Kind::Clear
        ? ownMetadataClearFailMessage(canonical, reason)
        : ownMetadataSetFailMessage(canonical, reason);
    echoOwnMetadataOutcome(networkId, found->destination, message);
    networkWatches->erase(found);
    if (networkWatches->empty())
        m_ownMetadataWatches.erase(networkWatches);
}
