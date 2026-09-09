QT += core gui qml quick quickcontrols2 dbus network

CONFIG += c++17 release
VERSION = 0.1.0
TARGET = omairc
TEMPLATE = app
DEFINES += OMAIRC_VERSION=\\\"$$VERSION\\\"

isEmpty(PREFIX): PREFIX = /usr/local

HEADERS += \
    src/backend.h \
    src/singleinstance.h \
    src/omaircipc.h \
    src/omaircipchandler.h \
    src/omairccli.h \
    src/systemtheme.h \
    src/irc/ircmessage.h \
    src/irc/ircparser.h \
    src/irc/ircframer.h \
    src/irc/ircwiretext.h \
    src/irc/irccommandbuilder.h \
    src/irc/irccasemapping.h \
    src/irc/ircserverfeatures.h \
    src/irc/irccapability.h \
    src/irc/irccapabilitynegotiation.h \
    src/irc/irctyping.h \
    src/irc/irctypingpublisher.h \
    src/irc/ircpresence.h \
    src/irc/irctcp.h \
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
    src/irc/ircchannelmode.h \
    src/irc/ircjointarget.h \
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
    src/singleinstance.cpp \
    src/omaircipc.cpp \
    src/omaircipchandler.cpp \
    src/omairccli.cpp \
    src/systemtheme.cpp \
    src/irc/ircparser.cpp \
    src/irc/ircframer.cpp \
    src/irc/ircwiretext.cpp \
    src/irc/irccommandbuilder.cpp \
    src/irc/irccasemapping.cpp \
    src/irc/ircserverfeatures.cpp \
    src/irc/irccapability.cpp \
    src/irc/irccapabilitynegotiation.cpp \
    src/irc/irctyping.cpp \
    src/irc/irctypingpublisher.cpp \
    src/irc/ircpresence.cpp \
    src/irc/irctcp.cpp \
    src/irc/irceventtranslator.cpp \
    src/irc/irceventreducer.cpp \
    src/irc/qtirctransport.cpp \
    src/irc/ircstatusentry.cpp \
    src/irc/ircnetworklog.cpp \
    src/irc/networklogmodel.cpp \
    src/irc/irccommand.cpp \
    src/irc/ircchannelmode.cpp \
    src/irc/ircjointarget.cpp \
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

unix {
    target.path = $$PREFIX/bin
    INSTALLS += target

    desktop.path = $$PREFIX/share/applications
    desktop.files = $$PWD/data/omairc.desktop
    INSTALLS += desktop

    icon.path = $$PREFIX/share/icons/hicolor/scalable/apps
    icon.files = $$PWD/data/icons/hicolor/scalable/apps/omairc.svg
    INSTALLS += icon

    license_mit.path = $$PREFIX/share/licenses/omairc
    license_mit.files = $$PWD/LICENSE
    INSTALLS += license_mit

    license_ofl.path = $$PREFIX/share/licenses/omairc
    license_ofl.files = $$PWD/fonts/OFL.txt
    INSTALLS += license_ofl

    license_lgpl.path = $$PREFIX/share/licenses/omairc
    license_lgpl.extra = \
        $(MKDIR) $(INSTALL_ROOT)$$license_lgpl.path && \
        /usr/bin/install -m 644 -p $$PWD/src/irc/COPYING \
            $(INSTALL_ROOT)$$license_lgpl.path/COPYING-LGPL
    license_lgpl.uninstall = \
        $(DEL_FILE) $(INSTALL_ROOT)$$license_lgpl.path/COPYING-LGPL
    INSTALLS += license_lgpl

    bash_completion.path = $$PREFIX/share/bash-completion/completions
    bash_completion.files = $$PWD/data/bash-completion/omairc
    INSTALLS += bash_completion
}
