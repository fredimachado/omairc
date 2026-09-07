#pragma once

#include <QString>

class QCoreApplication;

namespace OmaircCli {

bool looksLikeCommand(int argc, char **argv);
int run(QCoreApplication &app);

} // namespace OmaircCli
