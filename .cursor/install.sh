#!/usr/bin/env bash
# Idempotent Cloud Agent setup for Omairc.
#
# Omairc's desktop client is a Rust GPUI app in client/. This script installs
# a Rust toolchain, system libraries GPUI needs on Linux, Go for the TUI gate,
# and optional black-box desktop tooling (Xvfb / xdotool / lavapipe).
#
# Safe to run repeatedly: every step checks for existing state first.
set -euo pipefail

if [ "$(id -u)" -eq 0 ]; then
  SUDO=""
else
  SUDO="sudo"
fi

log() { printf '\n=== %s ===\n' "$1"; }

log "Installing system packages"
export DEBIAN_FRONTEND=noninteractive
$SUDO apt-get update -y
$SUDO apt-get install -y --no-install-recommends \
  build-essential ca-certificates curl git \
  pkg-config libsecret-1-dev libdbus-1-dev \
  python3 python3-venv python3-pip \
  libfontconfig1 libxkbcommon0 libxkbcommon-x11-0 \
  libxkbcommon-dev libxkbcommon-x11-dev libdbus-1-3 \
  libxcb-cursor0 libxcb-icccm4 libxcb-image0 libxcb-keysyms1 libxcb-randr0 \
  libxcb-render-util0 libxcb-shape0 libxcb-xinerama0 libxcb-xkb1 libxcb-util1 \
  libxcb-glx0 libxcb-shm0 libxcb-sync1 libxcb-xfixes0 libx11-xcb1 \
  xvfb x11-xserver-utils xauth xdotool xinput imagemagick \
  mesa-vulkan-drivers

log "Installing Rust (rustup) if missing"
if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  . "$HOME/.cargo/env"
fi
rustup component add clippy rustfmt >/dev/null

log "Installing Go if missing"
if ! command -v go >/dev/null 2>&1; then
  $SUDO apt-get install -y --no-install-recommends golang-go || true
fi

# Ubuntu ImageMagick 6 often lacks the `magick` entrypoint ImageMagick 7 provides.
if ! command -v magick >/dev/null 2>&1 && command -v convert >/dev/null 2>&1; then
  log "Installing magick shim for ImageMagick 6"
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
fi

log "Smoke"
cargo --version
command -v go >/dev/null 2>&1 && go version || echo "go not installed (tui/bin/test will skip)"
command -v magick
command -v Xvfb
ls /usr/share/vulkan/icd.d/*lvp* /usr/share/vulkan/icd.d/*lavapipe* 2>/dev/null \
  || ls /usr/share/vulkan/icd.d/*.json || true

echo
echo "Omairc agent environment ready. From the repo root:"
echo "  bin/check-conventions"
echo "  (cd client && cargo test --locked)"
echo "  bin/test"
