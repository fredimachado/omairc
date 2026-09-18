# macOS SDK/Qt compatibility: newer SDKs drop AGL; Qt 6.8 still links it.
# Xcode 26 Clang reports __has_builtin(__yield) in Qt 6.8's qyieldcpu.h, so
# Apple Silicon needs <arm_acle.h>. That header is ARM-only: forcing it on
# Intel fails with "ACLE intrinsics support not enabled."
macx {
    CONFIG += sdk_no_version_check
    equals(QMAKE_HOST.arch, arm64) {
        QMAKE_CXXFLAGS += -include arm_acle.h
    }

    QMAKE_LIBS_OPENGL = -framework OpenGL
    LIBS = $$replace(LIBS, "-framework AGL", )
}
