#pragma once

#include "omaircipc.h"

#include <QString>
#include <QStringList>

#include <variant>

class QCoreApplication;

namespace OmaircCli {

enum class CommandId { Help, Connections, Status, Send, Raise };

enum class HelpScope { Overview, Command };

struct HelpTopic {
    HelpScope scope = HelpScope::Overview;
    CommandId command = CommandId::Help;
};

struct VersionRequest {};

struct CliError {
    QString message;
};

using ParseOutcome = std::variant<HelpTopic, VersionRequest, OmaircIpc::Request, CliError>;

bool looksLikeCommand(int argc, char **argv);
ParseOutcome parseArgs(const QStringList &args);
QString formatHelp(const HelpTopic &topic);
QString formatVersion();
int printOutcome(const ParseOutcome &outcome);
int runRequest(QCoreApplication &app, const OmaircIpc::Request &request);

}
