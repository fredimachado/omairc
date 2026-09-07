#include "singleinstance.h"

#include <QDir>
#include <QLocalServer>
#include <QLocalSocket>
#include <QLockFile>
#include <QStandardPaths>

SingleInstance::SingleInstance(QObject *parent)
    : QObject(parent)
{
}

SingleInstance::~SingleInstance()
{
    if (m_server) {
        m_server->close();
        QLocalServer::removeServer(serverName());
    }
    if (m_lock) {
        if (m_primary)
            m_lock->unlock();
        delete m_lock;
        m_lock = nullptr;
    }
}

bool SingleInstance::acquireOrNotify()
{
    if (m_primary)
        return true;
    if (becomePrimary())
        return true;
    if (notifyPrimary())
        return false;

    QLocalServer::removeServer(serverName());
    if (m_lock)
        m_lock->removeStaleLockFile();
    if (becomePrimary())
        return true;
    return false;
}

bool SingleInstance::isPrimary() const
{
    return m_primary;
}

QString SingleInstance::runtimeDir() const
{
    QString dir = QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation);
    if (dir.isEmpty())
        dir = QStandardPaths::writableLocation(QStandardPaths::TempLocation);
    return dir;
}

QString SingleInstance::lockFilePath() const
{
    return QDir(runtimeDir()).filePath(QStringLiteral("omairc.lock"));
}

QString SingleInstance::serverName() const
{
    return QDir(runtimeDir()).filePath(QStringLiteral("omairc.sock"));
}

bool SingleInstance::becomePrimary()
{
    const QString dir = runtimeDir();
    if (dir.isEmpty() || !QDir().mkpath(dir))
        return false;

    if (!m_lock)
        m_lock = new QLockFile(lockFilePath());

    if (!m_lock->tryLock(100))
        return false;

    m_server = new QLocalServer(this);
    QLocalServer::removeServer(serverName());
    if (!m_server->listen(serverName())) {
        m_lock->unlock();
        delete m_server;
        m_server = nullptr;
        return false;
    }

    listenForActivation();
    m_primary = true;
    return true;
}

bool SingleInstance::notifyPrimary()
{
    QLocalSocket socket;
    socket.connectToServer(serverName());
    if (!socket.waitForConnected(1000))
        return false;
    if (socket.write("!", 1) != 1)
        return false;
    return socket.waitForBytesWritten(1000);
}

void SingleInstance::listenForActivation()
{
    QObject::connect(m_server, &QLocalServer::newConnection, this, [this]() {
        while (QLocalSocket *socket = m_server->nextPendingConnection()) {
            socket->setParent(this);
            const auto consumePing = [this, socket]() {
                if (socket->bytesAvailable() <= 0)
                    return;
                socket->readAll();
                emit activationRequested();
            };
            QObject::connect(socket, &QLocalSocket::readyRead, this, consumePing);
            QObject::connect(socket, &QLocalSocket::disconnected, socket, &QObject::deleteLater);
            consumePing();
        }
    });
}
