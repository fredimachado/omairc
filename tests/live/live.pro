QT += core network testlib
QT -= gui

macx {
    CONFIG -= app_bundle
}
include($$PWD/../../packaging/macos/sdk-compat.pri)

CONFIG += c++17 console testcase
TEMPLATE = app
TARGET = live_tests
include($$PWD/../../version.pri)

greaterThan(QMAKE_GCC_MAJOR_VERSION, 15): QMAKE_CXXFLAGS += -Wno-sfinae-incomplete

INCLUDEPATH += ../../src/irc ../support
DEFINES += LIVE_CERT_DIR=\\\"$$PWD/certs\\\"

HEADERS += \
    ../../src/irc/ircmessage.h \
    ../../src/irc/ircparser.h \
    ../../src/irc/ircframer.h \
    ../../src/irc/ircwiretext.h \
    ../../src/irc/irctextformatter.h \
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
    ../../src/irc/ircmonitor.h \
    ../../src/irc/ircmute.h \
    ../../src/irc/ircopendirect.h \
    ../../src/irc/irchighlight.h \
    ../../src/irc/ircautoaway.h \
    ../../src/irc/ircpref.h \
    ../../src/irc/ircinbox.h \
    ../../src/irc/ircinboxmodel.h \
    ../../src/irc/ircevent.h \
    ../../src/irc/ircviewnotify.h \
    ../../src/irc/irceventtranslator.h \
    ../../src/irc/irceventreducer.h \
    ../../src/irc/irctransport.h \
    ../../src/irc/ircstatusentry.h \
    ../../src/irc/ircservicenick.h \
    ../../src/irc/ircprefixnick.h \
    ../../src/irc/ircsecretpolicy.h \
    ../../src/irc/ircconversationlog.h \
    ../../src/irc/ircnetworklog.h \
    ../../src/irc/networklogmodel.h \
    ../../src/irc/irccommand.h \
    ../../src/irc/ircavatarurl.h \
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
    ../../src/irc/channellistmodel.h \
    ../../src/irc/irccontroller.h \
    ../../src/irc/ircnetworkprofile.h \
    ../../src/irc/ircprofilestore.h \
    ../../src/irc/qtirctransport.h \
    liveharness.h \
    livepeer.h

SOURCES += \
    tst_main.cpp \
    tst_live.cpp \
    tst_live_members.cpp \
    tst_live_reconnect.cpp \
    ../support/testsettings.cpp \
    liveharness.cpp \
    livepeer.cpp \
    ../../src/irc/ircparser.cpp \
    ../../src/irc/ircframer.cpp \
    ../../src/irc/ircwiretext.cpp \
    ../../src/irc/irctextformatter.cpp \
    ../../src/irc/irccommandbuilder.cpp \
    ../../src/irc/irccasemapping.cpp \
    ../../src/irc/ircserverfeatures.cpp \
    ../../src/irc/irccapability.cpp \
    ../../src/irc/irccapabilitynegotiation.cpp \
    ../../src/omaircpaths.cpp \
    ../../src/irc/ircsts.cpp \
    ../../src/irc/irctyping.cpp \
    ../../src/irc/irctypingpublisher.cpp \
    ../../src/irc/ircpresence.cpp \
    ../../src/irc/irctcp.cpp \
    ../../src/irc/ircignore.cpp \
    ../../src/irc/ircmonitor.cpp \
    ../../src/irc/ircmute.cpp \
    ../../src/irc/ircopendirect.cpp \
    ../../src/irc/irchighlight.cpp \
    ../../src/irc/ircautoaway.cpp \
    ../../src/irc/ircpref.cpp \
    ../../src/irc/ircinbox.cpp \
    ../../src/irc/ircinboxmodel.cpp \
    ../../src/irc/irceventtranslator.cpp \
    ../../src/irc/irceventreducer.cpp \
    ../../src/irc/ircstatusentry.cpp \
    ../../src/irc/ircservicenick.cpp \
    ../../src/irc/ircprefixnick.cpp \
    ../../src/irc/ircsecretpolicy.cpp \
    ../../src/irc/ircconversationlog.cpp \
    ../../src/irc/ircnetworklog.cpp \
    ../../src/irc/networklogmodel.cpp \
    ../../src/irc/irccommand.cpp \
    ../../src/irc/ircavatarurl.cpp \
    ../../src/irc/ircchannelmode.cpp \
    ../../src/irc/ircjointarget.cpp \
    ../../src/irc/ircslashcomplete.cpp \
    ../../src/irc/ircstatusconsole.cpp \
    ../../src/irc/ircsession.cpp \
    ../../src/irc/ircsessionmanager.cpp \
    ../../src/irc/conversationlistmodel.cpp \
    ../../src/irc/messagelistmodel.cpp \
    ../../src/irc/memberlistmodel.cpp \
    ../../src/irc/channellistmodel.cpp \
    ../../src/irc/irccontroller.cpp \
    ../../src/irc/ircnetworkprofile.cpp \
    ../../src/irc/ircprofilestore.cpp \
    ../../src/irc/qtirctransport.cpp
