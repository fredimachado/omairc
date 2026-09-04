QT += core gui qml quick quickcontrols2 dbus network

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
    src/irc/ircevent.h \
    src/irc/irceventtranslator.h \
    src/irc/irceventreducer.h \
    src/irc/irctransport.h \
    src/irc/qtirctransport.h \
    src/irc/ircsession.h \
    src/irc/ircsessionmanager.h \
    src/irc/conversationlistmodel.h \
    src/irc/messagelistmodel.h \
    src/irc/memberlistmodel.h \
    src/irc/irccontroller.h \
    src/irc/ircnetworkprofile.h \
    src/irc/ircprofilestore.h

SOURCES += \
    src/main.cpp \
    src/backend.cpp \
    src/systemtheme.cpp \
    src/irc/ircparser.cpp \
    src/irc/ircframer.cpp \
    src/irc/irccommandbuilder.cpp \
    src/irc/irccasemapping.cpp \
    src/irc/ircserverfeatures.cpp \
    src/irc/irceventtranslator.cpp \
    src/irc/irceventreducer.cpp \
    src/irc/qtirctransport.cpp \
    src/irc/ircsession.cpp \
    src/irc/ircsessionmanager.cpp \
    src/irc/conversationlistmodel.cpp \
    src/irc/messagelistmodel.cpp \
    src/irc/memberlistmodel.cpp \
    src/irc/irccontroller.cpp \
    src/irc/ircnetworkprofile.cpp \
    src/irc/ircprofilestore.cpp

RESOURCES += src/resources.qrc
