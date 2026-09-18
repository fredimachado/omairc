#include "irceventtranslator.h"

#include "irchistorybatch.h"
#include "ircprefixnick.h"
#include "ircpresence.h"
#include "ircservicenick.h"
#include "irctcp.h"
#include "irctyping.h"
#include "ircwiretext.h"

#include <QByteArray>
#include <QDateTime>

#include <optional>
#include <string_view>

namespace
{
QString parameter(const IrcMessage& message, std::size_t index)
{
    return index < message.parameters.size() ? ircWireText(message.parameters[index])
                                             : QString{};
}

QString author(const IrcMessage& message)
{
    return ircPrefixNick(message);
}

std::string utf8(const QString& value)
{
    const QByteArray bytes = value.toUtf8();
    return std::string(bytes.constData(), std::size_t(bytes.size()));
}

IrcConversationKey key(const QString& networkId,
                       const QString& target,
                       const IrcServerFeatures& features)
{
    return {networkId, ircWireText(features.caseMapping().normalize(utf8(target)))};
}

bool same(const QString& left,
          const QString& right,
          const IrcServerFeatures& features)
{
    return features.caseMapping().equals(utf8(left), utf8(right));
}

bool hasUserPrefix(const IrcMessage& message)
{
    return message.prefix.has_value()
        && !message.prefix->nick.empty()
        && !message.prefix->user.empty();
}

bool isNetworkNoticeTarget(const QString& target)
{
    return target.isEmpty()
        || target == QLatin1String("*")
        || target.compare(QLatin1String("AUTH"), Qt::CaseInsensitive) == 0;
}

bool isServiceUser(const IrcMessage& message, const IrcServerFeatures& features)
{
    if (!message.prefix)
        return false;
    const std::string_view types = features.channelTypes();
    return ircIsServiceIdentity(ircWireText(message.prefix->nick),
                                ircWireText(message.prefix->host),
                                QString::fromLatin1(types.data(), qsizetype(types.size())));
}

QStringList remainingParameters(const IrcMessage& message, std::size_t start)
{
    QStringList result;
    for (std::size_t index = start; index < message.parameters.size(); ++index)
        result.append(parameter(message, index));
    return result;
}

std::optional<QString> tagValue(const IrcMessage& message, const char *name)
{
    for (const IrcTag& tag : message.tags) {
        if (tag.name == name && tag.value)
            return ircWireText(*tag.value);
    }
    return std::nullopt;
}

QDateTime timestampFor(const IrcMessage& message)
{
    const std::optional<QString> raw = tagValue(message, "time");
    if (!raw)
        return QDateTime::currentDateTimeUtc();
    QDateTime parsed = QDateTime::fromString(*raw, Qt::ISODateWithMs);
    if (!parsed.isValid())
        parsed = QDateTime::fromString(*raw, Qt::ISODate);
    if (!parsed.isValid())
        return QDateTime::currentDateTimeUtc();
    return parsed.toUTC();
}

/// `<Target> <Key> <Visibility> [<Value>]` for `METADATA` and `761`.
void appendMemberMetadata(std::vector<IrcEvent>& events,
                          const QString& networkId,
                          const IrcMessage& message,
                          std::size_t targetIndex,
                          const IrcServerFeatures& features)
{
    if (message.parameters.size() < targetIndex + 3)
        return;
    const QString target = parameter(message, targetIndex);
    const QString key = parameter(message, targetIndex + 1);
    if (target.isEmpty() || features.isChannel(utf8(target)))
        return;
    const QString canonical = IrcMetadata::canonicalKey(key);
    if (canonical.isEmpty())
        return;
    const QString value =
        IrcMetadata::clamped(parameter(message, targetIndex + 3));
    if (value.isEmpty())
        return;
    events.emplace_back(IrcMemberMetadataEvent{networkId, target, canonical, value});
}

/// `766 RPL_KEYNOTSET`: `<Target> <Key> [:reason]`. Clears one known key for
/// GET misses, successful SET unset (demo server), and CLEAR batches. The
/// trailing text is never the key — replies that omit `<Key>` are ignored.
void append766KeyNotSet(std::vector<IrcEvent>& events,
                        const QString& networkId,
                        const IrcMessage& message,
                        const IrcServerFeatures& features)
{
    constexpr std::size_t targetIndex = 1;
    if (message.parameters.size() < targetIndex + 2)
        return;
    const QString target = parameter(message, targetIndex);
    const QString key = parameter(message, targetIndex + 1);
    if (target.isEmpty() || features.isChannel(utf8(target)))
        return;
    const QString canonical = IrcMetadata::canonicalKey(key);
    if (canonical.isEmpty())
        return;
    events.emplace_back(
        IrcMemberMetadataEvent{networkId, target, canonical, QString{}});
}

bool isJoinFailureNumeric(const QString& command)
{
    return command == QStringLiteral("403")
        || command == QStringLiteral("405")
        || command == QStringLiteral("448")
        || command == QStringLiteral("471")
        || command == QStringLiteral("473")
        || command == QStringLiteral("474")
        || command == QStringLiteral("475")
        || command == QStringLiteral("476")
        || command == QStringLiteral("477")
        || command == QStringLiteral("479")
        || command == QStringLiteral("489")
        || command == QStringLiteral("520");
}

bool isAmbiguousJoinFailureNumeric(const QString& command)
{
    return command == QStringLiteral("437")
        || command == QStringLiteral("480")
        || command == QStringLiteral("485");
}
}

std::optional<IrcConversationKey> ircConversationFor(
    const QString& networkId,
    const QString& target,
    const IrcMessage& message,
    const QString& currentNick,
    const IrcServerFeatures& features)
{
    // A bouncer addresses its playback markers to the channel, so the sender is
    // read before the target is classified.
    if (message.prefix && !message.prefix->nick.empty()
        && !ircNickIsRoutable(ircWireText(message.prefix->nick))) {
        return std::nullopt;
    }
    if (features.isChannel(utf8(target)))
        return key(networkId, target, features);
    if (isNetworkNoticeTarget(target) || !hasUserPrefix(message)
        || isServiceUser(message, features))
        return std::nullopt;
    const QString sender = author(message);
    if (sender.isEmpty())
        return std::nullopt;
    if (same(target, currentNick, features))
        return key(networkId, sender, features);
    if (same(sender, currentNick, features))
        return key(networkId, target, features);
    return std::nullopt;
}

std::vector<IrcEvent> IrcEventTranslator::translate(
    const QString& networkId,
    const QString& currentNick,
    const IrcServerFeatures& features,
    const IrcMessage& message)
{
    std::vector<IrcEvent> events;
    const QString command = ircWireText(message.command).toUpper();
    const QString sender = author(message);
    const QDateTime timestamp = timestampFor(message);

    if (command == QStringLiteral("NOTICE"))
        return events;

    if (command == QStringLiteral("PRIVMSG") && message.parameters.size() >= 2) {
        const QString wireTarget = parameter(message, 0);
        const std::optional<IrcConversationKey> conversation = ircConversationFor(
            networkId, wireTarget, message, currentNick, features);
        if (!conversation)
            return events;
        const QString body = parameter(message, 1);
        const auto ctcp = parseCtcpRequest(body);
        if (ctcp && ctcp->command != QStringLiteral("ACTION"))
            return events;
        const QString displayTarget = features.isChannel(utf8(wireTarget))
            ? wireTarget
            : (same(sender, currentNick, features) ? wireTarget : sender);
        const IrcMsgId msgid{tagValue(message, "msgid").value_or(QString{})};
        const QString actionPrefix = QChar(1) + QStringLiteral("ACTION ");
        if (body.startsWith(actionPrefix) && body.endsWith(QChar(1))) {
            events.emplace_back(IrcActionEvent{
                *conversation, sender,
                body.mid(actionPrefix.size(), body.size() - actionPrefix.size() - 1),
                timestamp, displayTarget, msgid});
        } else {
            events.emplace_back(IrcMessageEvent{
                *conversation, sender, body, timestamp, displayTarget, msgid});
        }
    } else if (command == QStringLiteral("TAGMSG") && !message.parameters.empty()) {
        const std::optional<QString> value = tagValue(message, "+typing");
        if (!value || sender.isEmpty())
            return events;
        const std::optional<IrcTypingPhase> phase = ircTypingPhaseFromTag(*value);
        if (!phase)
            return events;
        const QString wireTarget = parameter(message, 0);
        const std::optional<IrcConversationKey> conversation = ircConversationFor(
            networkId, wireTarget, message, currentNick, features);
        if (!conversation)
            return events;
        events.emplace_back(IrcTypingEvent{*conversation, sender, *phase, timestamp});
    } else if (command == QStringLiteral("JOIN") && !message.parameters.empty()) {
        events.emplace_back(IrcJoinEvent{networkId, parameter(message, 0), sender});
    } else if (command == QStringLiteral("PART") && !message.parameters.empty()) {
        events.emplace_back(IrcPartEvent{
            networkId, parameter(message, 0), sender, parameter(message, 1)});
    } else if (command == QStringLiteral("QUIT")) {
        events.emplace_back(IrcQuitEvent{networkId, sender, parameter(message, 0)});
    } else if (command == QStringLiteral("NICK") && !message.parameters.empty()) {
        events.emplace_back(IrcNickEvent{networkId, sender, parameter(message, 0)});
    } else if (command == QStringLiteral("KICK") && message.parameters.size() >= 2) {
        events.emplace_back(IrcKickEvent{
            networkId, parameter(message, 0), parameter(message, 1),
            sender, parameter(message, 2)});
    } else if (command == QStringLiteral("TOPIC") && message.parameters.size() >= 2) {
        events.emplace_back(IrcTopicEvent{
            networkId, parameter(message, 0), parameter(message, 1), sender});
    } else if (command == QStringLiteral("332") && message.parameters.size() >= 3) {
        events.emplace_back(IrcTopicEvent{
            networkId, parameter(message, 1), parameter(message, 2), QString{}});
    } else if (command == QStringLiteral("353") && message.parameters.size() >= 4) {
        std::vector<IrcName> names;
        const QStringList entries =
            parameter(message, 3).split(QLatin1Char(' '), Qt::SkipEmptyParts);
        for (const QString& token : entries) {
            const auto parsed = features.parseNamesToken(utf8(token));
            if (!parsed)
                continue;
            names.push_back({ircWireText(parsed->nick), parsed->ranks});
        }
        events.emplace_back(IrcNamesEvent{
            networkId, parameter(message, 2), std::move(names), false});
    } else if (command == QStringLiteral("366") && message.parameters.size() >= 2) {
        events.emplace_back(IrcNamesEvent{
            networkId, parameter(message, 1), {}, true});
    } else if (command == QStringLiteral("MODE") && message.parameters.size() >= 2) {
        events.emplace_back(IrcModeEvent{
            networkId, parameter(message, 0), sender, parameter(message, 1),
            remainingParameters(message, 2)});
    } else if (command == QStringLiteral("AWAY") && !sender.isEmpty()) {
        events.emplace_back(IrcAwayEvent{
            networkId,
            sender,
            message.parameters.empty()
                ? std::nullopt
                : std::optional<IrcAway>(IrcAway{parameter(message, 0)})});
    } else if (command == QStringLiteral("306")) {
        events.emplace_back(IrcSelfAwayEvent{networkId, true});
    } else if (command == QStringLiteral("305")) {
        events.emplace_back(IrcSelfAwayEvent{networkId, false});
    } else if (command == QStringLiteral("352") && message.parameters.size() >= 7) {
        const QString nick = parameter(message, 5);
        const bool away = parameter(message, 6).startsWith(QLatin1Char('G'));
        if (!nick.isEmpty()) {
            events.emplace_back(IrcAwayEvent{
                networkId,
                nick,
                away ? std::optional<IrcAway>(IrcAway{}) : std::nullopt});
        }
    } else if (command == QStringLiteral("CHGHOST")) {
        // Members store nick plus ranks. User and host are not modeled.
    } else if (command == QStringLiteral("METADATA")) {
        appendMemberMetadata(events, networkId, message, 0, features);
    } else if (command == QStringLiteral("761")) {
        appendMemberMetadata(events, networkId, message, 1, features);
    } else if (command == QStringLiteral("766")) {
        append766KeyNotSet(events, networkId, message, features);
    } else if ((isJoinFailureNumeric(command)
                || isAmbiguousJoinFailureNumeric(command))
               && message.parameters.size() >= 3) {
        const QString channel = parameter(message, 1);
        if (!isAmbiguousJoinFailureNumeric(command)
            || features.isChannel(utf8(channel))) {
            events.emplace_back(IrcChannelErrorEvent{
                networkId, channel, parameter(message, 2)});
        }
    }

    return events;
}

std::optional<IrcHistoryEvent> IrcEventTranslator::translateHistory(
    const QString& networkId,
    const QString& currentNick,
    const IrcServerFeatures& features,
    const IrcHistoryBatch& batch)
{
    if (batch.target.isEmpty())
        return std::nullopt;
    const IrcConversationKey conversation = key(networkId, batch.target, features);
    IrcHistoryEvent event{conversation, batch.target, {}};
    event.lines.reserve(batch.lines.size());
    for (const IrcMessage& line : batch.lines) {
        for (const IrcEvent& translated :
             translate(networkId, currentNick, features, line)) {
            if (const auto *message = std::get_if<IrcMessageEvent>(&translated)) {
                if (message->conversation != conversation)
                    continue;
                event.lines.push_back({message->author, message->body, message->timestamp,
                                       IrcMessageKindTag::Chat, message->msgid});
            } else if (const auto *action = std::get_if<IrcActionEvent>(&translated)) {
                if (action->conversation != conversation)
                    continue;
                event.lines.push_back({action->author, action->body, action->timestamp,
                                       IrcMessageKindTag::Emote, action->msgid});
            }
        }
    }
    return event;
}
