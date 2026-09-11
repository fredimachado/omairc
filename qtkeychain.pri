# QtKeychain discovery. Included from the project root (omairc.pro) and from
# tests/ (tests/tests.pro), so resolve .deps relative to this file.
deps_dir = $$clean_path($$PWD/.deps)
qtkeychain_pri = $$[QT_INSTALL_ARCHDATA]/mkspecs/modules/qt_Qt6Keychain.pri
exists($$deps_dir/usr/lib/qt6/mkspecs/modules/qt_Qt6Keychain.pri) {
    qtkeychain_pri = $$deps_dir/usr/lib/qt6/mkspecs/modules/qt_Qt6Keychain.pri
    INCLUDEPATH += $$deps_dir/usr/include
    LIBS += -L$$deps_dir/usr/lib
}
!exists($$qtkeychain_pri) {
    error(QtKeychain Qt6 module not found. Install qtkeychain-qt6 or provide .deps/usr/lib/qt6/mkspecs/modules/qt_Qt6Keychain.pri.)
}
include($$qtkeychain_pri)
QT += Qt6Keychain
