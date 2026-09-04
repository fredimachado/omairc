#pragma once

#include "conversationlistmodel.h"
#include "irceventreducer.h"
#include "ircsessionmanager.h"
#include "ircstatusconsole.h"
#include "memberlistmodel.h"
#include "messagelistmodel.h"

#include <QHash>
#include <QObject>

#include <optional>

class IrcController : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QAbstractItemModel* conversations READ conversations CONSTANT)
    Q_PROPERTY(QAbstractItemModel* messages READ messages CONSTANT)
    Q_PROPERTY(QAbstractItemModel* members READ members CONSTANT)
    Q_PROPERTY(QString selectedNetworkId READ selectedNetworkId NOTIFY selectionChanged)
    Q_PROPERTY(QString selectedTarget READ selectedTarget NOTIFY selectionChanged)
    Q_PROPERTY(QString topic READ topic NOTIFY selectionChanged)
    Q_PROPERTY(bool isChannel READ isChannel NOTIFY selectionChanged)
    Q_PROPERTY(int peopleCount READ peopleCount NOTIFY selectionChanged)
    Q_PROPERTY(QString connectionStatus READ connectionStatus NOTIFY statusChanged)
    Q_PROPERTY(QString lastError READ lastError NOTIFY statusChanged)
    Q_PROPERTY(QString currentNick READ currentNick NOTIFY selectionChanged)
    Q_PROPERTY(bool hasAwayPresence READ hasAwayPresence NOTIFY capabilitiesChanged)
    Q_PROPERTY(bool hasMemberStatus READ hasMemberStatus NOTIFY capabilitiesChanged)
    Q_PROPERTY(IrcStatusConsole* console READ console CONSTANT)

public:
    explicit IrcController(QObject *parent = nullptr);

    IrcSession *addSession(const IrcSessionConfig& config,
                           IrcTransport *transport,
                           IrcReconnectTimer *reconnectTimer = nullptr);
    bool discardSession(const QString &networkId);

    QAbstractItemModel *conversations();
    QAbstractItemModel *messages();
    QAbstractItemModel *members();
    QString selectedNetworkId() const;
    QString selectedTarget() const;
    QString topic() const;
    bool isChannel() const;
    int peopleCount() const;
    QString connectionStatus() const;
    QString lastError() const;
    QString currentNick() const;

    bool hasAwayPresence() const;
    bool hasMemberStatus() const;
    IrcStatusConsole *console();

    Q_INVOKABLE bool start(const QString& networkId);
    Q_INVOKABLE void selectConversation(const QString& networkId,
                                        const QString& target);
    Q_INVOKABLE void openDirectMessage(const QString& nick);
    Q_INVOKABLE bool sendMessage(const QString& text);

signals:
    void selectionChanged();
    void statusChanged();

    void capabilitiesChanged();
    void errorOccurred(const QString &networkId,
                       IrcSession::ErrorKind kind,
                       const QString &message);

private:
    void apply(const IrcEvent& event);
    void handleCapabilities(const QString& networkId,
                            IrcCapabilitySet capabilities);
    void echoLocal(IrcMessageKind kind, const QString& body);
    void handleMessage(const QString& networkId, const IrcMessage& message);
    void reloadModels();
    bool report(IrcCommandOutcome outcome, const IrcCommand& command);
    IrcSession *selectedSession() const;
    void updateStatus(IrcSession *session);

    IrcSessionManager m_sessions;
    IrcStatusConsole m_console;
    IrcEventReducer m_reducer;
    ConversationListModel m_conversations;
    MessageListModel m_messages;
    MemberListModel m_members;
    QHash<QString, QString> m_currentNicks;
    QHash<QString, IrcCapabilitySet> m_capabilities;
    std::optional<IrcConversationKey> m_selected;
    QString m_selectedTarget;
    QString m_connectionStatus = QStringLiteral("Offline");
    QString m_lastError;
};
