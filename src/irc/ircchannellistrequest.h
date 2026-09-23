#pragma once

#include <QHash>
#include <QObject>
#include <QTimer>
#include <QVector>
#include <optional>

#include "channellistmodel.h"

class IrcChannelListRequest : public QObject
{
    Q_OBJECT
public:
    struct State {
        QString mask;
        QVector<IrcChannelListRow> rows;
        bool complete = false;
        bool loading = false;
        QString error;
        std::optional<QString> pendingMask;
        bool drainTimedOutEnd = false;
    };
    enum class Request { Start, Loading, Cached };
    struct FinishResult {
        bool completed = false;
        std::optional<QString> pendingMask;
    };

    explicit IrcChannelListRequest(QObject *parent = nullptr);
    const State *state(const QString& networkId) const;
    Request request(const QString& networkId, const QString& mask, bool refresh);
    void cancelStart(const QString& networkId);
    void row(const QString& networkId, IrcChannelListRow row);
    void activity(const QString& networkId);
    FinishResult finish(const QString& networkId);
    bool fail(const QString& networkId, const QString& text, bool timedOut = false);
    void forget(const QString& networkId);
    void setIdleTimeoutMs(int milliseconds);
    void fireIdleTimeout(const QString& networkId);
    static bool sameMask(const QString& left, const QString& right);

signals:
    void rowChanged(const QString& networkId, IrcChannelListRow row, bool replaced);
    void timedOut(const QString& networkId, const QString& text);

private:
    void arm(const QString& networkId);
    void stop(const QString& networkId);
    QHash<QString, State> m_states;
    QHash<QString, QTimer *> m_timers;
    int m_idleTimeoutMs = 30000;
};
