#pragma once

#include <QString>
#include <QStringList>
#include <QVector>

enum class IrcComposerSurface
{
    Conversation,
    Status,
};

enum class IrcVerbScope
{
    Conversation,
    Status,
    Either,
};

struct IrcCommand
{
    enum class Verb { Empty, Say, Action, Join, Part, Nick, Quit, Clear, Close, Query, Msg, Topic, Notice, Away, Back, Whois, Mode, Kick, Unknown };
    Verb verb = Verb::Empty;
    QString name;
    QString argument;

    static IrcCommand parse(const QString& input);
    bool isLiveMessage() const;
    bool allowedOn(IrcComposerSurface surface) const;
};

struct IrcVerbSpec
{
    IrcCommand::Verb verb = IrcCommand::Verb::Unknown;
    QString name;
    QStringList aliases;
    QString usage;
    IrcVerbScope scope = IrcVerbScope::Either;
    QString wrongScopeText;

    bool allowedOn(IrcComposerSurface surface) const;
    QString wrongScopeMessage() const;
};

class IrcVerbTable
{
public:
    static const IrcVerbSpec *lookup(const QString& token);
    static const IrcVerbSpec *find(IrcCommand::Verb verb);
    static const QVector<IrcVerbSpec>& all();
    static QVector<IrcVerbSpec> visibleOn(IrcComposerSurface surface);
};

enum class IrcCommandOutcome { Sent, NotConnected, Refused, Unsupported, WrongScope };
QString ircCommandOutcomeText(IrcCommandOutcome outcome, const IrcCommand& command);
