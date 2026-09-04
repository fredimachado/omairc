QT += core network testlib
QT -= gui

CONFIG += c++17 console testcase
TEMPLATE = app
TARGET = protocol_tests

# GCC 16 emits this diagnostic from Qt 6.11's own headers.
greaterThan(QMAKE_GCC_MAJOR_VERSION, 15): QMAKE_CXXFLAGS += -Wno-sfinae-incomplete

INCLUDEPATH += ../src/irc support
DEFINES += TEST_CERT_DIR=\\\"$$PWD/support/certs\\\"

HEADERS += \
    ../src/irc/ircmessage.h \
    ../src/irc/ircparser.h \
    ../src/irc/ircframer.h \
    ../src/irc/irccommandbuilder.h \
    ../src/irc/irccasemapping.h \
    ../src/irc/ircserverfeatures.h \
    ../src/irc/ircevent.h \
    ../src/irc/irceventtranslator.h \
    ../src/irc/irceventreducer.h \
    ../src/irc/irctransport.h \
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
    protocol/tst_protocol.cpp \
    protocol/tst_casemapping.cpp \
    ../src/irc/ircparser.cpp \
    ../src/irc/ircframer.cpp \
    ../src/irc/irccommandbuilder.cpp \
    ../src/irc/irccasemapping.cpp \
    ../src/irc/ircserverfeatures.cpp \
    ../src/irc/irceventtranslator.cpp \
    ../src/irc/irceventreducer.cpp \
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
    session/tst_profile.cpp \
    session/tst_connection.cpp \
    session/tst_controller.cpp \
    session/tst_reducer.cpp \
    models/tst_models.cpp
