#pragma once

#include "omaircipc.h"

#include <QString>
#include <QStringList>

#include <optional>

class QCoreApplication;

namespace OmaircCli {

bool looksLikeCommand(int argc, char **argv);
int run(QCoreApplication &app);
std::optional<OmaircIpc::Request> parseArgs(const QStringList &args,
                                            QString &error);

} // namespace OmaircCli
