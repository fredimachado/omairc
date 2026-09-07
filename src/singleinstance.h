#pragma once

#include <QObject>
#include <QString>

#include <functional>

class QLockFile;
class QLocalServer;
class QLocalSocket;

class SingleInstance final : public QObject {
    Q_OBJECT

public:
    using RequestHandler = std::function<QByteArray(const QByteArray &line)>;

    explicit SingleInstance(QObject *parent = nullptr);
    ~SingleInstance() override;

    bool acquireOrNotify();
    bool isPrimary() const;
    void setRequestHandler(RequestHandler handler);

    static QString socketPath();
    static QString lockPath();

signals:
    void activationRequested();

private:
    QString lockFilePath() const;
    QString serverName() const;
    QString runtimeDir() const;
    bool becomePrimary();
    bool notifyPrimary();
    void listenForActivation();
    void consumeSocketData(QLocalSocket *socket);

    static constexpr int kMaxIpcLineBytes = 64 * 1024;

    QLockFile *m_lock = nullptr;
    QLocalServer *m_server = nullptr;
    bool m_primary = false;
    RequestHandler m_requestHandler;
};
