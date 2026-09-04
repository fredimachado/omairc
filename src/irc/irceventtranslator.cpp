#include "irceventtranslator.h"

#include <QByteArray>
#include <QDateTime>

#include <optional>

namespace
{
QString text(const std::string& value)
{
    return QString::fromUtf8(value.data(), qsizetype(value.size()));
}

QString parameter(const IrcMessage& message, std::size_t index)
{
    return index < message.parameters.size() ? text(message.parameters[index]) : QString{};
}

QString author(const IrcMessage& message)
{
    if (!message.prefix)
        return {};
    if (!message.prefix->nick.empty())
        return text(message.prefix->nick);
    const QString raw = text(message.prefix->raw);
    if (raw.contains(QLatin1Char('.')))
        return {};
    return raw;
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
    return {networkId, text(features.caseMapping().normalize(utf8(target)))};
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

std::optional<IrcConversationKey> conversationFor(const QString& networkId,
                                                  const QString& target,
                                                  const IrcMessage& message,
                                                  const QString& currentNick,
                                                  const IrcServerFeatures& features)
{
    if (features.isChannel(utf8(target)))
        return key(networkId, target, features);
    if (isNetworkNoticeTarget(target) || !hasUserPrefix(message)
        || !same(target, currentNick, features)) {
        return std::nullopt;
    }
    const QString sender = author(message);
    if (sender.isEmpty())
        return std::nullopt;
    return key(networkId, sender, features);
}

QStringList remainingParameters(const IrcMessage& message, std::size_t start)
{
    QStringList result;
    for (std::size_t index = start; index < message.parameters.size(); ++index)
        result.append(parameter(message, index));
    return result;
}
}

std::vector<IrcEvent> IrcEventTranslator::translate(
    const QString& networkId,
    const QString& currentNick,
    const IrcServerFeatures& features,
    const IrcMessage& message)
{
    std::vector<IrcEvent> events;
    const QString command = text(message.command).toUpper();
    const QString sender = author(message);
    const QDateTime now = QDateTime::currentDateTimeUtc();

    if ((command == QStringLiteral("PRIVMSG") || command == QStringLiteral("NOTICE"))
        && message.parameters.size() >= 2) {
        const QString wireTarget = parameter(message, 0);
        const std::optional<IrcConversationKey> conversation = conversationFor(
            networkId, wireTarget, message, currentNick, features);
        if (!conversation)
            return events;
        const QString body = parameter(message, 1);
        const QString displayTarget = features.isChannel(utf8(wireTarget))
            ? wireTarget
            : sender;
        if (command == QStringLiteral("PRIVMSG")
            && body.startsWith(QStringLiteral("\x01ACTION "))
            && body.endsWith(QChar(1))) {
            events.emplace_back(IrcActionEvent{
                *conversation, sender, body.mid(8, body.size() - 9), now, displayTarget});
        } else if (command == QStringLiteral("NOTICE")) {
            events.emplace_back(IrcNoticeEvent{
                *conversation, sender, body, now, displayTarget});
        } else {
            events.emplace_back(IrcMessageEvent{
                *conversation, sender, body, now, displayTarget});
        }
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
        for (QString nick : entries) {
            QStringList statuses;
            while (!nick.isEmpty()) {
                const std::string status = features.statusForPrefix(nick.front().toLatin1());
                if (status.empty())
                    break;
                statuses.append(text(status));
                nick.remove(0, 1);
            }
            names.push_back({nick, statuses.join(QLatin1Char(',')), false});
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
    }

    return events;
}
