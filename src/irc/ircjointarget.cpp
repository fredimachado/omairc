#include "ircjointarget.h"

#include <QByteArray>
#include <QRegularExpression>

#include <string>
#include <utility>

namespace
{
std::string utf8(const QString& value)
{
    const QByteArray bytes = value.toUtf8();
    return std::string(bytes.constData(), std::size_t(bytes.size()));
}

bool hasFieldBreakers(const QString& token)
{
    for (const QChar ch : token) {
        if (ch.isSpace() || ch == QLatin1Char(',') || ch == QChar(u'\0'))
            return true;
    }
    return false;
}

bool isSafeKey(const QString& key)
{
    return !key.isEmpty() && key.front() != QLatin1Char(':') && !hasFieldBreakers(key);
}

const QRegularExpression& whitespaceRun()
{
    static const QRegularExpression pattern(QStringLiteral("\\s+"));
    return pattern;
}
}

IrcJoinTarget::IrcJoinTarget(QString channel, std::optional<QString> key)
    : m_channel(std::move(channel))
    , m_key(std::move(key))
{
}

std::optional<IrcJoinTarget> IrcJoinTarget::make(const QString& channel,
                                                 std::optional<QString> key,
                                                 const IrcServerFeatures& features)
{
    QString name = channel.trimmed();
    if (name.isEmpty())
        return std::nullopt;
    if (!features.isChannel(utf8(name)))
        name.prepend(QLatin1Char('#'));
    if (name.size() < 2 || hasFieldBreakers(name))
        return std::nullopt;

    std::optional<QString> secret;
    if (key.has_value()) {
        const QString trimmedKey = key->trimmed();
        if (!isSafeKey(trimmedKey))
            return std::nullopt;
        secret = trimmedKey;
    }
    return IrcJoinTarget(std::move(name), std::move(secret));
}

bool operator==(const IrcJoinTarget& left, const IrcJoinTarget& right) noexcept
{
    return left.m_channel == right.m_channel && left.m_key == right.m_key;
}

bool operator!=(const IrcJoinTarget& left, const IrcJoinTarget& right) noexcept
{
    return !(left == right);
}

std::optional<QVector<IrcJoinTarget>> ircParseJoinTargets(
    const QString& argument, const IrcServerFeatures& features)
{
    QVector<IrcJoinTarget> targets;
    const QStringList entries = argument.split(QLatin1Char(','));
    for (const QString& entry : entries) {
        const QString trimmed = entry.trimmed();
        if (trimmed.isEmpty())
            continue;
        const QStringList tokens = trimmed.split(whitespaceRun(), Qt::SkipEmptyParts);
        if (tokens.isEmpty() || tokens.size() > 2)
            return std::nullopt;
        std::optional<QString> key;
        if (tokens.size() == 2)
            key = tokens.at(1);
        const std::optional<IrcJoinTarget> target =
            IrcJoinTarget::make(tokens.at(0), key, features);
        if (!target)
            return std::nullopt;
        targets.append(*target);
    }
    if (targets.isEmpty())
        return std::nullopt;
    return targets;
}
