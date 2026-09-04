#pragma once

#include <QString>
#include <QStringList>

#include <map>
#include <optional>

namespace IrcMetadata {
/// Single source of truth for the subscribed key set. IrcSession builds
/// `METADATA * SUB <keys>` from it and IrcEventTranslator matches incoming keys
/// against it, so the subscription and the parser cannot drift.
QString statusKey();
QStringList subscribedKeys();

constexpr int maximumValueBytes = 512;
} // namespace IrcMetadata

/// What we know about a nick, independent of any channel.
struct IrcNickPresence
{
    /// Engaged means away and carries the reason, which may be empty because
    /// `352 G` does not report one. Disengaged means present.
    std::optional<QString> awayMessage;

    /// IRCv3 metadata `status`. Empty means not set. Never a PREFIX mode letter.
    QString status;

    bool isDefault() const noexcept;
};

/// Presence for every known nick on one network, keyed by case-mapped nick.
/// An entry equal to a default IrcNickPresence is erased, so replaying `AWAY`,
/// `761` or `766` converges and an unknown nick needs no special case.
class IrcNetworkPresence
{
public:
    void setAway(const QString& normalizedNick, std::optional<QString> message);
    void setStatus(const QString& normalizedNick, const QString& status);

    /// Carries facts across a nick change, erasing any stale entry under
    /// `toNormalized` first so a recycled nick never inherits a ghost.
    void rename(const QString& fromNormalized, const QString& toNormalized);

    void forget(const QString& normalizedNick);
    void clear() noexcept;
    void clearAway();
    void clearStatus();
    bool knows(const QString& normalizedNick) const noexcept;

    /// Total function: unknown nicks are present with no status.
    IrcNickPresence lookup(const QString& normalizedNick) const;

private:
    IrcNickPresence& entry(const QString& normalizedNick);
    void dropIfDefault(const QString& normalizedNick);

    std::map<QString, IrcNickPresence> m_nicks;
};
