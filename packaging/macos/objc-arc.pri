# Compile Objective-C / Objective-C++ sources with ARC. Included from every
# .pro that builds src/macosnotifications.mm so notify() ownership is defined.
QMAKE_OBJECTIVE_CFLAGS += -fobjc-arc
QMAKE_OBJECTIVE_CXXFLAGS += -fobjc-arc
