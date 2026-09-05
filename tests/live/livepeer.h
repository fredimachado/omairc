#pragma once

#include "ircframer.h"
#include "ircmessage.h"
#include "qtirctransport.h"

#include <QSslConfiguration>
#include <QString>
#include <QStringList>
#include <QVector>

#include <memory>

class RawIrcPeer : public QObject
{
    Q_OBJECT

public:
    RawIrcPeer(const QString &host,
               quint16 port,
               bool tls,
               const QSslConfiguration &ssl,
               const QString &nick,
               QObject *parent = nullptr);
    ~RawIrcPeer() override;

    bool waitRegistered(int timeoutMs = 20000);
    bool join(const QString &channel, int timeoutMs = 10000);
    void writeLine(const QString &line);
    bool waitForCommand(const QString &command, int timeoutMs = 10000);

    QString nick;
    QVector<IrcMessage> incoming;

private:
    void handleBytes(const QByteArray &bytes);
    void answerPing(const IrcMessage &message);
    bool waitForCap(const QString &verb, int timeoutMs);
    bool capHas(const QString &verb) const;
    QStringList advertisedCaps() const;

    std::unique_ptr<QtIrcTransport> m_transport;
    IrcFramer m_framer;
};
