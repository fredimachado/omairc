QT += core gui qml quick quickcontrols2 dbus

CONFIG += c++17 release
TARGET = omairc
TEMPLATE = app

HEADERS += \
    src/backend.h \
    src/systemtheme.h

SOURCES += \
    src/main.cpp \
    src/backend.cpp \
    src/systemtheme.cpp

RESOURCES += src/resources.qrc
