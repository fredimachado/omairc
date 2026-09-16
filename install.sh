#!/usr/bin/env sh
set -eu

# Installs Omairc on Arch-based systems (Omarchy, Arch Linux, Manjaro,
# EndeavourOS, ...). Adds the omairc pacman repository pointing at the latest
# GitHub release, then installs it. Safe to re-run: the repository is only
# added once, and later runs upgrade to the newest release.

REPO_NAME=omairc
REPO_URL="https://github.com/fredimachado/omairc/releases/latest/download"
PACMAN_CONF="${PACMAN_CONF:-/etc/pacman.conf}"

usage() {
  cat <<'EOF'
Usage: install.sh

Installs the latest omairc release from its pacman repository.

    curl -fsSL https://raw.githubusercontent.com/fredimachado/omairc/master/install.sh | sh

Run it as your regular user; sudo is used where it is needed.
EOF
}

case "${1:-}" in
  -h|--help)
    usage
    exit 0
    ;;
esac

if ! command -v pacman >/dev/null 2>&1; then
  echo "omairc ships an Arch package, so this installer needs pacman." >&2
  echo "Omarchy, Arch Linux, Manjaro, and EndeavourOS are supported." >&2
  echo "For other distros, see https://github.com/fredimachado/omairc" >&2
  exit 1
fi

if [ "$(id -u)" -eq 0 ]; then
  echo "Run this as your regular user; sudo is used where it is needed." >&2
  exit 1
fi

if ! command -v sudo >/dev/null 2>&1; then
  echo "sudo is required to configure the repository and install omairc." >&2
  exit 1
fi

if grep -q "^\[$REPO_NAME\]" "$PACMAN_CONF"; then
  echo "The $REPO_NAME repository is already configured."
else
  echo "Adding the $REPO_NAME repository to $PACMAN_CONF"
  printf '\n[%s]\nSigLevel = Optional TrustAll\nServer = %s\n' \
    "$REPO_NAME" "$REPO_URL" | sudo tee -a "$PACMAN_CONF" >/dev/null
fi

echo "Installing the latest omairc release"
sudo pacman -Sy --noconfirm omairc
