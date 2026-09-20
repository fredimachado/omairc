#include "ircinbox.h"

#include <QByteArray>

#include <string>

namespace
{
std::string utf8(const QString& value)
{
    const QByteArray bytes = value.toUtf8();
    return std::string(bytes.constData(), std::size_t(bytes.size()));
}

bool isConversationKind(IrcInboxKind kind)
{
    switch (kind) {
    case IrcInboxKind::Mention:
    case IrcInboxKind::Highlight:
    case IrcInboxKind::Direct:
    case IrcInboxKind::Kick:
        return true;
    case IrcInboxKind::Invite:
    case IrcInboxKind::MonitorOnline:
        return false;
    }
    return false;
}
}

int IrcInbox::count() const noexcept
{
    return int(m_items.size());
}

const IrcInboxItem& IrcInbox::at(int index) const
{
    return m_items.at(std::size_t(index));
}

std::deque<IrcInboxItem> IrcInbox::items() const
{
    return m_items;
}

void IrcInbox::append(IrcInboxItem item)
{
    if (item.kind == IrcInboxKind::Invite) {
        for (auto it = m_items.begin(); it != m_items.end(); ++it) {
            if (it->kind == IrcInboxKind::Invite
                && it->networkId == item.networkId
                && it->target == item.target) {
                m_items.erase(it);
                break;
            }
        }
    } else if (item.kind == IrcInboxKind::MonitorOnline) {
        for (auto it = m_items.begin(); it != m_items.end(); ++it) {
            if (it->kind == IrcInboxKind::MonitorOnline
                && it->networkId == item.networkId
                && it->actor == item.actor) {
                m_items.erase(it);
                break;
            }
        }
    }

    m_items.push_front(std::move(item));
    trimToCap();
}

void IrcInbox::consumeAt(int index)
{
    if (index < 0 || index >= count())
        return;
    m_items.erase(m_items.begin() + index);
}

void IrcInbox::consumeConversation(const QString& networkId,
                                   const QString& target,
                                   const IrcCaseMapping& mapping)
{
    for (auto it = m_items.begin(); it != m_items.end(); ) {
        if (it->networkId == networkId && isConversationKind(it->kind)
            && targetsMatch(it->target, target, mapping)) {
            it = m_items.erase(it);
        } else {
            ++it;
        }
    }
}

void IrcInbox::consumeInvite(const QString& networkId,
                             const QString& channel,
                             const IrcCaseMapping& mapping)
{
    for (auto it = m_items.begin(); it != m_items.end(); ) {
        if (it->kind == IrcInboxKind::Invite && it->networkId == networkId
            && targetsMatch(it->target, channel, mapping)) {
            it = m_items.erase(it);
        } else {
            ++it;
        }
    }
}

void IrcInbox::consumeMonitor(const QString& networkId,
                              const QString& nick,
                              const IrcCaseMapping& mapping)
{
    for (auto it = m_items.begin(); it != m_items.end(); ) {
        if (it->kind == IrcInboxKind::MonitorOnline && it->networkId == networkId
            && nicksMatch(it->actor, nick, mapping)) {
            it = m_items.erase(it);
        } else {
            ++it;
        }
    }
}

bool IrcInbox::targetsMatch(const QString& left,
                            const QString& right,
                            const IrcCaseMapping& mapping) const
{
    return mapping.equals(utf8(left), utf8(right));
}

bool IrcInbox::nicksMatch(const QString& left,
                          const QString& right,
                          const IrcCaseMapping& mapping) const
{
    return mapping.equals(utf8(left), utf8(right));
}

void IrcInbox::trimToCap()
{
    while (m_items.size() > kMaxItems)
        m_items.pop_back();
}
