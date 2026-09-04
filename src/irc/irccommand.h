#pragma once

#include <QString>

struct IrcCommand
{
    enum class Verb { Empty, Say, Action, Join, Part, Nick, Quit, Clear, Unknown };
    Verb verb = Verb::Empty;
    QString name;
    QString argument;

    static IrcCommand parse(const QString& input);
    bool needsConversation() const;
};

enum class IrcCommandOutcome { Sent, NotConnected, Refused, Unsupported, WrongScope };
QString ircCommandOutcomeText(IrcCommandOutcome outcome, const IrcCommand& command);
