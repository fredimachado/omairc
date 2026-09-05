#!/usr/bin/env bash
# Idempotent Cloud Agent setup for Omairc.
#
# Omairc is a Qt 6 Quick application (qmake + C++17). It uses Qt APIs that
# require Qt >= 6.5 (for example Qt::ColorScheme), which is newer than the
# Qt 6.4 that Ubuntu 24.04 ships. This script installs a self-contained
# official Qt 6 via aqtinstall into /opt/Qt, plus the system libraries and
# optional black-box test tooling the repository's bin/ scripts expect.
#
# Safe to run repeatedly: every step checks for existing state first.
set -euo pipefail

QT_VERSION="6.8.3"
QT_ROOT="/opt/Qt/${QT_VERSION}/gcc_64"
PROFILE_SCRIPT="/etc/profile.d/omairc-qt.sh"

if [ "$(id -u)" -eq 0 ]; then
  SUDO=""
else
  SUDO="sudo"
fi

log() { printf '\n=== %s ===\n' "$1"; }

log "Installing system packages"
export DEBIAN_FRONTEND=noninteractive
$SUDO apt-get update -y
# build-essential + Qt xcb runtime libraries + optional desktop-test tooling
# (Xvfb/xauth/xdotool/imagemagick) used by bin/test-desktop, and python venv
# tooling for aqtinstall.
$SUDO apt-get install -y --no-install-recommends \
  build-essential ca-certificates \
  python3 python3-venv python3-pip \
  libgl1 libegl1 libfontconfig1 libxkbcommon0 libxkbcommon-x11-0 libdbus-1-3 \
  libxcb-cursor0 libxcb-icccm4 libxcb-image0 libxcb-keysyms1 libxcb-randr0 \
  libxcb-render-util0 libxcb-shape0 libxcb-xinerama0 libxcb-xkb1 libxcb-util1 \
  libxcb-glx0 libxcb-shm0 libxcb-sync1 libxcb-xfixes0 libx11-xcb1 \
  xvfb x11-xserver-utils xauth xdotool imagemagick

log "Installing Qt ${QT_VERSION} via aqtinstall (if missing)"
if [ ! -x "${QT_ROOT}/bin/qmake6" ]; then
  AQT_VENV="/opt/aqt-venv"
  if [ ! -x "${AQT_VENV}/bin/aqt" ]; then
    $SUDO mkdir -p "${AQT_VENV}"
    $SUDO chown "$(id -u):$(id -g)" "${AQT_VENV}"
    python3 -m venv "${AQT_VENV}"
    "${AQT_VENV}/bin/pip" install --upgrade pip aqtinstall
  fi
  $SUDO mkdir -p /opt/Qt
  $SUDO chown "$(id -u):$(id -g)" /opt/Qt
  "${AQT_VENV}/bin/aqt" install-qt linux desktop "${QT_VERSION}" linux_gcc_64 -O /opt/Qt
else
  echo "Qt ${QT_VERSION} already present at ${QT_ROOT}"
fi

log "Writing login-shell environment to ${PROFILE_SCRIPT}"
$SUDO tee "${PROFILE_SCRIPT}" >/dev/null <<EOF
# Qt ${QT_VERSION} (installed via aqtinstall) for the Omairc build.
export PATH="${QT_ROOT}/bin:\$PATH"
export LD_LIBRARY_PATH="${QT_ROOT}/lib:\${LD_LIBRARY_PATH:-}"
export QT_PLUGIN_PATH="${QT_ROOT}/plugins"
export QML2_IMPORT_PATH="${QT_ROOT}/qml"
EOF
$SUDO chmod +x "${PROFILE_SCRIPT}"

log "Installing ImageMagick 7 'magick' compatibility shim"
# Ubuntu ships ImageMagick 6 (legacy per-command tools) without the unified
# `magick` entrypoint that ImageMagick 7 / Omarchy provide and that
# bin/test-desktop invokes. Provide a thin dispatcher.
$SUDO tee /usr/local/bin/magick >/dev/null <<'EOF'
#!/usr/bin/env sh
set -eu
case "${1:-}" in
  convert|mogrify|identify|composite|montage|compare|import|conjure|stream|display|animate)
    cmd="$1"; shift; exec "$cmd" "$@" ;;
  *)
    exec convert "$@" ;;
esac
EOF
$SUDO chmod +x /usr/local/bin/magick

log "Building Omairc to validate the toolchain"
# shellcheck disable=SC1090
. "${PROFILE_SCRIPT}"
ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
"${ROOT}/bin/build"

log "Setup complete"
qmake6 -query QT_VERSION
