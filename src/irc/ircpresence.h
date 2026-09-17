#pragma once

#include <QString>
#include <QStringList>

#include <map>
#include <optional>

struct IrcMetadataCapability
{
    std::optional<int> maxSubs;
    std::optional<int> maxValueBytes;
};

namespace IrcMetadata {
QString statusKey();
QString avatarKey();
QString botKey();
QString displayNameKey();
QString pronounsKey();
QString homepageKey();
QString colorKey();
const QStringList& subscribedKeys();
QStringList subscriptionKeys(std::optional<int> maxSubs);
bool isKnownKey(const QString& key);
QString canonicalKey(const QString& key);
QString clamped(const QString& value);
QString clamped(const QString& value, int maxBytes);
int effectiveMaxValueBytes(std::optional<int> advertised);

constexpr int maximumValueBytes = 512;
}

std::optional<IrcMetadataCapability> parseIrcMetadataCapability(
    const QStringList& tokens);

struct IrcAway
{
    QString reason;
};

struct IrcNickPresence
{
    std::optional<IrcAway> away;
    std::map<QString, QString> keys;

    QString metadata(const QString& key) const;
    bool hasKey(const QString& key) const;
    bool isBot() const;
    QString status() const;
    QString avatar() const;
    bool isDefault() const noexcept;
};

class IrcNetworkPresence
{
public:
    void setAway(const QString& normalizedNick, std::optional<IrcAway> away);
    void setMetadata(const QString& normalizedNick,
                     const QString& key,
                     const QString& value);

    void rekey(const QString& fromNormalized, const QString& toNormalized);

    void forget(const QString& normalizedNick);
    void clear() noexcept;
    void clearAway();
    void clearMetadata();
    bool knows(const QString& normalizedNick) const noexcept;

    IrcNickPresence lookup(const QString& normalizedNick) const;

private:
    IrcNickPresence& entry(const QString& normalizedNick);
    void eraseIfDefault(const QString& normalizedNick);

    std::map<QString, IrcNickPresence> m_nicks;
};
