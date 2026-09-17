#include "ircpresence.h"

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

QStringList subscribedKeys()
{
    return {
        avatarKey(),
        statusKey(),
        botKey(),
        displayNameKey(),
        pronounsKey(),
        homepageKey(),
        colorKey(),
    };
}

bool isKnownKey(const QString& key)
{
    for (const QString& known : subscribedKeys()) {
        if (key.compare(known, Qt::CaseInsensitive) == 0)
            return true;
    }
    return false;
}
}

namespace {
QString storedMetadataKey(const QString& key)
{
    for (const QString& known : IrcMetadata::subscribedKeys()) {
        if (key.compare(known, Qt::CaseInsensitive) == 0)
            return known;
    }
    return {};
}
}

QString IrcNickPresence::metadata(const QString& key) const
{
    const QString stored = storedMetadataKey(key);
    if (stored.isEmpty())
        return {};
    const auto found = keys.find(stored);
    return found == keys.end() ? QString{} : found->second;
}

bool IrcNickPresence::hasKey(const QString& key) const
{
    const QString stored = storedMetadataKey(key);
    return !stored.isEmpty() && keys.find(stored) != keys.end();
}

bool IrcNickPresence::isBot() const
{
    return hasKey(IrcMetadata::botKey());
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
    const QString stored = storedMetadataKey(key);
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

void IrcNetworkPresence::clearStatus()
{
    clearMetadata();
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
