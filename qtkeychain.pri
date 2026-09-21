# QtKeychain discovery. Included from the project root (omairc.pro) and from
# tests/ (tests/tests.pro), so resolve .deps relative to this file.
deps_dir = $$clean_path($$PWD/.deps)
qtkeychain_pri = $$[QT_INSTALL_ARCHDATA]/mkspecs/modules/qt_Qt6Keychain.pri
# Prefix install into the Qt kit (CI / bin/install-qtkeychain into QT_INSTALL_PREFIX).
# Local .deps layouts are Unix-only (Linux packages and a cmake DESTDIR/prefix).
# Do not let a leftover Linux .deps tree win on Windows: jom then looks for
# .deps/usr/lib/qt6keychain.lib, which does not exist.
unix {
    exists($$deps_dir/usr/lib/qt6/mkspecs/modules/qt_Qt6Keychain.pri) {
        qtkeychain_pri = $$deps_dir/usr/lib/qt6/mkspecs/modules/qt_Qt6Keychain.pri
        INCLUDEPATH += $$deps_dir/usr/include
        LIBS += -L$$deps_dir/usr/lib
    } else:exists($$deps_dir/usr/mkspecs/modules/qt_Qt6Keychain.pri) {
        qtkeychain_pri = $$deps_dir/usr/mkspecs/modules/qt_Qt6Keychain.pri
        INCLUDEPATH += $$deps_dir/usr/include
        LIBS += -L$$deps_dir/usr/lib
    }
}
# Homebrew qtkeychain keeps its qmake module beside the formula, not under Qt.
!exists($$qtkeychain_pri) {
    brew_qtkeychain = $$system(command -v brew >/dev/null && brew --prefix qtkeychain 2>/dev/null)
    !isEmpty(brew_qtkeychain):exists($$brew_qtkeychain/mkspecs/modules/qt_Qt6Keychain.pri) {
        qtkeychain_pri = $$brew_qtkeychain/mkspecs/modules/qt_Qt6Keychain.pri
    }
}
!exists($$qtkeychain_pri) {
    error(QtKeychain Qt6 module not found. Install qtkeychain-qt6, brew install qtkeychain, or provide .deps/usr/mkspecs/modules/qt_Qt6Keychain.pri.)
}
include($$qtkeychain_pri)
QT += Qt6Keychain
