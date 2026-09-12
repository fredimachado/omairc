#pragma once

#include <QPointer>
#include <QString>

#include <memory>

class Backend;
class FakeIrcTransport;
class IrcController;
class IrcSlashSession;
class QObject;
class QQmlApplicationEngine;
class QQuickWindow;
class QTemporaryDir;

class SeededIrcFixture
{
public:
    SeededIrcFixture();
    ~SeededIrcFixture();

    SeededIrcFixture(const SeededIrcFixture &) = delete;
    SeededIrcFixture &operator=(const SeededIrcFixture &) = delete;

    bool open();
    bool createWindow();

    Backend &backend();
    IrcSlashSession &slash();
    IrcController &controller();
    FakeIrcTransport *omarchyTransport() const;
    FakeIrcTransport *oftcTransport() const;
    QQuickWindow *window() const;
    QString lastError() const;

    static QString omarchyNetworkId();
    static QString oftcNetworkId();

private:
    bool fail(const QString &why);
    bool installXdg();
    bool startNetwork(const QString &networkId,
                      const QString &nick,
                      const QStringList &autojoin,
                      FakeIrcTransport **transport);

    std::unique_ptr<QTemporaryDir> m_xdg;
    std::unique_ptr<Backend> m_backend;
    std::unique_ptr<IrcSlashSession> m_slash;
    std::unique_ptr<IrcController> m_controller;
    std::unique_ptr<QQmlApplicationEngine> m_engine;
    std::unique_ptr<QObject> m_root;
    QPointer<QQuickWindow> m_window;
    FakeIrcTransport *m_omarchyTransport = nullptr;
    FakeIrcTransport *m_oftcTransport = nullptr;
    QString m_error;
};
