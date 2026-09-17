# macOS SDK/Qt compatibility: newer SDKs drop AGL; Qt 6.8 still links it.
macx {
    CONFIG += sdk_no_version_check
    QMAKE_CXXFLAGS += -include arm_acle.h

    QMAKE_LIBS_OPENGL = -framework OpenGL
    LIBS = $$replace(LIBS, "-framework AGL", )
}
