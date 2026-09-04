import QtQuick

OmaircWindow {
    backend: appBackend
    irc: ircController
    connection: ircConnection
    slashCommands: slashSession
}
