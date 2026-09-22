#pragma once

#include <QByteArray>
#include <QHash>
#include <QList>
#include <QObject>
#include <QString>
#include <QStringList>

#include "ircnetworkprofile.h"

class IrcController;
class IrcLoopbackTransport;

class IrcDemoServer : public QObject
{
    Q_OBJECT

public:
    explicit IrcDemoServer(QObject *parent = nullptr);
    ~IrcDemoServer() override;

    IrcDemoServer(const IrcDemoServer &) = delete;
    IrcDemoServer &operator=(const IrcDemoServer &) = delete;

    static QString omarchyNetworkId();
    static QString oftcNetworkId();

    static QList<IrcNetworkProfile> seedProfiles();
    bool writeProfiles();
    bool attach(IrcController &controller, bool autoEcho = false);

    IrcLoopbackTransport *omarchyTransport() const;
    IrcLoopbackTransport *oftcTransport() const;

    void injectOmarchy(const QByteArray &bytes);
    void injectOftc(const QByteArray &bytes);
    bool echoLastOmarchyPrivmsg(const QString &nick);
    bool echoLastOftcPrivmsg(const QString &nick);

    QString lastError() const;

private:
    bool fail(const QString &why);
    bool startNetwork(IrcController &controller,
                      const QString &networkId,
                      const QString &nick,
                      const QStringList &autojoin,
                      IrcLoopbackTransport **transport);
    void hookAutoEcho(IrcLoopbackTransport *transport,
                      const QString &nick,
                      const QStringList &online);

    IrcLoopbackTransport *m_omarchyTransport = nullptr;
    IrcLoopbackTransport *m_oftcTransport = nullptr;
    QHash<IrcLoopbackTransport *, QStringList> m_monitorLists;
    QString m_error;
};
