#pragma once

#include "irccommand.h"
#include "ircnetworklog.h"
#include "networklogmodel.h"

#include <functional>

#include <QObject>
#include <QSet>
#include <QString>

class IrcSession;
class IrcSessionManager;

class IrcStatusConsole : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QAbstractItemModel* lines READ lines CONSTANT)
    Q_PROPERTY(bool open READ isOpen WRITE setOpen NOTIFY openChanged)
    Q_PROPERTY(int alerts READ alerts NOTIFY alertsChanged)
    Q_PROPERTY(QString networkId READ networkId NOTIFY networkChanged)
public:
    using Dispatch = std::function<IrcCommandOutcome(const IrcCommand&)>;

    explicit IrcStatusConsole(IrcSessionManager& sessions,
                              Dispatch dispatch,
                              QObject *parent = nullptr);

    QAbstractItemModel *lines();
    bool isOpen() const;
    int alerts() const;
    QString networkId() const;
    Q_INVOKABLE int alertsFor(const QString &networkId) const;

    void observe(IrcSession *session);
    void forget(const QString& networkId);
    void setNetwork(const QString& networkId);
    void setOpen(bool open);

    Q_INVOKABLE bool submit(const QString& input);
    bool clearLog();
    IrcSession *boundSession() const;

signals:
    void openChanged();
    void alertsChanged();
    void networkChanged();

private:
    void recordLifecycle(IrcSession *session);
    void noteLogChanged(const QString& networkId);

    IrcSessionManager& m_sessions;
    IrcNetworkLog m_log;
    NetworkLogModel m_lines;
    QSet<QString> m_observed;
    QString m_networkId;
    Dispatch m_dispatch;
    bool m_open = false;
};
