# Single version for --version, CTCP VERSION, tests, and packaging.
# Arch pkgver cannot contain hyphens: use 0.5.0alpha, not 0.5.0-alpha.
# Tag releases as v plus this string. Letter suffixes (alpha, beta, rc)
# compare older than the final 0.5.0, so pacman upgrades cleanly.
VERSION = 0.5.0
DEFINES += OMAIRC_VERSION=\\\"$$VERSION\\\"
