#pragma once

#include "ircloopbacktransport.h"

#include <QObject>
#include <QPointer>
#include <QString>

#include <memory>

class Backend;
class CredentialStore;
class IrcConnection;
class IrcController;
class IrcDemoServer;
class IrcSlashSession;
class QQmlApplicationEngine;
class QQuickWindow;
class QTemporaryDir;

class SeededIrcFixture : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QObject *backend READ backendObject NOTIFY openedChanged)
    Q_PROPERTY(QObject *irc READ irc NOTIFY openedChanged)
    Q_PROPERTY(QObject *controller READ irc NOTIFY openedChanged)
    Q_PROPERTY(QObject *connection READ connectionObject NOTIFY openedChanged)
    Q_PROPERTY(QObject *slash READ slashObject NOTIFY openedChanged)
    Q_PROPERTY(QObject *omarchyTransport READ omarchyTransportObject NOTIFY openedChanged)
    Q_PROPERTY(QObject *oftcTransport READ oftcTransportObject NOTIFY openedChanged)
    Q_PROPERTY(QObject *window READ windowObject NOTIFY windowChanged)
    Q_PROPERTY(QString lastError READ lastError NOTIFY lastErrorChanged)
    Q_PROPERTY(QString omarchyNetworkId READ omarchyId CONSTANT)
    Q_PROPERTY(QString oftcNetworkId READ oftcId CONSTANT)

public:
    explicit SeededIrcFixture(QObject *parent = nullptr);
    ~SeededIrcFixture() override;

    SeededIrcFixture(const SeededIrcFixture &) = delete;
    SeededIrcFixture &operator=(const SeededIrcFixture &) = delete;

    Q_INVOKABLE bool open();
    Q_INVOKABLE bool createWindow();
    Q_INVOKABLE void injectOmarchy(const QString &bytes);
    Q_INVOKABLE void injectOftc(const QString &bytes);
    Q_INVOKABLE bool echoLastOmarchyPrivmsg();
    Q_INVOKABLE bool echoLastOftcPrivmsg();

    Backend &backend();
    IrcSlashSession &slash();
    IrcController &controller();
    IrcConnection *connection() const;
    IrcLoopbackTransport *omarchyTransport() const;
    IrcLoopbackTransport *oftcTransport() const;
    QQuickWindow *window() const;
    QString lastError() const;

    QObject *backendObject() const;
    QObject *irc() const;
    QObject *connectionObject() const;
    QObject *slashObject() const;
    QObject *omarchyTransportObject() const;
    QObject *oftcTransportObject() const;
    QObject *windowObject() const;

    static QString omarchyNetworkId();
    static QString oftcNetworkId();
    QString omarchyId() const;
    QString oftcId() const;

signals:
    void openedChanged();
    void windowChanged();
    void lastErrorChanged();

private:
    bool fail(const QString &why);
    bool installXdg();

    std::unique_ptr<QTemporaryDir> m_xdg;
    std::unique_ptr<IrcDemoServer> m_demo;
    std::unique_ptr<Backend> m_backend;
    std::unique_ptr<IrcSlashSession> m_slash;
    std::unique_ptr<IrcController> m_controller;
    std::unique_ptr<CredentialStore> m_credentials;
    std::unique_ptr<IrcConnection> m_connection;
    std::unique_ptr<QQmlApplicationEngine> m_engine;
    std::unique_ptr<QObject> m_root;
    QPointer<QQuickWindow> m_window;
    IrcLoopbackTransport *m_omarchyTransport = nullptr;
    IrcLoopbackTransport *m_oftcTransport = nullptr;
    QString m_error;
};
