QT += core gui qml quick quickcontrols2 network
unix: QT += dbus

CONFIG += c++17 release
TARGET = omairc
TEMPLATE = app

HEADERS += \
    src/backend.h \
    src/systemtheme.h \
    src/irc/ircmessage.h \
    src/irc/ircparser.h \
    src/irc/ircframer.h \
    src/irc/irccommandbuilder.h \
    src/irc/irccasemapping.h \
    src/irc/ircserverfeatures.h \
    src/irc/irccapability.h \
    src/irc/irccapabilitynegotiation.h \
    src/irc/irctyping.h \
    src/irc/irctypingpublisher.h \
    src/irc/ircpresence.h \
    src/irc/ircevent.h \
    src/irc/ircviewnotify.h \
    src/irc/irceventtranslator.h \
    src/irc/irceventreducer.h \
    src/irc/irctransport.h \
    src/irc/qtirctransport.h \
    src/irc/ircstatusentry.h \
    src/irc/ircnetworklog.h \
    src/irc/networklogmodel.h \
    src/irc/irccommand.h \
    src/irc/ircslashcomplete.h \
    src/irc/ircstatusconsole.h \
    src/irc/ircsession.h \
    src/irc/ircsessionmanager.h \
    src/irc/conversationlistmodel.h \
    src/irc/messagelistmodel.h \
    src/irc/memberlistmodel.h \
    src/irc/irccontroller.h \
    src/irc/ircnetworkprofile.h \
    src/irc/ircprofilestore.h \
    src/irc/ircconnection.h

SOURCES += \
    src/main.cpp \
    src/backend.cpp \
    src/systemtheme.cpp \
    src/irc/ircparser.cpp \
    src/irc/ircframer.cpp \
    src/irc/irccommandbuilder.cpp \
    src/irc/irccasemapping.cpp \
    src/irc/ircserverfeatures.cpp \
    src/irc/irccapability.cpp \
    src/irc/irccapabilitynegotiation.cpp \
    src/irc/irctyping.cpp \
    src/irc/irctypingpublisher.cpp \
    src/irc/ircpresence.cpp \
    src/irc/irceventtranslator.cpp \
    src/irc/irceventreducer.cpp \
    src/irc/qtirctransport.cpp \
    src/irc/ircstatusentry.cpp \
    src/irc/ircnetworklog.cpp \
    src/irc/networklogmodel.cpp \
    src/irc/irccommand.cpp \
    src/irc/ircslashcomplete.cpp \
    src/irc/ircstatusconsole.cpp \
    src/irc/ircsession.cpp \
    src/irc/ircsessionmanager.cpp \
    src/irc/conversationlistmodel.cpp \
    src/irc/messagelistmodel.cpp \
    src/irc/memberlistmodel.cpp \
    src/irc/irccontroller.cpp \
    src/irc/ircnetworkprofile.cpp \
    src/irc/ircprofilestore.cpp \
    src/irc/ircconnection.cpp

RESOURCES += src/resources.qrc
