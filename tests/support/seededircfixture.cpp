#include "seededircfixture.h"

#include "backend.h"
#include "ircconnection.h"
#include "irccontroller.h"
#include "ircdemoserver.h"
#include "ircloopbacktransport.h"
#include "ircslashcomplete.h"
#include "storage/credentialstore.h"

#include <QCoreApplication>
#include <QDir>
#include <QMetaObject>
#include <QQmlApplicationEngine>
#include <QQmlComponent>
#include <QQuickWindow>
#include <QSettings>
#include <QTemporaryDir>
#include <QTest>
#include <QVariantMap>

namespace
{
class MissingCredentialStore final : public CredentialStore
{
public:
    void read(const CredentialKey &) override
    {
        emit readFinished(State::Loading, {}, {});
        QMetaObject::invokeMethod(this, [this]() {
            emit readFinished(State::Missing, {}, {});
        }, Qt::QueuedConnection);
    }

    void write(const CredentialKey &, const QString &) override
    {
        QMetaObject::invokeMethod(this, [this]() {
            emit writeFinished(State::Available, {});
        }, Qt::QueuedConnection);
    }

    void remove(const CredentialKey &) override
    {
        QMetaObject::invokeMethod(this, [this]() {
            emit writeFinished(State::Missing, {});
        }, Qt::QueuedConnection);
    }
};
}

SeededIrcFixture::SeededIrcFixture(QObject *parent)
    : QObject(parent)
{
}

SeededIrcFixture::~SeededIrcFixture()
{
    m_window.clear();
    m_root.reset();
    m_engine.reset();
    m_connection.reset();
    m_credentials.reset();
    m_controller.reset();
    m_omarchyTransport = nullptr;
    m_oftcTransport = nullptr;
    m_demo.reset();
    m_slash.reset();
    m_backend.reset();
    m_xdg.reset();
}

QString SeededIrcFixture::omarchyNetworkId()
{
    return IrcDemoServer::omarchyNetworkId();
}

QString SeededIrcFixture::oftcNetworkId()
{
    return IrcDemoServer::oftcNetworkId();
}

QString SeededIrcFixture::omarchyId() const
{
    return omarchyNetworkId();
}

QString SeededIrcFixture::oftcId() const
{
    return oftcNetworkId();
}

Backend &SeededIrcFixture::backend()
{
    return *m_backend;
}

IrcSlashSession &SeededIrcFixture::slash()
{
    return *m_slash;
}

IrcController &SeededIrcFixture::controller()
{
    return *m_controller;
}

IrcConnection *SeededIrcFixture::connection() const
{
    return m_connection.get();
}

IrcLoopbackTransport *SeededIrcFixture::omarchyTransport() const
{
    return m_omarchyTransport;
}

IrcLoopbackTransport *SeededIrcFixture::oftcTransport() const
{
    return m_oftcTransport;
}

QQuickWindow *SeededIrcFixture::window() const
{
    return m_window;
}

QString SeededIrcFixture::lastError() const
{
    return m_error;
}

QObject *SeededIrcFixture::backendObject() const
{
    return m_backend.get();
}

QObject *SeededIrcFixture::irc() const
{
    return m_controller.get();
}

QObject *SeededIrcFixture::connectionObject() const
{
    return m_connection.get();
}

QObject *SeededIrcFixture::slashObject() const
{
    return m_slash.get();
}

QObject *SeededIrcFixture::omarchyTransportObject() const
{
    return m_omarchyTransport;
}

QObject *SeededIrcFixture::oftcTransportObject() const
{
    return m_oftcTransport;
}

QObject *SeededIrcFixture::windowObject() const
{
    return m_window;
}

bool SeededIrcFixture::fail(const QString &why)
{
    m_error = why;
    emit lastErrorChanged();
    return false;
}

bool SeededIrcFixture::installXdg()
{
    m_xdg = std::make_unique<QTemporaryDir>();
    if (!m_xdg->isValid())
        return fail(QStringLiteral("xdg temp dir"));
    const QString root = m_xdg->path();
    const QString config = root + QLatin1String("/config");
    QDir().mkpath(config);
    QDir().mkpath(root + QLatin1String("/cache"));
    QDir().mkpath(root + QLatin1String("/data"));
    qputenv("XDG_CONFIG_HOME", config.toUtf8());
    qputenv("XDG_CACHE_HOME", (root + QLatin1String("/cache")).toUtf8());
    qputenv("XDG_DATA_HOME", (root + QLatin1String("/data")).toUtf8());
    QSettings::setPath(QSettings::NativeFormat, QSettings::UserScope, config);
    return true;
}

bool SeededIrcFixture::open()
{
    if (m_controller)
        return fail(QStringLiteral("already open"));
    if (!installXdg())
        return false;

    m_demo = std::make_unique<IrcDemoServer>();
    if (!m_demo->writeProfiles())
        return fail(m_demo->lastError().isEmpty()
                        ? QStringLiteral("write profiles")
                        : m_demo->lastError());

    m_backend = std::make_unique<Backend>();
    m_slash = std::make_unique<IrcSlashSession>();
    m_controller = std::make_unique<IrcController>();
    m_credentials = std::make_unique<MissingCredentialStore>();
    m_connection = std::make_unique<IrcConnection>(*m_controller, *m_credentials);
    if (!m_demo->attach(*m_controller))
        return fail(m_demo->lastError().isEmpty()
                        ? QStringLiteral("attach demo")
                        : m_demo->lastError());
    m_omarchyTransport = m_demo->omarchyTransport();
    m_oftcTransport = m_demo->oftcTransport();
    emit openedChanged();
    return true;
}

bool SeededIrcFixture::createWindow()
{
    if (!m_controller)
        return fail(QStringLiteral("open first"));
    if (m_window)
        return fail(QStringLiteral("window already created"));

    m_engine = std::make_unique<QQmlApplicationEngine>();
    QQmlComponent component(m_engine.get(), QUrl(QStringLiteral("qrc:/OmaircWindow.qml")));
    if (component.status() == QQmlComponent::Error)
        return fail(component.errorString());

    QVariantMap properties;
    properties.insert(QStringLiteral("backend"), QVariant::fromValue(m_backend.get()));
    properties.insert(QStringLiteral("irc"), QVariant::fromValue(m_controller.get()));
    properties.insert(QStringLiteral("slashCommands"), QVariant::fromValue(m_slash.get()));
    m_root.reset(component.createWithInitialProperties(properties));
    if (!m_root)
        return fail(component.errorString());
    m_window = qobject_cast<QQuickWindow *>(m_root.get());
    if (!m_window)
        return fail(QStringLiteral("root is not QQuickWindow"));
    if (!QTest::qWaitForWindowExposed(m_window))
        return fail(QStringLiteral("window not exposed"));
    m_window->setWidth(1180);
    m_window->setHeight(760);
    m_controller->selectConversation(omarchyNetworkId(), QStringLiteral("#omarchy"));
    QCoreApplication::processEvents();
    emit windowChanged();
    return true;
}

void SeededIrcFixture::injectOmarchy(const QString &bytes)
{
    if (m_demo)
        m_demo->injectOmarchy(bytes.toUtf8());
}

void SeededIrcFixture::injectOftc(const QString &bytes)
{
    if (m_demo)
        m_demo->injectOftc(bytes.toUtf8());
}

bool SeededIrcFixture::echoLastOmarchyPrivmsg()
{
    if (!m_demo || !m_controller)
        return false;
    return m_demo->echoLastOmarchyPrivmsg(m_controller->currentNick());
}

bool SeededIrcFixture::echoLastOftcPrivmsg()
{
    if (!m_demo || !m_controller)
        return false;
    return m_demo->echoLastOftcPrivmsg(m_controller->currentNick());
}
