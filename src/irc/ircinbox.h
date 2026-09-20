#pragma once

#include "irccasemapping.h"
#include "ircevent.h"

#include <QDateTime>
#include <QString>
#include <deque>

enum class IrcInboxKind
{
    Mention,
    Highlight,
    Direct,
    Invite,
    MonitorOnline,
    Kick,
};

struct IrcInboxItem
{
    IrcInboxKind kind;
    QDateTime timestamp;
    QString networkId;
    QString actor;
    QString target;
    QString preview;
    IrcMsgId msgid{};
};

class IrcInbox
{
public:
    static constexpr int kMaxItems = 50;

    int count() const noexcept;
    const IrcInboxItem& at(int index) const;
    std::deque<IrcInboxItem> items() const;

    void append(IrcInboxItem item);
    void consumeAt(int index);
    void consumeConversation(const QString& networkId,
                             const QString& target,
                             const IrcCaseMapping& mapping);
    void consumeInvite(const QString& networkId,
                       const QString& channel,
                       const IrcCaseMapping& mapping);
    void consumeMonitor(const QString& networkId,
                        const QString& nick,
                        const IrcCaseMapping& mapping);

private:
    bool targetsMatch(const QString& left,
                      const QString& right,
                      const IrcCaseMapping& mapping) const;
    bool nicksMatch(const QString& left,
                    const QString& right,
                    const IrcCaseMapping& mapping) const;
    void trimToCap();

    std::deque<IrcInboxItem> m_items;
};
