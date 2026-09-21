# Single version for --version, CTCP VERSION, tests, and packaging.
# Arch pkgver cannot contain hyphens: use 0.9.0alpha, not 0.9.0-alpha.
# Tag releases as v plus this string. Letter suffixes (alpha, beta, rc)
# compare older than the final 0.9.0, so pacman upgrades cleanly.
VERSION = 0.9.0

# Write a generated header so incremental builds recompile objects that embed
# the version when VERSION changes. Regeneration is content-compared so an
# unchanged version does not touch the file and recompile the world.
isEmpty(OMAIRC_ROOT) {
    exists($$PWD/version.pri): OMAIRC_ROOT = $$PWD
    else:exists($$PWD/../version.pri): OMAIRC_ROOT = $$PWD/..
    else:exists($$PWD/../../version.pri): OMAIRC_ROOT = $$PWD/../..
    else:error("Cannot locate repository root for version.pri")
}

OMAIRC_EFFECTIVE_VERSION = $$VERSION
!isEmpty(OMAIRC_BUILD_VERSION) {
    OMAIRC_EFFECTIVE_VERSION = $$OMAIRC_BUILD_VERSION
}

OMAIRC_VERSION_HEADER = $$OUT_PWD/omaircversion.h
win32: OMAIRC_PYTHON = python
else: OMAIRC_PYTHON = python3
system($$OMAIRC_PYTHON $$shell_quote($$OMAIRC_ROOT/bin/gen-omaircversion-h.py) $$OMAIRC_EFFECTIVE_VERSION $$shell_quote($$OMAIRC_VERSION_HEADER))

INCLUDEPATH += $$OUT_PWD
HEADERS += $$OMAIRC_VERSION_HEADER
