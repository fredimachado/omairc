#include "singleinstance.h"

#include "omaircipc.h"

#include <QDir>
#include <QLocalServer>
#include <QLocalSocket>
#include <QLockFile>
#include <QStandardPaths>
#include <QTimer>

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
    listenForActivation();
    QLocalServer::removeServer(serverName());
    if (!m_server->listen(serverName())) {
        m_lock->unlock();
        delete m_server;
        m_server = nullptr;
        return false;
    }

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
            if (m_activeClients >= kMaxIpcClients) {
                rejectSocket(socket);
                continue;
            }
            ++m_activeClients;
            socket->setParent(this);
            socket->setProperty("omaircBuffer", QByteArray());
            socket->setProperty("omaircHandled", false);
            auto *idleTimer = new QTimer(socket);
            idleTimer->setSingleShot(true);
            idleTimer->setInterval(kIpcIdleTimeoutMs);
            socket->setProperty("omaircIdleTimer", QVariant::fromValue(idleTimer));
            QObject::connect(idleTimer, &QTimer::timeout, socket, [this, socket]() {
                expireSocket(socket);
            });
            QObject::connect(socket, &QLocalSocket::readyRead, this, [this, socket]() {
                consumeSocketData(socket);
            });
            QObject::connect(socket, &QLocalSocket::disconnected, this, [this, socket]() {
                --m_activeClients;
                socket->deleteLater();
            });
            idleTimer->start();
            if (socket->bytesAvailable() > 0)
                consumeSocketData(socket);
        }
    });
}

void SingleInstance::consumeSocketData(QLocalSocket *socket)
{
    if (socket->property("omaircHandled").toBool())
        return;

    QByteArray buffer = socket->property("omaircBuffer").toByteArray();
    buffer += socket->readAll();
    if (buffer.size() > kMaxIpcInputBytes) {
        rejectSocket(socket);
        return;
    }

    if (buffer == OmaircIpc::raisePing() && socket->property("omaircPingReady").toBool()) {
        socket->setProperty("omaircBuffer", QByteArray());
        emit activationRequested();
        socket->setProperty("omaircHandled", true);
        socket->disconnectFromServer();
        return;
    }
    socket->setProperty("omaircPingReady", buffer == OmaircIpc::raisePing());

    const int newline = buffer.indexOf('\n');
    if (newline < 0) {
        socket->setProperty("omaircBuffer", buffer);
        return;
    }
    if (newline > kMaxIpcLineBytes) {
        rejectSocket(socket);
        return;
    }

    QByteArray line = buffer.left(newline);
    if (line.endsWith('\r'))
        line.chop(1);

    if (OmaircIpc::isRaisePing(line)) {
        emit activationRequested();
        if (m_requestHandler)
            finishSocket(socket, OmaircIpc::okResponse() + '\n');
        else {
            socket->setProperty("omaircHandled", true);
            socket->disconnectFromServer();
        }
        return;
    }

    if (!m_requestHandler) {
        emit activationRequested();
        finishSocket(socket, OmaircIpc::errorResponse(
                                      QStringLiteral("Omairc is starting")) + '\n');
        return;
    }

    const QByteArray response = m_requestHandler(line);
    if (response.size() + 1 > kMaxIpcResponseBytes) {
        rejectSocket(socket);
        return;
    }
    finishSocket(socket, response + '\n');
}

void SingleInstance::finishSocket(QLocalSocket *socket, const QByteArray &response)
{
    socket->setProperty("omaircHandled", true);
    auto *idleTimer = socket->property("omaircIdleTimer").value<QTimer *>();
    if (idleTimer)
        idleTimer->stop();
    socket->write(response);
    socket->flush();
    socket->disconnectFromServer();
}

void SingleInstance::rejectSocket(QLocalSocket *socket)
{
    socket->setProperty("omaircBuffer", QByteArray());
    socket->setProperty("omaircHandled", true);
    socket->abort();
}

void SingleInstance::expireSocket(QLocalSocket *socket)
{
    const QByteArray buffer = socket->property("omaircBuffer").toByteArray();
    if (buffer == OmaircIpc::raisePing()) {
        emit activationRequested();
        socket->setProperty("omaircHandled", true);
        socket->abort();
        return;
    }
    rejectSocket(socket);
}
