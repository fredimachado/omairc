QT += core network testlib
QT -= gui

CONFIG += c++17 console testcase
TEMPLATE = app
TARGET = live_tests
include($$PWD/../../version.pri)

greaterThan(QMAKE_GCC_MAJOR_VERSION, 15): QMAKE_CXXFLAGS += -Wno-sfinae-incomplete

INCLUDEPATH += ../../src/irc
DEFINES += LIVE_CERT_DIR=\\\"$$PWD/certs\\\"

HEADERS += \
    ../../src/irc/ircmessage.h \
    ../../src/irc/ircparser.h \
    ../../src/irc/ircframer.h \
    ../../src/irc/ircwiretext.h \
    ../../src/irc/irccommandbuilder.h \
    ../../src/irc/irccasemapping.h \
    ../../src/irc/ircserverfeatures.h \
    ../../src/irc/irccapability.h \
    ../../src/irc/irccapabilitynegotiation.h \
    ../../src/irc/ircsts.h \
    ../../src/irc/irctyping.h \
    ../../src/irc/irctypingpublisher.h \
    ../../src/irc/ircpresence.h \
    ../../src/irc/irctcp.h \
    ../../src/irc/ircignore.h \
    ../../src/irc/ircevent.h \
    ../../src/irc/ircviewnotify.h \
    ../../src/irc/irceventtranslator.h \
    ../../src/irc/irceventreducer.h \
    ../../src/irc/irctransport.h \
    ../../src/irc/ircstatusentry.h \
    ../../src/irc/ircservicenick.h \
    ../../src/irc/ircprefixnick.h \
    ../../src/irc/ircsecretpolicy.h \
    ../../src/irc/ircnetworklog.h \
    ../../src/irc/networklogmodel.h \
    ../../src/irc/irccommand.h \
    ../../src/irc/ircchannelmode.h \
    ../../src/irc/ircjointarget.h \
    ../../src/irc/ircslashcomplete.h \
    ../../src/irc/ircstatusconsole.h \
    ../../src/irc/irchistorybatch.h \
    ../../src/irc/ircsession.h \
    ../../src/irc/ircsessionmanager.h \
    ../../src/irc/conversationlistmodel.h \
    ../../src/irc/messagelistmodel.h \
    ../../src/irc/memberlistmodel.h \
    ../../src/irc/irccontroller.h \
    ../../src/irc/qtirctransport.h \
    liveharness.h \
    livepeer.h

SOURCES += \
    tst_main.cpp \
    tst_live.cpp \
    liveharness.cpp \
    livepeer.cpp \
    ../../src/irc/ircparser.cpp \
    ../../src/irc/ircframer.cpp \
    ../../src/irc/ircwiretext.cpp \
    ../../src/irc/irccommandbuilder.cpp \
    ../../src/irc/irccasemapping.cpp \
    ../../src/irc/ircserverfeatures.cpp \
    ../../src/irc/irccapability.cpp \
    ../../src/irc/irccapabilitynegotiation.cpp \
    ../../src/irc/ircsts.cpp \
    ../../src/irc/irctyping.cpp \
    ../../src/irc/irctypingpublisher.cpp \
    ../../src/irc/ircpresence.cpp \
    ../../src/irc/irctcp.cpp \
    ../../src/irc/ircignore.cpp \
    ../../src/irc/irceventtranslator.cpp \
    ../../src/irc/irceventreducer.cpp \
    ../../src/irc/ircstatusentry.cpp \
    ../../src/irc/ircservicenick.cpp \
    ../../src/irc/ircprefixnick.cpp \
    ../../src/irc/ircsecretpolicy.cpp \
    ../../src/irc/ircnetworklog.cpp \
    ../../src/irc/networklogmodel.cpp \
    ../../src/irc/irccommand.cpp \
    ../../src/irc/ircchannelmode.cpp \
    ../../src/irc/ircjointarget.cpp \
    ../../src/irc/ircslashcomplete.cpp \
    ../../src/irc/ircstatusconsole.cpp \
    ../../src/irc/ircsession.cpp \
    ../../src/irc/ircsessionmanager.cpp \
    ../../src/irc/conversationlistmodel.cpp \
    ../../src/irc/messagelistmodel.cpp \
    ../../src/irc/memberlistmodel.cpp \
    ../../src/irc/irccontroller.cpp \
    ../../src/irc/qtirctransport.cpp
