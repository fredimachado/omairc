QT += core gui qml quick quickcontrols2 network
unix:!macx: QT += dbus

include(qtkeychain.pri)

CONFIG += c++17 release
TARGET = omairc
TEMPLATE = app

# qmake's Windows default dumps .obj and moc/qrc sources next to the exe.
win32 {
    OBJECTS_DIR = $$OUT_PWD/.obj
    MOC_DIR = $$OUT_PWD/.moc
    RCC_DIR = $$OUT_PWD/.rcc
    RC_ICONS = $$PWD/data/icons/omairc.ico
}

macx {
    OBJECTS_DIR = $$OUT_PWD/.obj
    MOC_DIR = $$OUT_PWD/.moc
    RCC_DIR = $$OUT_PWD/.rcc
    CONFIG += app_bundle

    exists($$PWD/data/icons/omairc.icns) {
        ICON = $$PWD/data/icons/omairc.icns
    }
    exists($$PWD/packaging/macos/Info.plist) {
        QMAKE_INFO_PLIST = $$PWD/packaging/macos/Info.plist
    }

    HEADERS += src/macosnotifications.h
    OBJECTIVE_SOURCES += src/macosnotifications.mm
    LIBS += -framework UserNotifications -framework Foundation -framework AppKit
    include($$PWD/packaging/macos/objc-arc.pri)
}

include($$PWD/version.pri)

# GCC 16 emits this diagnostic from Qt 6.11's own headers.
greaterThan(QMAKE_GCC_MAJOR_VERSION, 15): QMAKE_CXXFLAGS += -Wno-sfinae-incomplete

isEmpty(PREFIX): PREFIX = /usr/local

HEADERS += \
    src/omaircpaths.h \
    src/backend.h \
    src/singleinstance.h \
    src/omaircipc.h \
    src/omaircipchandler.h \
    src/omairccli.h \
    src/omaircclipcursor.h \
    src/omaircfilelog.h \
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
    src/irc/ircsts.h \
    src/irc/irctyping.h \
    src/irc/irctypingpublisher.h \
    src/irc/ircpresence.h \
    src/irc/ircavatarurl.h \
    src/irc/ircavatarstore.h \
    src/irc/irctcp.h \
    src/irc/ircignore.h \
    src/irc/ircmute.h \
    src/irc/ircopendirect.h \
    src/irc/irchighlight.h \
    src/irc/ircevent.h \
    src/irc/ircviewnotify.h \
    src/irc/irceventtranslator.h \
    src/irc/irceventreducer.h \
    src/irc/irctransport.h \
    src/irc/ircloopbacktransport.h \
    src/irc/ircdemoserver.h \
    src/irc/qtirctransport.h \
    src/irc/ircstatusentry.h \
    src/irc/ircservicenick.h \
    src/irc/ircprefixnick.h \
    src/irc/ircsecretpolicy.h \
    src/irc/ircconversationlog.h \
    src/irc/ircnetworklog.h \
    src/irc/networklogmodel.h \
    src/irc/irccommand.h \
    src/irc/ircchannelmode.h \
    src/irc/ircjointarget.h \
    src/irc/ircslashcomplete.h \
    src/irc/ircstatusconsole.h \
    src/irc/irchistorybatch.h \
    src/irc/ircsession.h \
    src/irc/ircsessionmanager.h \
    src/irc/conversationlistmodel.h \
    src/irc/messagelistmodel.h \
    src/irc/memberlistmodel.h \
    src/irc/irccontroller.h \
    src/irc/ircnetworkprofile.h \
    src/irc/ircprofilestore.h \
    src/storage/credentialstore.h \
    src/storage/secretservicecredentialstore.h \
    src/irc/ircconnection.h

SOURCES += \
    src/main.cpp \
    src/omaircpaths.cpp \
    src/backend.cpp \
    src/singleinstance.cpp \
    src/omaircipc.cpp \
    src/omaircipchandler.cpp \
    src/omairccli.cpp \
    src/omaircclipcursor.cpp \
    src/omaircfilelog.cpp \
    src/systemtheme.cpp \
    src/irc/ircparser.cpp \
    src/irc/ircframer.cpp \
    src/irc/ircwiretext.cpp \
    src/irc/irccommandbuilder.cpp \
    src/irc/irccasemapping.cpp \
    src/irc/ircserverfeatures.cpp \
    src/irc/irccapability.cpp \
    src/irc/irccapabilitynegotiation.cpp \
    src/irc/ircsts.cpp \
    src/irc/irctyping.cpp \
    src/irc/irctypingpublisher.cpp \
    src/irc/ircpresence.cpp \
    src/irc/ircavatarurl.cpp \
    src/irc/ircavatarstore.cpp \
    src/irc/irctcp.cpp \
    src/irc/ircignore.cpp \
    src/irc/ircmute.cpp \
    src/irc/ircopendirect.cpp \
    src/irc/irchighlight.cpp \
    src/irc/irceventtranslator.cpp \
    src/irc/irceventreducer.cpp \
    src/irc/qtirctransport.cpp \
    src/irc/ircloopbacktransport.cpp \
    src/irc/ircdemoserver.cpp \
    src/irc/ircstatusentry.cpp \
    src/irc/ircservicenick.cpp \
    src/irc/ircprefixnick.cpp \
    src/irc/ircsecretpolicy.cpp \
    src/irc/ircconversationlog.cpp \
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
    src/storage/secretservicecredentialstore.cpp \
    src/irc/ircconnection.cpp

RESOURCES += src/resources.qrc

unix:!macx {
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

include($$PWD/packaging/macos/sdk-compat.pri)
