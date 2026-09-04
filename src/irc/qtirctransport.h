#pragma once

#include "irctransport.h"

#include <QByteArrayList>
#include <QSslSocket>

class QtIrcTransport : public IrcTransport
{
public:
    explicit QtIrcTransport(QObject *parent = nullptr);
    explicit QtIrcTransport(const QSslConfiguration &sslConfiguration,
                            QObject *parent = nullptr);

    void connectToHost(const QString &host, quint16 port, bool tlsEnabled) override;
    void write(const QByteArray &frame) override;
    void shutdown() override;
    ConnectionState connectionState() const override;

private:
    bool isOpen() const;
    bool isFinished() const;
    void setState(ConnectionState state);
    void fail(const QString &message);
    void flushPendingWrites();
    void onConnected();
    void onEncrypted();
    void onDisconnected();
    void onReadyRead();
    void onSocketError(QAbstractSocket::SocketError error);
    void onSslErrors(const QList<QSslError> &errors);

    QSslSocket m_socket;
    ConnectionState m_state = ConnectionState::Idle;
    bool m_tlsEnabled = false;
    QByteArrayList m_pendingWrites;
};
