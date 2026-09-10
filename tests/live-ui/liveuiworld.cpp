#include "liveuiworld.h"

#include "backend.h"
#include "ircconnection.h"
#include "irccontroller.h"
#include "ircnetworkprofile.h"
#include "ircprofilestore.h"
#include "ircsession.h"
#include "ircslashcomplete.h"
#include "ircstatusconsole.h"
#include "liveharness.h"
#include "livepeer.h"
#include "memberlistmodel.h"
#include "messagelistmodel.h"
#include "networklogmodel.h"

#include <QAbstractItemModel>
#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QGuiApplication>
#include <QImage>
#include <QMetaEnum>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQmlError>
#include <QQuickItem>
#include <QQuickWindow>
#include <QSettings>
#include <QSslConfiguration>
#include <QTemporaryDir>
#include <QTest>
#include <QVariant>

#include <algorithm>
#include <functional>
#include <utility>

namespace
{
QString envOr(const char *key, const QString &fallback)
{
    const QByteArray value = qgetenv(key);
    return value.isEmpty() ? fallback : QString::fromLocal8Bit(value);
}

QString sessionKey(IrcSession::State state)
{
    const QMetaEnum meta = QMetaEnum::fromType<IrcSession::State>();
    const char *name = meta.valueToKey(int(state));
    return name ? QString::fromLatin1(name) : QStringLiteral("?");
}

void walkItems(QQuickItem *item, const std::function<void(QQuickItem *)> &visit)
{
    if (!item)
        return;
    visit(item);
    const QList<QQuickItem *> children = item->childItems();
    for (QQuickItem *child : children)
        walkItems(child, visit);
}

QString ancestorNetworkId(QQuickItem *item)
{
    for (QQuickItem *current = item; current; current = current->parentItem()) {
        const QVariant id = current->property("networkId");
        if (id.isValid() && !id.toString().isEmpty())
            return id.toString();
    }
    return {};
}

bool alreadyChosen(const QVector<LiveDaemonInfo> &chosen, const QString &name)
{
    for (const LiveDaemonInfo &daemon : chosen) {
        if (daemon.name == name)
            return true;
    }
    return false;
}

QQuickItem *directNamed(QQuickItem *parent, const QString &name)
{
    if (!parent)
        return nullptr;
    const QList<QQuickItem *> children = parent->childItems();
    for (QQuickItem *child : children) {
        if (child->objectName() == name)
            return child;
    }
    return nullptr;
}

TranscriptRowChrome chromeFromRow(QQuickItem *row)
{
    TranscriptRowChrome chrome;
    if (!row)
        return chrome;
    chrome.height = row->height();
    if (QQuickItem *avatar = directNamed(row, QStringLiteral("messageAvatar")))
        chrome.avatarVisible = avatar->property("visible").toBool();
    if (QQuickItem *header = directNamed(row, QStringLiteral("messageHeader")))
        chrome.headerVisible = header->property("visible").toBool();
    QQuickItem *event = directNamed(row, QStringLiteral("messageEvent"));
    QQuickItem *body = directNamed(row, QStringLiteral("messageBody"));
    if (event && event->property("visible").toBool())
        chrome.body = event->property("text").toString();
    else if (body) {
        chrome.body = body->property("text").toString();
        chrome.bodyColor = body->property("color").value<QColor>();
    }
    return chrome;
}

QQuickItem *findMessageList(QQuickWindow *window)
{
    if (!window)
        return nullptr;
    if (QQuickItem *named = window->findChild<QQuickItem *>(QStringLiteral("messageList")))
        return named;
    QQuickItem *found = nullptr;
    walkItems(window->contentItem(), [&](QQuickItem *item) {
        if (!found && item->objectName() == QStringLiteral("messageList"))
            found = item;
    });
    return found;
}
}

QVector<TranscriptRowChrome> collectTranscriptChrome(QQuickWindow *window)
{
    QVector<TranscriptRowChrome> rows;
    QQuickItem *list = findMessageList(window);
    if (!list)
        return rows;
    list->setProperty("cacheBuffer", 100000);
    QMetaObject::invokeMethod(list, "positionViewAtEnd");
    QCoreApplication::processEvents();

    QQuickItem *content = list->property("contentItem").value<QQuickItem *>();
    if (!content)
        return rows;
    QVector<QQuickItem *> delegates;
    const QList<QQuickItem *> children = content->childItems();
    for (QQuickItem *child : children) {
        if (directNamed(child, QStringLiteral("messageAvatar")))
            delegates.append(child);
    }
    std::sort(delegates.begin(), delegates.end(), [](QQuickItem *left, QQuickItem *right) {
        return left->y() < right->y();
    });
    rows.reserve(delegates.size());
    for (QQuickItem *row : delegates)
        rows.append(chromeFromRow(row));
    return rows;
}

LiveUiId::LiveUiId(LiveUiKind kind, QString networkId, QString target)
    : m_kind(kind)
    , m_networkId(std::move(networkId))
    , m_target(std::move(target))
{
}

LiveUiId LiveUiId::channel(QString networkId, QString target)
{
    Q_ASSERT(!networkId.isEmpty());
    Q_ASSERT(!target.isEmpty());
    return LiveUiId(LiveUiKind::Channel, std::move(networkId), std::move(target));
}

LiveUiId LiveUiId::direct(QString networkId, QString nick)
{
    Q_ASSERT(!networkId.isEmpty());
    Q_ASSERT(!nick.isEmpty());
    return LiveUiId(LiveUiKind::Direct, std::move(networkId), std::move(nick));
}

LiveUiId LiveUiId::status(QString networkId)
{
    Q_ASSERT(!networkId.isEmpty());
    return LiveUiId(LiveUiKind::Status, std::move(networkId), QStringLiteral("status"));
}

QString LiveUiId::wire() const
{
    if (m_kind == LiveUiKind::Status)
        return QStringLiteral("status\n") + m_networkId;
    IrcConversationKey key;
    key.networkId = m_networkId;
    key.normalizedTarget = m_target;
    return ircConversationId(key);
}

QString LiveUiId::networkId() const
{
    return m_networkId;
}

QString LiveUiId::target() const
{
    return m_target;
}

LiveUiKind LiveUiId::kind() const
{
    return m_kind;
}

LiveUiId LiveUiSeat::channelId() const
{
    return LiveUiId::channel(networkId, channel);
}

LiveUiId LiveUiSeat::peerDirectId() const
{
    return LiveUiId::direct(networkId, peerNick);
}

LiveUiId LiveUiSeat::statusId() const
{
    return LiveUiId::status(networkId);
}

QSet<QString> LiveUiSeat::channelMembers() const
{
    return {clientNick, peerNick, extraNick};
}

LiveUiWorld::LiveUiWorld() = default;

LiveUiWorld::~LiveUiWorld()
{
    close();
}

const LiveUiSeat &LiveUiWorld::left() const
{
    return m_left;
}

const LiveUiSeat &LiveUiWorld::right() const
{
    return m_right;
}

bool LiveUiWorld::open()
{
    close();
    qputenv("OMAIRC_ALLOW_MULTI", "1");

    if (!pickSeats())
        return false;

    m_xdg = std::make_unique<QTemporaryDir>();
    if (!m_xdg->isValid()) {
        m_fail = QStringLiteral("xdg temp dir");
        return false;
    }
    const QString root = m_xdg->path();
    const QString config = root + QLatin1String("/config");
    const QString cache = root + QLatin1String("/cache");
    const QString data = root + QLatin1String("/data");
    const QString state = root + QLatin1String("/state");
    QDir().mkpath(config);
    QDir().mkpath(cache);
    QDir().mkpath(data);
    QDir().mkpath(state);
    qputenv("XDG_CONFIG_HOME", config.toUtf8());
    qputenv("XDG_CACHE_HOME", cache.toUtf8());
    qputenv("XDG_DATA_HOME", data.toUtf8());
    qputenv("XDG_STATE_HOME", state.toUtf8());
    QSettings::setPath(QSettings::NativeFormat, QSettings::UserScope, config);

    if (!writeProfiles())
        return false;
    if (!seedPeers())
        return false;

    m_backend = std::make_unique<Backend>();
    m_slash = std::make_unique<IrcSlashSession>();
    m_controller = std::make_unique<IrcController>();
    m_connection = std::make_unique<IrcConnection>(*m_controller);

    if (!loadWindow())
        return false;
    if (!m_connection->activateStartup()) {
        m_fail = QStringLiteral("activateStartup");
        return false;
    }
    tapSession(m_left.networkId);
    tapSession(m_right.networkId);
    if (!waitReady())
        return false;
    if (!sendStatusMarks())
        return false;
    return true;
}

void LiveUiWorld::close()
{
    m_window.clear();
    m_engine.reset();
    m_connection.reset();
    m_controller.reset();
    m_slash.reset();
    m_backend.reset();
    m_leftTwin.reset();
    m_leftExtra.reset();
    m_rightTwin.reset();
    m_rightExtra.reset();
    m_xdg.reset();
    m_welcomeCounts.clear();
    m_namesEndCounts.clear();
    m_peerCommands.clear();
}

bool LiveUiWorld::pickSeats()
{
    const QVector<LiveDaemonInfo> all = liveDaemons();
    QVector<LiveDaemonInfo> chosen;
    const auto take = [&](const QString &name) {
        for (const LiveDaemonInfo &daemon : all) {
            if (daemon.name == name && daemon.plainPort != 0 && !alreadyChosen(chosen, name))
                chosen.append(daemon);
        }
    };
    take(QStringLiteral("ergo"));
    take(QStringLiteral("solanum"));
    if (chosen.size() < 2) {
        for (const LiveDaemonInfo &daemon : all) {
            if (daemon.plainPort == 0 || alreadyChosen(chosen, daemon.name))
                continue;
            chosen.append(daemon);
            if (chosen.size() >= 2)
                break;
        }
    }
    if (chosen.size() < 2) {
        m_fail = QStringLiteral("need two plain-port daemons");
        return false;
    }

    const QString channel = uniqueChannel();
    const QString peer = uniqueNick(16);
    const qint64 pid = QCoreApplication::applicationPid();

    auto fill = [&](LiveUiSeat &seat, const LiveDaemonInfo &daemon, QLatin1Char side) {
        seat.daemonName = daemon.name;
        seat.port = daemon.plainPort;
        seat.channel = channel;
        seat.clientNick = uniqueNick(16);
        seat.peerNick = peer;
        seat.extraNick = uniqueNick(16);
        seat.topic = QStringLiteral("topic-%1-%2").arg(side).arg(pid);
        seat.inboundMark = QStringLiteral("in-%1-%2-1").arg(side).arg(pid);
        seat.outboundMark = QStringLiteral("out-%1-%2-1").arg(side).arg(pid);
        seat.statusMark = QStringLiteral("st-%1-%2-1").arg(side).arg(pid);
    };
    fill(m_left, chosen.at(0), QLatin1Char('L'));
    fill(m_right, chosen.at(1), QLatin1Char('R'));
    return true;
}

bool LiveUiWorld::writeProfiles()
{
    auto write = [&](LiveUiSeat &seat) {
        IrcNetworkProfile profile = IrcNetworkProfile::create();
        profile.host = liveHost();
        profile.port = seat.port;
        profile.tlsEnabled = false;
        profile.connectOnStartup = true;
        profile.nick = seat.clientNick;
        profile.username = QStringLiteral("omairc");
        profile.realname = QStringLiteral("Omairc live ui");
        profile.autojoinChannels = {seat.channel};
        if (profile.validate() != IrcNetworkProfile::Problem::None) {
            m_fail = QStringLiteral("invalid profile for %1").arg(seat.daemonName);
            return false;
        }
        IrcProfileStore store;
        store.save(profile);
        seat.networkId = profile.networkId;
        return true;
    };
    return write(m_left) && write(m_right);
}

bool LiveUiWorld::seedPeers()
{
    const QSslConfiguration ssl;
    auto connectPeer = [&](std::unique_ptr<RawIrcPeer> &peer,
                           const LiveUiSeat &seat,
                           const QString &nick,
                           bool twin) {
        peer = std::make_unique<RawIrcPeer>(liveHost(), seat.port, false, ssl, nick);
        if (!peer->waitRegistered()) {
            m_fail = QStringLiteral("peer %1 on %2 did not register").arg(nick, seat.daemonName);
            return false;
        }
        if (!peer->join(seat.channel)) {
            m_fail = QStringLiteral("peer %1 did not join %2").arg(nick, seat.channel);
            return false;
        }
        if (!twin)
            return true;
        const QString topicLine = QStringLiteral("TOPIC %1 :%2").arg(seat.channel, seat.topic);
        notePeerCommand(topicLine);
        peer->writeLine(topicLine);
        if (!waitUntil([&] {
                for (const IrcMessage &message : peer->incoming) {
                    if ((message.command == "TOPIC" || message.command == "332")
                        && !message.parameters.empty()
                        && QString::fromStdString(message.parameters.back()) == seat.topic) {
                        return true;
                    }
                }
                return false;
            })) {
            m_fail = QStringLiteral("topic %1 missing on %2").arg(seat.topic, seat.daemonName);
            return false;
        }
        return true;
    };

    return connectPeer(m_leftTwin, m_left, m_left.peerNick, true)
        && connectPeer(m_leftExtra, m_left, m_left.extraNick, false)
        && connectPeer(m_rightTwin, m_right, m_right.peerNick, true)
        && connectPeer(m_rightExtra, m_right, m_right.extraNick, false);
}

bool LiveUiWorld::loadWindow()
{
    m_engine = std::make_unique<QQmlApplicationEngine>();
    QObject::connect(m_engine.get(), &QQmlApplicationEngine::warnings,
                     m_engine.get(), [this](const QList<QQmlError> &warnings) {
                         for (const QQmlError &warning : warnings) {
                             if (m_peerCommands.size() < 24)
                                 m_peerCommands.append(QStringLiteral("qml:%1")
                                                           .arg(warning.toString()));
                         }
                     });
    m_engine->rootContext()->setContextProperty(QStringLiteral("appBackend"), m_backend.get());
    m_engine->rootContext()->setContextProperty(QStringLiteral("ircController"),
                                                m_controller.get());
    m_engine->rootContext()->setContextProperty(QStringLiteral("ircConnection"),
                                                m_connection.get());
    m_engine->rootContext()->setContextProperty(QStringLiteral("slashSession"), m_slash.get());
    m_engine->load(QUrl(QStringLiteral("qrc:/Main.qml")));
    if (m_engine->rootObjects().isEmpty()) {
        m_fail = QStringLiteral("qrc:/Main.qml failed to load");
        return false;
    }
    m_window = qobject_cast<QQuickWindow *>(m_engine->rootObjects().constFirst());
    if (!m_window) {
        m_fail = QStringLiteral("root is not a window");
        return false;
    }
    if (!QTest::qWaitForWindowExposed(m_window)) {
        m_fail = QStringLiteral("window not exposed");
        return false;
    }
    m_window->setWidth(1180);
    m_window->setHeight(760);
    QCoreApplication::processEvents();
    return true;
}

void LiveUiWorld::tapSession(const QString &networkId)
{
    IrcSession *session = m_controller ? m_controller->session(networkId) : nullptr;
    if (!session)
        return;
    QObject::connect(session, &IrcSession::messageReceived, session,
                     [this](const QString &id, const IrcMessage &message) {
                         const QString command = QString::fromStdString(message.command);
                         if (command.compare(QLatin1String("001"), Qt::CaseInsensitive) == 0)
                             m_welcomeCounts[id] += 1;
                         if (command.compare(QLatin1String("366"), Qt::CaseInsensitive) == 0)
                             m_namesEndCounts[id] += 1;
                     });
}

bool LiveUiWorld::waitReady()
{
    const auto registered = [&](const LiveUiSeat &seat) {
        IrcSession *session = m_controller->session(seat.networkId);
        return session && session->state() == IrcSession::State::Registered;
    };
    if (!waitUntil([&] { return registered(m_left) && registered(m_right); })) {
        m_fail = QStringLiteral("sessions did not register");
        return false;
    }

    const auto named = [&](const LiveUiSeat &seat) {
        return m_namesEndCounts.value(seat.networkId) > 0;
    };
    if (!waitUntil([&] { return named(m_left) && named(m_right); })) {
        m_fail = QStringLiteral("missing 366");
        return false;
    }

    const auto sidebarReady = [&](const LiveUiSeat &seat) {
        return findConversationRow(seat.channelId()) != nullptr
            && m_controller->connectionStatusFor(seat.networkId)
                == QStringLiteral("Connected");
    };
    if (!waitUntil([&] { return sidebarReady(m_left) && sidebarReady(m_right); })) {
        m_fail = QStringLiteral("sidebar or Connected missing");
        return false;
    }
    return true;
}

bool LiveUiWorld::sendStatusMarks()
{
    // NOTICE before the client is registered never reaches Status
    auto send = [&](const LiveUiSeat &seat, RawIrcPeer *twin) {
        if (!twin)
            return false;
        const QString line = QStringLiteral("NOTICE %1 :%2").arg(seat.clientNick, seat.statusMark);
        notePeerCommand(line);
        twin->writeLine(line);
        return true;
    };
    if (!send(m_left, m_leftTwin.get()) || !send(m_right, m_rightTwin.get()))
        return false;
    if (!click(m_left.statusId()))
        return false;
    if (!waitUntil([&] { return visibleConsoleTexts().contains(m_left.statusMark); })) {
        m_fail = QStringLiteral("left status mark missing");
        return false;
    }
    if (!click(m_right.statusId()))
        return false;
    if (!waitUntil([&] { return visibleConsoleTexts().contains(m_right.statusMark); })) {
        m_fail = QStringLiteral("right status mark missing");
        return false;
    }
    return true;
}

void LiveUiWorld::notePeerCommand(const QString &line)
{
    m_peerCommands.append(line);
    while (m_peerCommands.size() > 8)
        m_peerCommands.removeFirst();
}

RawIrcPeer *LiveUiWorld::twinFor(const LiveUiSeat &seat) const
{
    if (seat.networkId == m_left.networkId)
        return m_leftTwin.get();
    if (seat.networkId == m_right.networkId)
        return m_rightTwin.get();
    return nullptr;
}

QQuickItem *LiveUiWorld::rootItem() const
{
    return m_window ? m_window->contentItem() : nullptr;
}

QQuickItem *LiveUiWorld::findNamed(const QString &name) const
{
    if (QQuickItem *named = m_window ? m_window->findChild<QQuickItem *>(name) : nullptr)
        return named;
    QQuickItem *found = nullptr;
    walkItems(rootItem(), [&](QQuickItem *item) {
        if (!found && item->objectName() == name)
            found = item;
    });
    return found;
}

QQuickItem *LiveUiWorld::findConversationRow(const LiveUiId &id) const
{
    QQuickItem *hidden = nullptr;
    QQuickItem *found = nullptr;
    walkItems(rootItem(), [&](QQuickItem *item) {
        if (found)
            return;
        const QString conversationId = item->property("conversationId").toString();
        const QString name = item->property("conversationName").toString();
        const QString networkId = item->property("networkId").toString();
        if (conversationId != id.wire()
            && !(networkId == id.networkId() && name == id.target())
            && item->objectName()
                != QStringLiteral("conversation-%1-%2").arg(id.networkId(), id.target())) {
            return;
        }
        if (item->isVisible() && item->height() > 0)
            found = item;
        else if (!hidden)
            hidden = item;
    });
    return found ? found : hidden;
}

QQuickItem *LiveUiWorld::findStatusHeader(const QString &networkId) const
{
    QQuickItem *hidden = nullptr;
    QQuickItem *found = nullptr;
    walkItems(rootItem(), [&](QQuickItem *item) {
        if (found)
            return;
        const QString name = item->objectName();
        const bool button = name == QLatin1String("networkHeaderButton")
            || name.startsWith(QLatin1String("networkHeaderButton-"));
        if (!button)
            return;
        if (name != QStringLiteral("networkHeaderButton-%1").arg(networkId)
            && ancestorNetworkId(item) != networkId) {
            return;
        }
        if (item->isVisible() && item->height() > 0)
            found = item;
        else if (!hidden)
            hidden = item;
    });
    return found ? found : hidden;
}

QQuickItem *LiveUiWorld::findMember(const QString &nick) const
{
    return findNamed(QStringLiteral("member-%1").arg(nick));
}

bool LiveUiWorld::bringIntoView(QQuickItem *item) const
{
    if (!item)
        return false;
    for (QQuickItem *current = item->parentItem(); current; current = current->parentItem()) {
        if (!current->property("contentY").isValid()
            || !current->property("contentItem").isValid()) {
            continue;
        }
        auto *content = current->property("contentItem").value<QQuickItem *>();
        if (!content)
            continue;
        const QPointF point = item->mapToItem(content, QPointF(0, item->height() / 2));
        const qreal view = current->height();
        const qreal contentY = current->property("contentY").toReal();
        if (point.y() < contentY || point.y() > contentY + view) {
            current->setProperty("contentY", qMax(qreal(0), point.y() - view / 2));
            QCoreApplication::processEvents();
        }
        break;
    }
    return item->isVisible() && item->width() > 0 && item->height() > 0;
}

void LiveUiWorld::mouseClick(QQuickItem *item) const
{
    QQuickWindow *window = item->window();
    if (!window)
        return;
    const QPointF scene = item->mapToScene(QPointF(item->width() / 2, item->height() / 2));
    QTest::mouseClick(window, Qt::LeftButton, Qt::NoModifier, scene.toPoint());
}

bool LiveUiWorld::clickSettled(const LiveUiId &id) const
{
    if (!m_window || !m_controller)
        return false;
    if (id.kind() == LiveUiKind::Status) {
        return m_window->property("consoleVisible").toBool()
            && m_controller->console()->networkId() == id.networkId();
    }
    return !m_window->property("consoleVisible").toBool()
        && selectedConversationId() == id.wire();
}

bool LiveUiWorld::click(const LiveUiId &id)
{
    QQuickItem *target = nullptr;
    if (!waitUntil([&] {
            target = id.kind() == LiveUiKind::Status
                ? findStatusHeader(id.networkId())
                : findConversationRow(id);
            return target && target->isVisible() && target->height() > 0;
        })) {
        m_fail = QStringLiteral("no click target for %1").arg(id.wire());
        return false;
    }
    if (!bringIntoView(target))
        return false;
    mouseClick(target);
    if (!waitUntil([&] { return clickSettled(id); })) {
        m_fail = QStringLiteral("click did not select %1").arg(id.wire());
        return false;
    }
    return true;
}

bool LiveUiWorld::clickMember(const QString &nick)
{
    QQuickItem *member = nullptr;
    if (!waitUntil([&] {
            member = findMember(nick);
            return member && member->isVisible() && member->height() > 0;
        })) {
        m_fail = QStringLiteral("member-%1 missing").arg(nick);
        return false;
    }
    if (!bringIntoView(member))
        return false;
    mouseClick(member);
    return waitUntil([&] {
        const QString id = selectedConversationId();
        return !m_window->property("consoleVisible").toBool()
            && id.endsWith(QLatin1Char('\n') + nick);
    });
}

bool LiveUiWorld::typeAndSend(const QString &text)
{
    QQuickItem *composer = findNamed(QStringLiteral("messageComposer"));
    if (!composer || !m_window) {
        m_fail = QStringLiteral("messageComposer missing");
        return false;
    }
    composer->forceActiveFocus();
    if (!waitUntil([&] { return composer->hasActiveFocus(); })) {
        m_fail = QStringLiteral("composer unfocused");
        return false;
    }
    for (const QChar ch : text)
        QTest::keyClick(m_window, ch.toLatin1(), Qt::NoModifier, 0);
    QTest::keyClick(m_window, Qt::Key_Return);
    return waitUntil([&] { return visibleBodies().contains(text); });
}

bool LiveUiWorld::peerPrivmsg(const LiveUiSeat &seat, const QString &target, const QString &body)
{
    RawIrcPeer *twin = twinFor(seat);
    if (!twin) {
        m_fail = QStringLiteral("no twin for %1").arg(seat.networkId);
        return false;
    }
    const QString line = QStringLiteral("PRIVMSG %1 :%2").arg(target, body);
    notePeerCommand(line);
    twin->writeLine(line);
    const LiveUiId want = target.compare(seat.clientNick, Qt::CaseInsensitive) == 0
        ? seat.peerDirectId()
        : LiveUiId::channel(seat.networkId, target);
    return waitUntil([&] { return findConversationRow(want) != nullptr; });
}

bool LiveUiWorld::saveShot(const QString &stem)
{
    if (!m_window) {
        m_fail = QStringLiteral("no window to grab");
        return false;
    }
    QCoreApplication::processEvents();
    const QImage image = m_window->grabWindow();
    if (image.isNull()) {
        m_fail = QStringLiteral("grabWindow empty");
        return false;
    }
    const QString dir = artifactDir();
    QDir().mkpath(dir);
    const QString path = dir + QLatin1Char('/') + stem + QLatin1String(".png");
    if (!image.save(path) || QFileInfo(path).size() <= 0) {
        m_fail = QStringLiteral("shot %1 missing").arg(path);
        return false;
    }
    m_lastShot = path;
    return true;
}

QString LiveUiWorld::selectedConversationId() const
{
    return m_window ? m_window->property("currentConversationId").toString() : QString();
}

QString LiveUiWorld::itemText(QQuickItem *item) const
{
    return item ? item->property("text").toString() : QString();
}

QString LiveUiWorld::visibleTopic() const
{
    return itemText(findNamed(QStringLiteral("conversationTopic")));
}

QString LiveUiWorld::visibleFooterNick() const
{
    return itemText(findNamed(QStringLiteral("selfNickLabel")));
}

QString LiveUiWorld::visiblePeopleHeading() const
{
    QString heading;
    if (QQuickItem *panel = findNamed(QStringLiteral("membersPanel"))) {
        walkItems(panel, [&](QQuickItem *item) {
            if (!heading.isEmpty())
                return;
            const QString text = item->property("text").toString();
            if (text.startsWith(QLatin1String("ONLINE - ")))
                heading = text;
        });
    }
    return heading;
}

QStringList LiveUiWorld::visibleMemberNicks() const
{
    QStringList nicks;
    if (QQuickItem *list = findNamed(QStringLiteral("membersList"))) {
        walkItems(list, [&](QQuickItem *item) {
            const QString nick = item->property("nick").toString();
            if (nick.isEmpty())
                return;
            if (item->objectName() == QStringLiteral("member-%1").arg(nick)
                && !nicks.contains(nick)) {
                nicks.append(nick);
            }
        });
    }
    return nicks;
}

QStringList LiveUiWorld::collectNamedTexts(QQuickItem *list, const QString &objectName) const
{
    QStringList texts;
    walkItems(list, [&](QQuickItem *item) {
        if (item->objectName() != objectName)
            return;
        const QString text = item->property("text").toString();
        if (!text.isEmpty())
            texts.append(text);
    });
    return texts;
}

QVector<TranscriptRowChrome> LiveUiWorld::transcriptChrome() const
{
    return collectTranscriptChrome(m_window);
}

QStringList LiveUiWorld::visibleBodies() const
{
    QQuickItem *list = findNamed(QStringLiteral("messageList"));
    if (list) {
        list->setProperty("cacheBuffer", 100000);
        QMetaObject::invokeMethod(list, "positionViewAtEnd");
        QCoreApplication::processEvents();
    }
    QStringList bodies = collectNamedTexts(list, QStringLiteral("messageBody"));
    bodies += collectNamedTexts(list, QStringLiteral("messageEvent"));
    if (auto *model = qobject_cast<QAbstractItemModel *>(
            list ? list->property("model").value<QObject *>() : nullptr)) {
        for (int row = 0; row < model->rowCount(); ++row) {
            const QString body =
                model->index(row, 0).data(MessageListModel::BodyRole).toString();
            if (!body.isEmpty() && !bodies.contains(body))
                bodies.append(body);
        }
    }
    return bodies;
}

QStringList LiveUiWorld::visibleConsoleTexts() const
{
    QQuickItem *list = findNamed(QStringLiteral("consoleList"));
    if (list) {
        list->setProperty("cacheBuffer", 100000);
        QMetaObject::invokeMethod(list, "positionViewAtEnd");
        QCoreApplication::processEvents();
    }
    QStringList texts = collectNamedTexts(list, QStringLiteral("consoleText"));
    if (auto *model = qobject_cast<QAbstractItemModel *>(
            list ? list->property("model").value<QObject *>() : nullptr)) {
        for (int row = 0; row < model->rowCount(); ++row) {
            const QString text =
                model->index(row, 0).data(NetworkLogModel::TextRole).toString();
            if (!text.isEmpty() && !texts.contains(text))
                texts.append(text);
        }
    }
    QStringList expanded = texts;
    for (const QString &text : texts) {
        const int split = text.indexOf(QLatin1String("- "));
        if (text.startsWith(QLatin1Char('-')) && split > 0)
            expanded.append(text.mid(split + 2));
    }
    return expanded;
}

QString LiveUiWorld::visibleStatusNetworkId() const
{
    return m_controller && m_controller->console()
        ? m_controller->console()->networkId()
        : QString();
}

QString LiveUiWorld::windowTitle() const
{
    return m_window ? m_window->title() : QString();
}

QStringList LiveUiWorld::sidebarConversationIds() const
{
    QStringList ids;
    if (!m_controller)
        return ids;
    QAbstractItemModel *model = m_controller->conversations();
    for (int row = 0; row < model->rowCount(); ++row) {
        ids.append(model->index(row, 0).data(ConversationListModel::ConversationIdRole)
                       .toString());
    }
    return ids;
}

QString LiveUiWorld::sessionStateName(const QString &networkId) const
{
    IrcSession *session = m_controller ? m_controller->session(networkId) : nullptr;
    if (!session)
        return QStringLiteral("none");
    return sessionKey(session->state());
}

QString LiveUiWorld::artifactDir() const
{
    return envOr("OMAIRC_LIVE_UI_ARTIFACTS",
                 QDir(QCoreApplication::applicationDirPath() + QLatin1String("/../test-artifacts/live-ui"))
                     .absolutePath());
}

QString LiveUiWorld::dump() const
{
    QStringList lines;
    lines << QStringLiteral("fail=%1").arg(m_fail);
    lines << QStringLiteral("left daemon=%1 port=%2 id=%3 state=%4 error=%5")
                 .arg(m_left.daemonName)
                 .arg(m_left.port)
                 .arg(m_left.networkId)
                 .arg(sessionStateName(m_left.networkId))
                 .arg(m_controller ? m_controller->lastErrorFor(m_left.networkId)
                                   : QString());
    lines << QStringLiteral("right daemon=%1 port=%2 id=%3 state=%4 error=%5")
                 .arg(m_right.daemonName)
                 .arg(m_right.port)
                 .arg(m_right.networkId)
                 .arg(sessionStateName(m_right.networkId))
                 .arg(m_controller ? m_controller->lastErrorFor(m_right.networkId)
                                   : QString());
    lines << QStringLiteral("sidebar=%1").arg(sidebarConversationIds().join(QLatin1Char('|')));
    lines << QStringLiteral("selected=%1").arg(selectedConversationId());
    lines << QStringLiteral("consoleOpen=%1 network=%2")
                 .arg(m_window ? m_window->property("consoleVisible").toBool() : false)
                 .arg(visibleStatusNetworkId());
    lines << QStringLiteral("001 L=%1 R=%2 366 L=%3 R=%4")
                 .arg(m_welcomeCounts.value(m_left.networkId))
                 .arg(m_welcomeCounts.value(m_right.networkId))
                 .arg(m_namesEndCounts.value(m_left.networkId))
                 .arg(m_namesEndCounts.value(m_right.networkId));
    lines << QStringLiteral("peer=%1").arg(m_peerCommands.join(QLatin1Char('|')));
    lines << QStringLiteral("shot=%1").arg(m_lastShot);
    QStringList chrome;
    for (const TranscriptRowChrome &row : transcriptChrome()) {
        chrome.append(QStringLiteral("%1 a=%2 h=%3")
                          .arg(row.body)
                          .arg(row.avatarVisible)
                          .arg(row.headerVisible));
    }
    lines << QStringLiteral("chrome=%1").arg(chrome.join(QLatin1Char('|')));
    return lines.join(QLatin1Char('\n'));
}
