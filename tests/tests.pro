QT += core network testlib
QT -= gui

CONFIG += c++17 console testcase
TEMPLATE = app
TARGET = protocol_tests

# GCC 16 emits this diagnostic from Qt 6.11's own headers.
greaterThan(QMAKE_GCC_MAJOR_VERSION, 15): QMAKE_CXXFLAGS += -Wno-sfinae-incomplete

INCLUDEPATH += ../src ../src/irc support
DEFINES += TEST_CERT_DIR=\\\"$$PWD/support/certs\\\" OMAIRC_VERSION=\\\"0.1.0\\\"

HEADERS += \
    ../src/singleinstance.h \
    ../src/irc/ircmessage.h \
    ../src/irc/ircparser.h \
    ../src/irc/ircframer.h \
    ../src/irc/irccommandbuilder.h \
    ../src/irc/irccasemapping.h \
    ../src/irc/ircserverfeatures.h \
    ../src/irc/irccapability.h \
    ../src/irc/irccapabilitynegotiation.h \
    ../src/irc/irctyping.h \
    ../src/irc/irctypingpublisher.h \
    ../src/irc/ircpresence.h \
    ../src/irc/irctcp.h \
    ../src/irc/ircevent.h \
    ../src/irc/ircviewnotify.h \
    ../src/irc/irceventtranslator.h \
    ../src/irc/irceventreducer.h \
    ../src/irc/irctransport.h \
    ../src/irc/ircstatusentry.h \
    ../src/irc/ircnetworklog.h \
    ../src/irc/networklogmodel.h \
    ../src/irc/irccommand.h \
    ../src/irc/ircchannelmode.h \
    ../src/irc/ircslashcomplete.h \
    ../src/irc/ircstatusconsole.h \
    ../src/irc/ircsession.h \
    ../src/irc/ircsessionmanager.h \
    ../src/irc/conversationlistmodel.h \
    ../src/irc/messagelistmodel.h \
    ../src/irc/memberlistmodel.h \
    ../src/irc/irccontroller.h \
    ../src/irc/ircnetworkprofile.h \
    ../src/irc/ircprofilestore.h \
    ../src/irc/ircconnection.h \
    ../src/irc/qtirctransport.h \
    support/fakeirctransport.h

SOURCES += \
    tst_main.cpp \
    tst_singleinstance.cpp \
    ../src/singleinstance.cpp \
    protocol/tst_protocol.cpp \
    protocol/tst_casemapping.cpp \
    ../src/irc/ircparser.cpp \
    ../src/irc/ircframer.cpp \
    ../src/irc/irccommandbuilder.cpp \
    ../src/irc/irccasemapping.cpp \
    ../src/irc/ircserverfeatures.cpp \
    ../src/irc/irccapability.cpp \
    ../src/irc/irccapabilitynegotiation.cpp \
    ../src/irc/irctyping.cpp \
    ../src/irc/irctypingpublisher.cpp \
    ../src/irc/ircpresence.cpp \
    ../src/irc/irctcp.cpp \
    ../src/irc/irceventtranslator.cpp \
    ../src/irc/irceventreducer.cpp \
    ../src/irc/ircstatusentry.cpp \
    ../src/irc/ircnetworklog.cpp \
    ../src/irc/networklogmodel.cpp \
    ../src/irc/irccommand.cpp \
    ../src/irc/ircchannelmode.cpp \
    ../src/irc/ircslashcomplete.cpp \
    ../src/irc/ircstatusconsole.cpp \
    ../src/irc/ircsession.cpp \
    ../src/irc/ircsessionmanager.cpp \
    ../src/irc/conversationlistmodel.cpp \
    ../src/irc/messagelistmodel.cpp \
    ../src/irc/memberlistmodel.cpp \
    ../src/irc/irccontroller.cpp \
    ../src/irc/ircnetworkprofile.cpp \
    ../src/irc/ircprofilestore.cpp \
    ../src/irc/ircconnection.cpp \
    ../src/irc/qtirctransport.cpp \
    support/fakeirctransport.cpp \
    integration/tst_qtirctransport.cpp \
    session/tst_transport.cpp \
    session/tst_session.cpp \
    session/tst_capability.cpp \
    session/tst_profile.cpp \
    session/tst_connection.cpp \
    session/tst_controller.cpp \
    session/tst_command.cpp \
    session/tst_typing.cpp \
    session/tst_reducer.cpp \
    models/tst_models.cpp
