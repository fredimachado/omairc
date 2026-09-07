#include "singleinstance.h"

#include "omaircipc.h"

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

void SingleInstance::setRequestHandler(RequestHandler handler)
{
    m_requestHandler = std::move(handler);
}

QString SingleInstance::socketPath()
{
    SingleInstance probe;
    return probe.serverName();
}

QString SingleInstance::lockPath()
{
    SingleInstance probe;
    return probe.lockFilePath();
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
    m_server->setSocketOptions(QLocalServer::UserAccessOption);
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
    const QByteArray ping = OmaircIpc::raisePing();
    if (socket.write(ping) != ping.size())
        return false;
    return socket.waitForBytesWritten(1000);
}

void SingleInstance::listenForActivation()
{
    QObject::connect(m_server, &QLocalServer::newConnection, this, [this]() {
        while (QLocalSocket *socket = m_server->nextPendingConnection()) {
            socket->setParent(this);
            socket->setProperty("omaircBuffer", QByteArray());
            QObject::connect(socket, &QLocalSocket::readyRead, this, [this, socket]() {
                consumeSocketData(socket);
            });
            QObject::connect(socket, &QLocalSocket::disconnected, socket,
                             &QObject::deleteLater);
            if (socket->bytesAvailable() > 0)
                consumeSocketData(socket);
        }
    });
}

void SingleInstance::consumeSocketData(QLocalSocket *socket)
{
    QByteArray buffer = socket->property("omaircBuffer").toByteArray();
    buffer += socket->readAll();
    if (buffer.size() > kMaxIpcLineBytes) {
        socket->setProperty("omaircBuffer", QByteArray());
        socket->abort();
        return;
    }

    // Legacy secondary launch writes a bare "!" with no newline.
    if (OmaircIpc::isRaisePing(buffer) && !buffer.contains('\n')) {
        socket->setProperty("omaircBuffer", QByteArray());
        emit activationRequested();
        return;
    }

    while (true) {
        const int newline = buffer.indexOf('\n');
        if (newline < 0)
            break;

        QByteArray line = buffer.left(newline);
        buffer.remove(0, newline + 1);
        if (line.endsWith('\r'))
            line.chop(1);

        if (OmaircIpc::isRaisePing(line)) {
            emit activationRequested();
            if (m_requestHandler) {
                socket->write(OmaircIpc::okResponse() + '\n');
                socket->flush();
            }
            continue;
        }

        if (!m_requestHandler) {
            emit activationRequested();
            continue;
        }

        const QByteArray response = m_requestHandler(line);
        socket->write(response + '\n');
        socket->flush();
    }

    socket->setProperty("omaircBuffer", buffer);
}
