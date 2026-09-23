#include "ircchannellistrequest.h"

#include <algorithm>

IrcChannelListRequest::IrcChannelListRequest(QObject *parent) : QObject(parent) {}

const IrcChannelListRequest::State *IrcChannelListRequest::state(const QString& id) const
{
    const auto it = m_states.constFind(id);
    return it == m_states.cend() ? nullptr : &it.value();
}

bool IrcChannelListRequest::sameMask(const QString& a, const QString& b)
{
    return QString::compare(a.trimmed(), b.trimmed(), Qt::CaseInsensitive) == 0;
}

IrcChannelListRequest::Request IrcChannelListRequest::request(
    const QString& id, const QString& mask, bool refresh)
{
    State *existing = m_states.contains(id) ? &m_states[id] : nullptr;
    if (existing && existing->loading) {
        if (sameMask(existing->mask, mask))
            existing->pendingMask.reset();
        else
            existing->pendingMask = mask;
        return Request::Loading;
    }
    if (existing && !refresh && existing->complete && sameMask(existing->mask, mask))
        return Request::Cached;
    State &value = m_states[id];
    const bool drain = value.drainTimedOutEnd;
    value = State{};
    value.mask = mask;
    value.loading = true;
    value.drainTimedOutEnd = drain;
    arm(id);
    return Request::Start;
}

void IrcChannelListRequest::cancelStart(const QString& id) { forget(id); }

void IrcChannelListRequest::row(const QString& id, IrcChannelListRow value)
{
    if (value.channel.isEmpty()) return;
    auto it = m_states.find(id);
    if (it == m_states.end() || !it->loading) return;
    arm(id);
    bool replaced = false;
    for (auto &old : it->rows) {
        if (old.channel.compare(value.channel, Qt::CaseInsensitive) == 0) {
            old = value; replaced = true; break;
        }
    }
    if (!replaced) {
        if (it->rows.size() >= ChannelListModel::kMaxRows) return;
        it->rows.append(value);
    }
    emit rowChanged(id, value, replaced);
}

void IrcChannelListRequest::activity(const QString& id)
{
    auto it = m_states.constFind(id);
    if (it != m_states.cend() && it->loading) arm(id);
}

std::optional<QString> IrcChannelListRequest::finish(const QString& id)
{
    auto it = m_states.find(id);
    if (it == m_states.end()) return {};
    if (!it->loading) { it->drainTimedOutEnd = false; return {}; }
    if (it->drainTimedOutEnd) {
        it->drainTimedOutEnd = false; arm(id); return {};
    }
    stop(id);
    const auto pending = it->pendingMask;
    it->pendingMask.reset(); it->loading = false; it->complete = true; it->error.clear();
    if (pending && !sameMask(*pending, it->mask)) return pending;
    return {};
}

bool IrcChannelListRequest::fail(const QString& id, const QString& text, bool timeout)
{
    auto it = m_states.find(id);
    if (it == m_states.end() || !it->loading) return false;
    stop(id); it->loading = false; it->complete = false; it->pendingMask.reset();
    it->error = text;
    if (timeout) it->drainTimedOutEnd = true;
    return true;
}

void IrcChannelListRequest::forget(const QString& id) { stop(id); m_states.remove(id); }
void IrcChannelListRequest::setIdleTimeoutMs(int ms) { m_idleTimeoutMs = std::max(1, ms); }

void IrcChannelListRequest::fireIdleTimeout(const QString& id)
{
    const QString text = QStringLiteral("Channel list timed out. Try /list again.");
    if (fail(id, text, true)) emit timedOut(id, text);
}

void IrcChannelListRequest::arm(const QString& id)
{
    QTimer *&timer = m_timers[id];
    if (!timer) {
        timer = new QTimer(this); timer->setSingleShot(true);
        connect(timer, &QTimer::timeout, this, [this, id] { fireIdleTimeout(id); });
    }
    timer->start(m_idleTimeoutMs);
}

void IrcChannelListRequest::stop(const QString& id)
{
    QTimer *timer = m_timers.take(id);
    if (!timer) return;
    timer->stop(); timer->deleteLater();
}
