# Single version for --version, CTCP VERSION, tests, and packaging.
# Arch pkgver cannot contain hyphens: use 1.0.7alpha, not 1.0.7-alpha.
# Tag releases as v plus this string. Letter suffixes (alpha, beta, rc)
# compare older than the final 1.0.7, so pacman upgrades cleanly.
#
# The Rust client reads this in client/build.rs (OMAIRC_VERSION). The TUI
# injects it through tui/bin/build. Do not duplicate VERSION elsewhere.
VERSION = 1.11.0rc1
