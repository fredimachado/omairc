#pragma once

#include <QObject>
#include <QPointer>
#include <QString>

#include <memory>

class Backend;
class FakeIrcTransport;
class IrcController;
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
    Q_PROPERTY(QObject *slash READ slashObject NOTIFY openedChanged)
    Q_PROPERTY(QObject *omarchyTransport READ omarchyTransportObject NOTIFY openedChanged)
    Q_PROPERTY(QObject *oftcTransport READ oftcTransportObject NOTIFY openedChanged)
    Q_PROPERTY(QObject *window READ windowObject NOTIFY windowChanged)
    Q_PROPERTY(QString lastError READ lastError NOTIFY lastErrorChanged)

public:
    explicit SeededIrcFixture(QObject *parent = nullptr);
    ~SeededIrcFixture() override;

    SeededIrcFixture(const SeededIrcFixture &) = delete;
    SeededIrcFixture &operator=(const SeededIrcFixture &) = delete;

    Q_INVOKABLE bool open();
    Q_INVOKABLE bool createWindow();

    Backend &backend();
    IrcSlashSession &slash();
    IrcController &controller();
    FakeIrcTransport *omarchyTransport() const;
    FakeIrcTransport *oftcTransport() const;
    QQuickWindow *window() const;
    QString lastError() const;

    QObject *backendObject() const;
    QObject *irc() const;
    QObject *slashObject() const;
    QObject *omarchyTransportObject() const;
    QObject *oftcTransportObject() const;
    QObject *windowObject() const;

    static QString omarchyNetworkId();
    static QString oftcNetworkId();

signals:
    void openedChanged();
    void windowChanged();
    void lastErrorChanged();

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
