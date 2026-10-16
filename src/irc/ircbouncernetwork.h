#pragma once

#include "ircmessage.h"
#include "ircparser.h"

#include <QList>
#include <QString>

#include <optional>

enum class IrcBouncerNetworkState
{
    Disconnected,
    Connecting,
    Connected,
};

// One upstream network. tls defaults to on and port follows tls (6697 or
// 6667) until the server names them. pass is kept for the connection and
// must not be copied into Status.
struct IrcBouncerNetwork
{
    QString id;
    QString name;
    QString host;
    int port = 6697;
    bool tls = true;
    QString nickname;
    QString username;
    QString realname;
    QString pass;
    IrcBouncerNetworkState state = IrcBouncerNetworkState::Disconnected;
    QString error;
    bool portExplicit = false;
    bool tlsExplicit = false;
    bool stateSet = false;
};

struct IrcBouncerAttributeValue
{
    QString key;
    QString value;
};

struct IrcBouncerFailure
{
    enum class Code
    {
        UnknownCommand,
        AccountRequired,
        RegistrationIsCompleted,
        InvalidNetId,
        InvalidAttribute,
        ReadOnlyAttribute,
        UnknownAttribute,
        NeedAttribute,
    };

    Code code = Code::UnknownCommand;
    QString codeName;
    QString subcommand;
    QString netId;
    QString attribute;
    QString description;
};

struct IrcBouncerCommandResult
{
    QString subcommand;
    bool ok = false;
    QString netId;
    QString code;
    QString attribute;
    QString description;
};

class IrcBouncerNetworkTable
{
public:
    void clear();
    void apply(const IrcParsedBouncer& command);
    void replaceSnapshot(const QList<IrcParsedBouncer>& lines);
    QList<IrcBouncerNetwork> list() const;

private:
    QList<IrcBouncerNetwork> m_networks;
};

std::optional<IrcBouncerFailure> parseBouncerFailure(const IrcMessage& message);

QString ircBouncerListCommand();
std::optional<QString> ircBouncerAddCommand(
    const QList<IrcBouncerAttributeValue>& attributes);
std::optional<QString> ircBouncerChangeCommand(
    const QString& netId, const QList<IrcBouncerAttributeValue>& attributes);
std::optional<QString> ircBouncerDeleteCommand(const QString& netId);
std::optional<QString> ircBouncerBindCommand(const QString& netId);
