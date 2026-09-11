#pragma once

#include "conversationlistmodel.h"

#include <QColor>
#include <QHash>
#include <QPointer>
#include <QSet>
#include <QString>
#include <QStringList>
#include <QtGlobal>
#include <QVector>

#include <memory>

class QQuickWindow;

struct TranscriptRowChrome
{
    QString body;
    bool avatarVisible = false;
    bool headerVisible = false;
    qreal height = 0;
    QColor bodyColor;
};

QVector<TranscriptRowChrome> collectTranscriptChrome(QQuickWindow *window);

enum class LiveUiKind { Channel, Direct, Status };

class LiveUiId
{
public:
    static LiveUiId channel(QString networkId, QString target);
    static LiveUiId direct(QString networkId, QString nick);
    static LiveUiId status(QString networkId);

    QString wire() const;
    QString networkId() const;
    QString target() const;
    LiveUiKind kind() const;

private:
    LiveUiId(LiveUiKind kind, QString networkId, QString target);

    LiveUiKind m_kind = LiveUiKind::Channel;
    QString m_networkId;
    QString m_target;
};

struct LiveUiSeat
{
    QString daemonName;
    quint16 port = 0;
    QString networkId;
    QString channel;
    QString clientNick;
    QString peerNick;
    QString extraNick;
    QString topic;
    QString inboundMark;
    QString outboundMark;
    QString statusMark;

    LiveUiId channelId() const;
    LiveUiId peerDirectId() const;
    LiveUiId statusId() const;
    QSet<QString> channelMembers() const;
};

class Backend;
class IrcConnection;
class IrcController;
class IrcSlashSession;
class QQmlApplicationEngine;
class QQuickItem;
class QQuickWindow;
class QTemporaryDir;
class RawIrcPeer;

class LiveUiWorld
{
public:
    LiveUiWorld();
    ~LiveUiWorld();

    LiveUiWorld(const LiveUiWorld &) = delete;
    LiveUiWorld &operator=(const LiveUiWorld &) = delete;

    bool open();
    void close();

    const LiveUiSeat &left() const;
    const LiveUiSeat &right() const;

    bool click(const LiveUiId &id);
    bool clickMember(const QString &nick);
    bool typeAndSend(const QString &text);
    bool peerPrivmsg(const LiveUiSeat &seat, const QString &target, const QString &body);
    bool saveShot(const QString &stem);

    QString selectedConversationId() const;
    QString visibleTopic() const;
    QString visibleFooterNick() const;
    QString visiblePeopleHeading() const;
    QStringList visibleMemberNicks() const;
    QStringList visibleBodies() const;
    QVector<TranscriptRowChrome> transcriptChrome() const;
    QStringList visibleConsoleTexts() const;
    QString visibleStatusNetworkId() const;
    QString windowTitle() const;
    QString dump() const;

private:
    bool pickSeats();
    bool writeProfiles();
    bool seedPeers();
    bool loadWindow();
    bool waitReady();
    bool sendStatusMarks();
    void tapSession(const QString &networkId);
    void notePeerCommand(const QString &line);
    RawIrcPeer *twinFor(const LiveUiSeat &seat) const;
    QQuickItem *rootItem() const;
    QQuickItem *findNamed(const QString &name) const;
    QQuickItem *findConversationRow(const LiveUiId &id) const;
    QQuickItem *findStatusHeader(const QString &networkId) const;
    QQuickItem *findMember(const QString &nick) const;
    bool bringIntoView(QQuickItem *item) const;
    void mouseClick(QQuickItem *item) const;
    bool clickSettled(const LiveUiId &id) const;
    QString itemText(QQuickItem *item) const;
    QStringList collectNamedTexts(QQuickItem *list, const QString &objectName) const;
    QStringList sidebarConversationIds() const;
    QString sessionStateName(const QString &networkId) const;
    QString artifactDir() const;

    LiveUiSeat m_left;
    LiveUiSeat m_right;
    std::unique_ptr<QTemporaryDir> m_xdg;
    std::unique_ptr<Backend> m_backend;
    std::unique_ptr<IrcSlashSession> m_slash;
    std::unique_ptr<IrcController> m_controller;
    std::unique_ptr<IrcConnection> m_connection;
    std::unique_ptr<QQmlApplicationEngine> m_engine;
    std::unique_ptr<RawIrcPeer> m_leftTwin;
    std::unique_ptr<RawIrcPeer> m_leftExtra;
    std::unique_ptr<RawIrcPeer> m_rightTwin;
    std::unique_ptr<RawIrcPeer> m_rightExtra;
    QPointer<QQuickWindow> m_window;
    QHash<QString, int> m_welcomeCounts;
    QHash<QString, int> m_namesEndCounts;
    QStringList m_peerCommands;
    QString m_lastShot;
    QString m_fail;
};
