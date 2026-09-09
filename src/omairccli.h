#pragma once

#include "omaircipc.h"

#include <QByteArray>

#include <variant>

namespace OmaircCli {

struct GuiLaunch {
    bool mockMode = false;
};

struct TerminalExit {
    QByteArray standardOutput;
    QByteArray standardError;
    int code = 0;
};

struct ControlRequest {
    OmaircIpc::Request request;
};

using StartupPlan = std::variant<GuiLaunch, TerminalExit, ControlRequest>;

StartupPlan plan(int argc, char *const argv[], const char *version);
int execute(const ControlRequest &control);

}
