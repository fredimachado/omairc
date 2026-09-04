#include "irccommand.h"

IrcCommand IrcCommand::parse(const QString& input)
{
    IrcCommand command;
    const QString trimmed = input.trimmed();
    if (trimmed.isEmpty()) {
        command.verb = Verb::Empty;
        return command;
    }
    if (!trimmed.startsWith(QLatin1Char('/'))) {
        command.verb = Verb::Say;
        command.argument = trimmed;
        return command;
    }
    if (trimmed.startsWith(QStringLiteral("//"))) {
        command.verb = Verb::Say;
        command.argument = trimmed.mid(1);
        return command;
    }

    const int space = trimmed.indexOf(QLatin1Char(' '));
    command.name = space < 0 ? trimmed : trimmed.left(space);
    command.argument = space < 0 ? QString() : trimmed.mid(space + 1).trimmed();

    const QString verb = command.name.mid(1).toLower();
    if (verb == QLatin1String("me"))
        command.verb = Verb::Action;
    else if (verb == QLatin1String("join"))
        command.verb = Verb::Join;
    else if (verb == QLatin1String("part"))
        command.verb = Verb::Part;
    else if (verb == QLatin1String("nick"))
        command.verb = Verb::Nick;
    else if (verb == QLatin1String("quit"))
        command.verb = Verb::Quit;
    else if (verb == QLatin1String("clear"))
        command.verb = Verb::Clear;
    else
        command.verb = Verb::Unknown;
    return command;
}

bool IrcCommand::needsConversation() const
{
    return verb == Verb::Say || verb == Verb::Action;
}

QString ircCommandOutcomeText(IrcCommandOutcome outcome, const IrcCommand& command)
{
    switch (outcome) {
    case IrcCommandOutcome::Sent:
        return {};
    case IrcCommandOutcome::NotConnected:
        return QStringLiteral("Not connected");
    case IrcCommandOutcome::Refused:
        return QStringLiteral("Command was refused");
    case IrcCommandOutcome::Unsupported:
        return command.name.isEmpty()
            ? QStringLiteral("That command is not supported")
            : QStringLiteral("Unknown command: %1").arg(command.name);
    case IrcCommandOutcome::WrongScope:
        return QStringLiteral("Select a connected conversation first");
    }
    return QStringLiteral("That command is not supported");
}
