QT += core gui network quick testlib
unix:!macx: QT += dbus

macx {
    CONFIG -= app_bundle
    HEADERS += ../src/macosnotifications.h
    OBJECTIVE_SOURCES += ../src/macosnotifications.mm
    LIBS += -framework UserNotifications -framework Foundation -framework AppKit
    include($$PWD/../packaging/macos/objc-arc.pri)
}

include(../qtkeychain.pri)

CONFIG += c++17 console testcase
TEMPLATE = app
TARGET = protocol_tests

# GCC 16 emits this diagnostic from Qt 6.11's own headers.
greaterThan(QMAKE_GCC_MAJOR_VERSION, 15): QMAKE_CXXFLAGS += -Wno-sfinae-incomplete

INCLUDEPATH += ../src ../src/irc support
include($$PWD/../version.pri)
# $$PWD already uses forward slashes on current Qt kits; do not $$replace(..., \\, /)
# here — that regex clears the path on win32-g++ qmake 3.1.
DEFINES += TEST_CERT_DIR=\\\"$$PWD/support/certs\\\"
DEFINES += TEST_CORPUS_DIR=\\\"$$PWD/protocol/corpus\\\"
DEFINES += OMAIRC_TEST

HEADERS += \
    ../src/omaircpaths.h \
    ../src/singleinstance.h \
    ../src/omaircipc.h \
    ../src/omaircipchandler.h \
    ../src/omairccli.h \
    ../src/omaircclipcursor.h \
    ../src/omaircfilelog.h \
    ../src/omaircupdatecheck.h \
    ../src/backend.h \
    ../src/linuxsessionbus.h \
    ../src/irc/ircmessage.h \
    ../src/irc/ircparser.h \
    ../src/irc/ircframer.h \
    ../src/irc/ircwiretext.h \
    ../src/irc/irctextformatter.h \
    ../src/irc/irccommandbuilder.h \
    ../src/irc/irccasemapping.h \
    ../src/irc/ircserverfeatures.h \
    ../src/irc/irccapability.h \
    ../src/irc/irccapabilitynegotiation.h \
    ../src/irc/ircsaslscram.h \
    ../src/irc/ircsts.h \
    ../src/irc/irctyping.h \
    ../src/irc/irctypingpublisher.h \
    ../src/irc/ircpresence.h \
    ../src/irc/ircavatarurl.h \
    ../src/irc/ircavatarhttp.h \
    ../src/irc/ircavatarstore.h \
    ../src/irc/irctcp.h \
    ../src/irc/ircignore.h \
    ../src/irc/ircmonitor.h \
    ../src/irc/ircmute.h \
    ../src/irc/ircopendirect.h \
    ../src/irc/ircplaybacktime.h \
    ../src/irc/irchighlight.h \
    ../src/irc/ircautoaway.h \
    ../src/irc/ircpref.h \
    ../src/irc/ircinbox.h \
    ../src/irc/ircinboxmodel.h \
    ../src/irc/ircevent.h \
    ../src/irc/ircviewnotify.h \
    ../src/irc/irceventtranslator.h \
    ../src/irc/irceventreducer.h \
    ../src/irc/irctransport.h \
    ../src/irc/ircstatusentry.h \
    ../src/irc/ircservicenick.h \
    ../src/irc/ircprefixnick.h \
    ../src/irc/ircsecretpolicy.h \
    ../src/irc/ircstoragepath.h \
    ../src/irc/ircconversationlog.h \
    ../src/irc/ircnetworklog.h \
    ../src/irc/networklogmodel.h \
    ../src/irc/irccommand.h \
    ../src/irc/ircchannelmode.h \
    ../src/irc/ircjointarget.h \
    ../src/irc/ircslashcomplete.h \
    ../src/irc/ircstatusconsole.h \
    ../src/irc/irchistorybatch.h \
    ../src/irc/ircsession.h \
    ../src/irc/ircsessionmanager.h \
    ../src/irc/conversationlistmodel.h \
    ../src/irc/messagelistmodel.h \
    ../src/irc/memberlistmodel.h \
    ../src/irc/channellistmodel.h \
    ../src/irc/ircchannellistrequest.h \
    ../src/irc/irccontroller.h \
    ../src/irc/ircloopbacktransport.h \
    ../src/irc/ircdemoserver.h \
    ../src/irc/ircnetworkprofile.h \
    ../src/irc/ircprofilestore.h \
    ../src/storage/credentialstore.h \
    ../src/storage/secretservicecredentialstore.h \
    ../src/irc/ircconnection.h \
    ../src/irc/qtirctransport.h \
    support/fakeirctransport.h \
    support/testsettings.h

SOURCES += \
    tst_main.cpp \
    tst_singleinstance.cpp \
    tst_omaircipc.cpp \
    tst_omairccli.cpp \
    tst_omaircfilelog.cpp \
    tst_omaircpaths.cpp \
    tst_irctextformatter.cpp \
    tst_omaircupdatecheck.cpp \
    tst_backend.cpp \
    ../src/omaircpaths.cpp \
    ../src/singleinstance.cpp \
    ../src/omaircipc.cpp \
    ../src/omaircipchandler.cpp \
    ../src/omairccli.cpp \
    ../src/omaircclipcursor.cpp \
    ../src/omaircfilelog.cpp \
    ../src/omaircupdatecheck.cpp \
    ../src/backend.cpp \
    protocol/tst_protocol.cpp \
    protocol/tst_casemapping.cpp \
    protocol/tst_corpus.cpp \
    ../src/irc/ircparser.cpp \
    ../src/irc/ircframer.cpp \
    ../src/irc/ircwiretext.cpp \
    ../src/irc/irctextformatter.cpp \
    ../src/irc/irccommandbuilder.cpp \
    ../src/irc/irccasemapping.cpp \
    ../src/irc/ircserverfeatures.cpp \
    ../src/irc/irccapability.cpp \
    ../src/irc/irccapabilitynegotiation.cpp \
    ../src/irc/ircsaslscram.cpp \
    ../src/irc/ircsts.cpp \
    ../src/irc/irctyping.cpp \
    ../src/irc/irctypingpublisher.cpp \
    ../src/irc/ircpresence.cpp \
    ../src/irc/ircavatarurl.cpp \
    ../src/irc/ircavatarhttp.cpp \
    ../src/irc/ircavatarstore.cpp \
    ../src/irc/irctcp.cpp \
    ../src/irc/ircignore.cpp \
    ../src/irc/ircmonitor.cpp \
    ../src/irc/ircmute.cpp \
    ../src/irc/ircopendirect.cpp \
    ../src/irc/ircplaybacktime.cpp \
    ../src/irc/irchighlight.cpp \
    ../src/irc/ircautoaway.cpp \
    ../src/irc/ircpref.cpp \
    ../src/irc/ircinbox.cpp \
    ../src/irc/ircinboxmodel.cpp \
    ../src/irc/irceventtranslator.cpp \
    ../src/irc/irceventreducer.cpp \
    ../src/irc/ircstatusentry.cpp \
    ../src/irc/ircservicenick.cpp \
    ../src/irc/ircprefixnick.cpp \
    ../src/irc/ircsecretpolicy.cpp \
    ../src/irc/ircstoragepath.cpp \
    ../src/irc/ircconversationlog.cpp \
    ../src/irc/ircnetworklog.cpp \
    ../src/irc/networklogmodel.cpp \
    ../src/irc/irccommand.cpp \
    ../src/irc/ircchannelmode.cpp \
    ../src/irc/ircjointarget.cpp \
    ../src/irc/ircslashcomplete.cpp \
    ../src/irc/ircstatusconsole.cpp \
    ../src/irc/ircsession.cpp \
    ../src/irc/ircsessionmanager.cpp \
    ../src/irc/conversationlistmodel.cpp \
    ../src/irc/messagelistmodel.cpp \
    ../src/irc/memberlistmodel.cpp \
    ../src/irc/channellistmodel.cpp \
    ../src/irc/ircchannellistrequest.cpp \
    ../src/irc/irccontroller.cpp \
    ../src/irc/ircloopbacktransport.cpp \
    ../src/irc/ircdemoserver.cpp \
    ../src/irc/ircnetworkprofile.cpp \
    ../src/irc/ircprofilestore.cpp \
    ../src/storage/secretservicecredentialstore.cpp \
    ../src/irc/ircconnection.cpp \
    ../src/irc/qtirctransport.cpp \
    support/fakeirctransport.cpp \
    support/testsettings.cpp \
    integration/tst_qtirctransport.cpp \
    session/tst_transport.cpp \
    session/tst_session.cpp \
    session/tst_demoserver.cpp \
    session/tst_capability.cpp \
    session/tst_scram.cpp \
    session/tst_sts.cpp \
    session/tst_profile.cpp \
    session/tst_connection.cpp \
    storage/tst_secretservice.cpp \
    session/tst_storagepath.cpp \
    session/tst_controller.cpp \
    session/tst_channellistrequest.cpp \
    session/tst_command.cpp \
    session/tst_ignore.cpp \
    session/tst_monitor.cpp \
    session/tst_mute.cpp \
    session/tst_opendirect.cpp \
    session/tst_highlight.cpp \
    session/tst_autoaway.cpp \
    session/tst_pref.cpp \
    session/tst_labeledresponse.cpp \
    session/tst_inbox.cpp \
    session/tst_typing.cpp \
    session/tst_reducer.cpp \
    session/tst_avatarurl.cpp \
    session/tst_avatarstore.cpp \
    models/tst_models.cpp

include($$PWD/../packaging/macos/sdk-compat.pri)
