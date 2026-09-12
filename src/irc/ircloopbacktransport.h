#pragma once

#include "irctransport.h"

#include <QByteArrayList>
#include <QString>

class IrcLoopbackTransport : public IrcTransport
{
    Q_OBJECT

public:
    explicit IrcLoopbackTransport(QObject *parent = nullptr);

    void connectToHost(const QString &host, quint16 port, bool tlsEnabled) override;
    void write(const QByteArray &frame) override;
    void shutdown() override;
    ConnectionState connectionState() const override;

    void completeConnect();
    void failConnect(const QString &message);
    void failTls(const QString &message);
    void timeoutConnect();
    void injectBytes(const QByteArray &bytes);
    void remoteClose();

    QByteArrayList writtenFrames() const;
    QString lastError() const;
    QString connectedHost() const;
    quint16 connectedPort() const;
    bool tlsRequested() const;

signals:
    void frameWritten(const QByteArray &frame);

private:
    bool isOpen() const;
    bool isFinished() const;
    void setState(ConnectionState state);
    void fail(const QString &message);

    ConnectionState m_state = ConnectionState::Idle;
    bool m_tlsEnabled = false;
    QString m_host;
    quint16 m_port = 0;
    QByteArrayList m_written;
    QString m_lastError;
};
