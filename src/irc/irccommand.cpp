#include "irccommand.h"

bool IrcVerbSpec::allowedOn(IrcComposerSurface surface) const
{
    switch (scope) {
    case IrcVerbScope::Conversation:
        return surface == IrcComposerSurface::Conversation;
    case IrcVerbScope::Status:
        return surface == IrcComposerSurface::Status;
    case IrcVerbScope::Either:
        return true;
    }
    return false;
}

QString IrcVerbSpec::wrongScopeMessage() const
{
    if (wrongScopeText.isEmpty())
        return QStringLiteral("Select a connected conversation first");
    return wrongScopeText;
}

const QVector<IrcVerbSpec>& IrcVerbTable::all()
{
    static const QVector<IrcVerbSpec> rows = {
        {IrcCommand::Verb::Action, QStringLiteral("me"), {},
         QStringLiteral("/me <text>"), IrcVerbScope::Conversation, {}},
        {IrcCommand::Verb::Join, QStringLiteral("join"), {QStringLiteral("j")},
         QStringLiteral("/join <channel>"), IrcVerbScope::Either, {}},
        {IrcCommand::Verb::Part, QStringLiteral("part"), {QStringLiteral("leave")},
         QStringLiteral("/part [channel]"), IrcVerbScope::Either,
         QStringLiteral("Part applies to channels")},
        {IrcCommand::Verb::Nick, QStringLiteral("nick"), {},
         QStringLiteral("/nick <nickname>"), IrcVerbScope::Either, {}},
        {IrcCommand::Verb::Quit, QStringLiteral("quit"), {},
         QStringLiteral("/quit [reason]"), IrcVerbScope::Either, {}},
        {IrcCommand::Verb::Clear, QStringLiteral("clear"), {},
         QStringLiteral("/clear"), IrcVerbScope::Either, {}},
        {IrcCommand::Verb::Close, QStringLiteral("close"), {},
         QStringLiteral("/close"), IrcVerbScope::Conversation,
         QStringLiteral("Close applies to direct messages")},
        {IrcCommand::Verb::Query, QStringLiteral("query"), {QStringLiteral("msg")},
         QStringLiteral("/query <nick> [text]"), IrcVerbScope::Either, {}},
        {IrcCommand::Verb::Topic, QStringLiteral("topic"), {},
         QStringLiteral("/topic [text]"), IrcVerbScope::Conversation,
         QStringLiteral("Topic applies to channels")},
        {IrcCommand::Verb::Notice, QStringLiteral("notice"), {},
         QStringLiteral("/notice <target> <text>"), IrcVerbScope::Either, {}},
        {IrcCommand::Verb::Away, QStringLiteral("away"), {},
         QStringLiteral("/away [reason]"), IrcVerbScope::Either, {}},
        {IrcCommand::Verb::Back, QStringLiteral("back"), {},
         QStringLiteral("/back"), IrcVerbScope::Either, {}},
    };
    return rows;
}

const IrcVerbSpec *IrcVerbTable::lookup(const QString& token)
{
    const QString folded = token.toLower();
    for (const IrcVerbSpec& row : all()) {
        if (row.name == folded)
            return &row;
        if (row.aliases.contains(folded))
            return &row;
    }
    return nullptr;
}

const IrcVerbSpec *IrcVerbTable::find(IrcCommand::Verb verb)
{
    if (verb == IrcCommand::Verb::Empty || verb == IrcCommand::Verb::Say
        || verb == IrcCommand::Verb::Unknown) {
        return nullptr;
    }
    for (const IrcVerbSpec& row : all()) {
        if (row.verb == verb)
            return &row;
    }
    return nullptr;
}

QVector<IrcVerbSpec> IrcVerbTable::visibleOn(IrcComposerSurface surface)
{
    QVector<IrcVerbSpec> rows;
    for (const IrcVerbSpec& row : all()) {
        if (row.allowedOn(surface))
            rows.append(row);
    }
    return rows;
}

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

    const IrcVerbSpec *spec = IrcVerbTable::lookup(command.name.mid(1));
    command.verb = spec ? spec->verb : Verb::Unknown;
    if (command.verb == Verb::Back)
        command.argument.clear();
    return command;
}

bool IrcCommand::isLiveMessage() const
{
    return verb == Verb::Say || verb == Verb::Action;
}

bool IrcCommand::allowedOn(IrcComposerSurface surface) const
{
    if (verb == Verb::Say)
        return surface == IrcComposerSurface::Conversation;
    const IrcVerbSpec *spec = IrcVerbTable::find(verb);
    return spec ? spec->allowedOn(surface) : true;
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
    case IrcCommandOutcome::WrongScope: {
        const IrcVerbSpec *spec = IrcVerbTable::find(command.verb);
        return spec ? spec->wrongScopeMessage()
                    : QStringLiteral("Select a connected conversation first");
    }
    }
    return QStringLiteral("That command is not supported");
}
