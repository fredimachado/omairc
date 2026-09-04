#pragma once

#include <QString>
#include <QStringList>

#include <map>
#include <optional>

namespace IrcMetadata {
QString statusKey();
QStringList subscribedKeys();

constexpr int maximumValueBytes = 512;
}

struct IrcAway
{
    QString reason;
};

struct IrcNickPresence
{
    std::optional<IrcAway> away;
    QString status;

    bool isDefault() const noexcept;
};

class IrcNetworkPresence
{
public:
    void setAway(const QString& normalizedNick, std::optional<IrcAway> away);
    void setStatus(const QString& normalizedNick, const QString& status);

    void rekey(const QString& fromNormalized, const QString& toNormalized);

    void forget(const QString& normalizedNick);
    void clear() noexcept;
    void clearAway();
    void clearStatus();
    bool knows(const QString& normalizedNick) const noexcept;

    IrcNickPresence lookup(const QString& normalizedNick) const;

private:
    IrcNickPresence& entry(const QString& normalizedNick);
    void eraseIfDefault(const QString& normalizedNick);

    std::map<QString, IrcNickPresence> m_nicks;
};
