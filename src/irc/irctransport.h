#pragma once

#include <QByteArray>
#include <QObject>
#include <QString>

class IrcTransport : public QObject
{
    Q_OBJECT

public:
    enum class ConnectionState {
        Idle,
        Connecting,
        Connected,
        Encrypted,
        Closing,
        Disconnected,
        Failed,
    };
    Q_ENUM(ConnectionState)

    explicit IrcTransport(QObject *parent = nullptr);

    virtual void connectToHost(const QString &host, quint16 port, bool tlsEnabled) = 0;
    virtual void write(const QByteArray &frame) = 0;
    virtual void shutdown() = 0;
    virtual ConnectionState connectionState() const = 0;

signals:
    void connected();
    void encrypted();
    void disconnected();
    void bytesReceived(const QByteArray &bytes);
    void errorOccurred(const QString &message);
};

inline IrcTransport::IrcTransport(QObject *parent)
    : QObject(parent)
{
}
