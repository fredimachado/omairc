#include "ircpresence.h"

namespace {
QString tokenName(const QString& token)
{
    return token.section(QLatin1Char('='), 0, 0);
}

QString tokenValue(const QString& token)
{
    return token.contains(QLatin1Char('='))
        ? token.section(QLatin1Char('='), 1)
        : QString{};
}

std::optional<int> parseNonNegativeInt(const QString& raw)
{
    if (raw.isEmpty())
        return std::nullopt;
    bool ok = false;
    const int value = raw.toInt(&ok);
    if (!ok || value < 0)
        return std::nullopt;
    return value;
}
}

namespace IrcMetadata {
QString statusKey()
{
    return QStringLiteral("status");
}

QString avatarKey()
{
    return QStringLiteral("avatar");
}

QString botKey()
{
    return QStringLiteral("bot");
}

QString displayNameKey()
{
    return QStringLiteral("display-name");
}

QString pronounsKey()
{
    return QStringLiteral("pronouns");
}

QString homepageKey()
{
    return QStringLiteral("homepage");
}

QString colorKey()
{
    return QStringLiteral("color");
}

const QStringList& subscribedKeys()
{
    // Subscription priority: status first so /status works under max-subs.
    static const QStringList keys = {
        statusKey(),
        avatarKey(),
        botKey(),
        displayNameKey(),
        pronounsKey(),
        homepageKey(),
        colorKey(),
    };
    return keys;
}

QStringList subscriptionKeys(std::optional<int> maxSubs)
{
    const QStringList& all = subscribedKeys();
    if (!maxSubs.has_value())
        return all;
    if (*maxSubs <= 0)
        return {};
    return all.mid(0, qMin(*maxSubs, int(all.size())));
}

QString canonicalKey(const QString& key)
{
    for (const QString& known : subscribedKeys()) {
        if (key.compare(known, Qt::CaseInsensitive) == 0)
            return known;
    }
    return {};
}

bool isKnownKey(const QString& key)
{
    return !canonicalKey(key).isEmpty();
}

int effectiveMaxValueBytes(std::optional<int> advertised)
{
    // Absent/malformed → client default. Explicit 0 means no non-empty values.
    if (!advertised.has_value())
        return maximumValueBytes;
    return qMin(*advertised, maximumValueBytes);
}

QString clamped(const QString& value)
{
    return clamped(value, maximumValueBytes);
}

QString clamped(const QString& value, int maxBytes)
{
    if (maxBytes <= 0)
        return {};
    QString result = value;
    while (result.toUtf8().size() > maxBytes)
        result.chop(1);
    return result;
}
}

std::optional<IrcMetadataCapability> parseIrcMetadataCapability(
    const QStringList& tokens)
{
    std::optional<IrcMetadataCapability> result;
    for (const QString& token : tokens) {
        if (tokenName(token).compare(QLatin1String("draft/metadata-2"),
                                     Qt::CaseInsensitive)
            != 0) {
            continue;
        }
        result = IrcMetadataCapability{};
        const QString raw = tokenValue(token);
        if (raw.isEmpty())
            continue;
        const QStringList parts = raw.split(QLatin1Char(','), Qt::SkipEmptyParts);
        for (const QString& part : parts) {
            const QString key = tokenName(part).toCaseFolded();
            const std::optional<int> value = parseNonNegativeInt(tokenValue(part));
            if (!value)
                continue;
            if (key == QLatin1String("max-subs")) {
                if (!result->maxSubs)
                    result->maxSubs = *value;
                continue;
            }
            if (key == QLatin1String("max-value-bytes")) {
                if (!result->maxValueBytes)
                    result->maxValueBytes = *value;
            }
        }
    }
    return result;
}

QString IrcNickPresence::metadata(const QString& key) const
{
    const QString stored = IrcMetadata::canonicalKey(key);
    if (stored.isEmpty())
        return {};
    const auto found = keys.find(stored);
    return found == keys.end() ? QString{} : found->second;
}

bool IrcNickPresence::hasKey(const QString& key) const
{
    const QString stored = IrcMetadata::canonicalKey(key);
    return !stored.isEmpty() && keys.find(stored) != keys.end();
}

bool IrcNickPresence::isBot() const
{
    // IRCv3 registry: setting `bot` marks the client; the value names the software.
    const QString software = metadata(IrcMetadata::botKey());
    if (software.isEmpty())
        return false;
    const QString trimmed = software.trimmed();
    return trimmed.compare(QStringLiteral("0"), Qt::CaseInsensitive) != 0
        && trimmed.compare(QStringLiteral("false"), Qt::CaseInsensitive) != 0
        && trimmed.compare(QStringLiteral("no"), Qt::CaseInsensitive) != 0;
}

QString IrcNickPresence::status() const
{
    return metadata(IrcMetadata::statusKey());
}

QString IrcNickPresence::avatar() const
{
    return metadata(IrcMetadata::avatarKey());
}

bool IrcNickPresence::isDefault() const noexcept
{
    return !away.has_value() && keys.empty();
}

IrcNickPresence& IrcNetworkPresence::entry(const QString& normalizedNick)
{
    return m_nicks[normalizedNick];
}

void IrcNetworkPresence::eraseIfDefault(const QString& normalizedNick)
{
    const auto found = m_nicks.find(normalizedNick);
    if (found != m_nicks.end() && found->second.isDefault())
        m_nicks.erase(found);
}

void IrcNetworkPresence::setAway(const QString& normalizedNick,
                                 std::optional<IrcAway> away)
{
    if (normalizedNick.isEmpty())
        return;
    entry(normalizedNick).away = std::move(away);
    eraseIfDefault(normalizedNick);
}

void IrcNetworkPresence::setMetadata(const QString& normalizedNick,
                                     const QString& key,
                                     const QString& value)
{
    if (normalizedNick.isEmpty())
        return;
    const QString stored = IrcMetadata::canonicalKey(key);
    if (stored.isEmpty())
        return;
    if (value.isEmpty()) {
        const auto found = m_nicks.find(normalizedNick);
        if (found == m_nicks.end())
            return;
        found->second.keys.erase(stored);
        eraseIfDefault(normalizedNick);
        return;
    }
    entry(normalizedNick).keys[stored] = value;
    eraseIfDefault(normalizedNick);
}

void IrcNetworkPresence::rekey(const QString& fromNormalized,
                               const QString& toNormalized)
{
    if (fromNormalized == toNormalized)
        return;
    m_nicks.erase(toNormalized);
    const auto found = m_nicks.find(fromNormalized);
    if (found == m_nicks.end())
        return;
    IrcNickPresence moved = std::move(found->second);
    m_nicks.erase(found);
    m_nicks.emplace(toNormalized, std::move(moved));
}

void IrcNetworkPresence::forget(const QString& normalizedNick)
{
    m_nicks.erase(normalizedNick);
}

void IrcNetworkPresence::clear() noexcept
{
    m_nicks.clear();
}

void IrcNetworkPresence::clearAway()
{
    for (auto entry = m_nicks.begin(); entry != m_nicks.end();) {
        entry->second.away.reset();
        entry = entry->second.isDefault() ? m_nicks.erase(entry) : std::next(entry);
    }
}

void IrcNetworkPresence::clearMetadata()
{
    for (auto entry = m_nicks.begin(); entry != m_nicks.end();) {
        entry->second.keys.clear();
        entry = entry->second.isDefault() ? m_nicks.erase(entry) : std::next(entry);
    }
}

bool IrcNetworkPresence::knows(const QString& normalizedNick) const noexcept
{
    return m_nicks.count(normalizedNick) != 0;
}

IrcNickPresence IrcNetworkPresence::lookup(const QString& normalizedNick) const
{
    const auto found = m_nicks.find(normalizedNick);
    return found == m_nicks.end() ? IrcNickPresence{} : found->second;
}
