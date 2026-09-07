#pragma once

#include "conversationlistmodel.h"
#include "irceventreducer.h"
#include "ircsessionmanager.h"
#include "ircstatusconsole.h"
#include "memberlistmodel.h"
#include "messagelistmodel.h"

#include <QHash>
#include <QObject>
#include <QSet>
#include <QStringList>
#include <QTimer>

#include <optional>

struct IrcViewNotify;

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
    Q_PROPERTY(bool selfAway READ selfAway NOTIFY selfAwayChanged)
    Q_PROPERTY(bool hasAwayPresence READ hasAwayPresence NOTIFY capabilitiesChanged)
    Q_PROPERTY(bool hasMemberStatus READ hasMemberStatus NOTIFY capabilitiesChanged)
    Q_PROPERTY(bool hasTyping READ hasTyping NOTIFY capabilitiesChanged)
    Q_PROPERTY(QStringList typingNicks READ typingNicks NOTIFY typingChanged)
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
    bool selfAway() const;

    bool hasAwayPresence() const;
    bool hasMemberStatus() const;
    bool hasTyping() const;
    QStringList typingNicks() const;
    IrcStatusConsole *console();
    const IrcServerFeatures &serverFeatures(const QString &networkId) const;

    Q_INVOKABLE bool start(const QString& networkId);
    Q_INVOKABLE void selectConversation(const QString& networkId,
                                        const QString& target);
    Q_INVOKABLE void openDirectMessage(const QString& nick);
    Q_INVOKABLE void closeDirectMessage();
    Q_INVOKABLE bool sendMessage(const QString& text);
    Q_INVOKABLE bool nickIsTyping(const QString& nick) const;
    Q_INVOKABLE void notifyComposerText(const QString& text);

    QStringList networkIds() const;
    IrcSession *session(const QString &networkId) const;
    bool sendToTarget(const QString &networkId,
                      const QString &target,
                      const QString &text);

signals:
    void selectionChanged();
    void statusChanged();
    void selfAwayChanged();

    void capabilitiesChanged();
    void typingChanged();
    void errorOccurred(const QString &networkId,
                       IrcSession::ErrorKind kind,
                       const QString &message);

private:
    enum class QuietWire { Privmsg, Notice };
    enum class QuietTarget { Nick, Any };
    struct QuietSend {
        QuietWire wire;
        QuietTarget target;
    };
    static std::optional<QuietSend> quietSendFor(IrcCommand::Verb verb);

    void apply(const IrcEvent& event);
    void adoptReducerSelection();
    void publish(const IrcViewNotify& notify);
    void handleCapabilities(const QString& networkId,
                            IrcCapabilitySet capabilities);
    void echoLocal(IrcMessageKind kind, const QString& body);
    void handleMessage(const QString& networkId, const IrcMessage& message);
    void reloadModels();
    IrcCommandOutcome dispatch(const IrcCommand& command,
                               IrcComposerSurface surface);
    QString queryNetworkId(IrcComposerSurface surface) const;
    IrcCommandOutcome sendSelectedMessage(const QString& body);
    IrcCommandOutcome setSelectedTopic(const QString& topic);
    IrcCommandOutcome dispatchQuery(const IrcCommand& command,
                                    IrcComposerSurface surface);
    IrcCommandOutcome dispatchQuietSend(const IrcCommand& command,
                                        IrcComposerSurface surface);
    IrcCommandOutcome dispatchMode(const IrcCommand& command,
                                   IrcComposerSurface surface);
    IrcCommandOutcome dispatchWhois(const IrcCommand& command,
                                    IrcComposerSurface surface);
    void echoIfPresent(IrcSession *session,
                       const QString& target,
                       const QString& body,
                       QuietWire wire);
    void unawayAfterChat(IrcSession *session);
    IrcCommandOutcome clearSurface(IrcComposerSurface surface);
    bool report(IrcCommandOutcome outcome, const IrcCommand& command);
    bool selectedIsCloseableDirect() const;
    void dropSelectedDirectAndReselect();
    void clearConversationSelection();
    IrcSession *selectedSession() const;
    QString identityNetworkId() const;
    void notifySelfAwayIfChanged(const QString& previousId, bool previousAway);
    void updateStatus(IrcSession *session);
    void armTypingRefresh();

    IrcSessionManager m_sessions;
    IrcStatusConsole m_console;
    IrcEventReducer m_reducer;
    ConversationListModel m_conversations;
    MessageListModel m_messages;
    MemberListModel m_members;
    QHash<QString, QString> m_currentNicks;
    QHash<QString, IrcCapabilitySet> m_capabilities;
    QSet<QString> m_unawaySent;
    std::optional<IrcConversationKey> m_selected;
    QString m_selectedTarget;
    QString m_connectionStatus = QStringLiteral("Offline");
    QString m_lastError;
    QTimer m_typingRefresh;
    QString m_composerDraft;
    QString m_typingTarget;
};
