#include "ircpresence.h"

namespace IrcMetadata {
QString statusKey()
{
    return QStringLiteral("status");
}

QStringList subscribedKeys()
{
    return {statusKey()};
}
} // namespace IrcMetadata

bool IrcNickPresence::isDefault() const noexcept
{
    return !awayMessage.has_value() && status.isEmpty();
}

IrcNickPresence& IrcNetworkPresence::entry(const QString& normalizedNick)
{
    return m_nicks[normalizedNick];
}

void IrcNetworkPresence::dropIfDefault(const QString& normalizedNick)
{
    const auto found = m_nicks.find(normalizedNick);
    if (found != m_nicks.end() && found->second.isDefault())
        m_nicks.erase(found);
}

void IrcNetworkPresence::setAway(const QString& normalizedNick,
                                 std::optional<QString> message)
{
    if (normalizedNick.isEmpty())
        return;
    entry(normalizedNick).awayMessage = std::move(message);
    dropIfDefault(normalizedNick);
}

void IrcNetworkPresence::setStatus(const QString& normalizedNick,
                                   const QString& status)
{
    if (normalizedNick.isEmpty())
        return;
    entry(normalizedNick).status = status;
    dropIfDefault(normalizedNick);
}

void IrcNetworkPresence::rename(const QString& fromNormalized,
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
        entry->second.awayMessage.reset();
        entry = entry->second.isDefault() ? m_nicks.erase(entry) : std::next(entry);
    }
}

void IrcNetworkPresence::clearStatus()
{
    for (auto entry = m_nicks.begin(); entry != m_nicks.end();) {
        entry->second.status.clear();
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
