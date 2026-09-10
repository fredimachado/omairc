#include "ircignore.h"

#include "ircwiretext.h"

#include <QByteArray>
#include <QSettings>

#include <string>

namespace
{
const auto ignoresGroup = QStringLiteral("ignores");
const auto nicksKey = QStringLiteral("nicks");

std::string utf8(const QString& value)
{
    const QByteArray bytes = value.toUtf8();
    return std::string(bytes.constData(), std::size_t(bytes.size()));
}

QString prefixNick(const IrcMessage& message)
{
    if (!message.prefix)
        return {};
    return ircWireText(message.prefix->nick);
}

QString parameter(const IrcMessage& message, std::size_t index)
{
    if (index >= message.parameters.size())
        return {};
    return ircWireText(message.parameters[index]);
}

bool nickEquals(const IrcCaseMapping& mapping, const QString& left, const QString& right)
{
    return mapping.equals(utf8(left), utf8(right));
}

int indexOfNick(const QStringList& nicks,
                const QString& nick,
                const IrcCaseMapping& mapping)
{
    for (int i = 0; i < nicks.size(); ++i) {
        if (nickEquals(mapping, nicks.at(i), nick))
            return i;
    }
    return -1;
}

bool listContains(const QStringList& nicks,
                  const QString& nick,
                  const IrcCaseMapping& mapping)
{
    return indexOfNick(nicks, nick, mapping) >= 0;
}
}

QStringList IrcIgnoreStore::load(const QString& networkId) const
{
    if (networkId.isEmpty())
        return {};
    QSettings settings;
    settings.beginGroup(ignoresGroup);
    settings.beginGroup(networkId);
    return settings.value(nicksKey).toStringList();
}

void IrcIgnoreStore::save(const QString& networkId, const QStringList& nicks)
{
    if (networkId.isEmpty())
        return;
    QSettings settings;
    settings.beginGroup(ignoresGroup);
    if (nicks.isEmpty()) {
        settings.remove(networkId);
    } else {
        settings.beginGroup(networkId);
        settings.setValue(nicksKey, nicks);
        settings.endGroup();
    }
    settings.endGroup();
    settings.sync();
}

const QStringList& IrcIgnoreStore::cached(const QString& networkId) const
{
    const auto found = m_cache.constFind(networkId);
    if (found != m_cache.cend())
        return found.value();
    return m_cache.insert(networkId, load(networkId)).value();
}

QStringList IrcIgnoreStore::nicks(const QString& networkId) const
{
    if (networkId.isEmpty())
        return {};
    return cached(networkId);
}

QStringList IrcIgnoreStore::listed(const QString& networkId,
                                   const IrcCaseMapping& mapping) const
{
    QStringList unique;
    for (const QString& nick : nicks(networkId)) {
        if (!listContains(unique, nick, mapping))
            unique.append(nick);
    }
    return unique;
}

bool IrcIgnoreStore::contains(const QString& networkId,
                              const QString& nick,
                              const IrcCaseMapping& mapping) const
{
    if (networkId.isEmpty() || nick.isEmpty())
        return false;
    return listContains(cached(networkId), nick, mapping);
}

bool IrcIgnoreStore::add(const QString& networkId,
                         const QString& nick,
                         const IrcCaseMapping& mapping)
{
    if (networkId.isEmpty() || nick.isEmpty())
        return false;
    QStringList current = cached(networkId);
    if (listContains(current, nick, mapping))
        return false;
    current.append(nick);
    m_cache.insert(networkId, current);
    save(networkId, current);
    return true;
}

bool IrcIgnoreStore::remove(const QString& networkId,
                            const QString& nick,
                            const IrcCaseMapping& mapping)
{
    if (networkId.isEmpty() || nick.isEmpty())
        return false;
    QStringList current = cached(networkId);
    bool changed = false;
    for (int i = current.size() - 1; i >= 0; --i) {
        if (nickEquals(mapping, current.at(i), nick)) {
            current.removeAt(i);
            changed = true;
        }
    }
    if (!changed)
        return false;
    m_cache.insert(networkId, current);
    save(networkId, current);
    return true;
}

void IrcIgnoreStore::forget(const QString& networkId)
{
    if (networkId.isEmpty())
        return;
    m_cache.remove(networkId);
    save(networkId, {});
}

bool ircIgnoreDropsInbound(const IrcMessage& message,
                           const QString& selfNick,
                           const QStringList& nicks,
                           const IrcServerFeatures& features)
{
    const QString sender = prefixNick(message);
    if (sender.isEmpty() || nicks.isEmpty())
        return false;
    const IrcCaseMapping& mapping = features.caseMapping();
    if (!listContains(nicks, sender, mapping))
        return false;

    const std::string& command = message.command;
    if (command == "NOTICE" || command == "INVITE")
        return true;
    if (command != "PRIVMSG" && command != "TAGMSG")
        return false;

    const QString target = parameter(message, 0);
    if (target.isEmpty() || features.isChannel(utf8(target)))
        return false;
    return nickEquals(mapping, target, selfNick);
}
