# Single version for --version, CTCP VERSION, tests, and packaging.
# Arch pkgver cannot contain hyphens: use 0.2.0alpha, not 0.2.0-alpha.
# Tag releases as v plus this string. Letter suffixes (alpha, beta, rc)
# compare older than the final 0.2.0, so pacman upgrades cleanly.
VERSION = 0.2.0
DEFINES += OMAIRC_VERSION=\\\"$$VERSION\\\"
