#!/usr/bin/env sh
set -eu

# Installs Omairc's terminal client, omairc-tui, on Linux or macOS.
#
# It resolves the latest GitHub release, downloads the static binary for the
# host OS and architecture, checks its --version, and installs it under
# ~/.local/bin as your user. Safe to re-run: each run replaces the binary with
# the newest release.
#
# This is the package-manager-free path; pacman, Homebrew, and Scoop each have
# their own package. install.sh still installs the Qt client only.

RELEASES_URL="https://github.com/fredimachado/omairc/releases"
BIN_NAME=omairc-tui

usage() {
  cat <<'EOF'
Usage: install-tui.sh

Installs the latest omairc-tui release for Linux or macOS.

    curl -fsSL https://raw.githubusercontent.com/fredimachado/omairc/main/install-tui.sh | sh

Windows is not supported here; use Scoop or the release zip.

Environment:
  OMAIRC_TUI_VERSION       Install a specific version instead of the latest.
  OMAIRC_TUI_DOWNLOAD_URL  Fetch the archive from this URL instead of GitHub.
  OMAIRC_TUI_BIN_DIR       Install directory (default: $HOME/.local/bin).
EOF
}

case "${1:-}" in
  -h|--help)
    usage
    exit 0
    ;;
  "")
    ;;
  *)
    echo "install-tui.sh takes no arguments. See install-tui.sh --help." >&2
    exit 1
    ;;
esac

for command in curl tar install; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "$command is required to install $BIN_NAME." >&2
    exit 1
  fi
done

case "$(uname -s)" in
  Linux) goos=linux ;;
  Darwin) goos=darwin ;;
  *)
    echo "$BIN_NAME is a Linux and macOS binary." >&2
    echo "On Windows, use Scoop or unzip the release zip." >&2
    exit 1
    ;;
esac

case "$(uname -m)" in
  x86_64|amd64) goarch=amd64 ;;
  arm64|aarch64) goarch=arm64 ;;
  *)
    echo "No $BIN_NAME release for this machine ($(uname -m))." >&2
    exit 1
    ;;
esac

if [ -n "${OMAIRC_TUI_VERSION:-}" ]; then
  version="$OMAIRC_TUI_VERSION"
else
  # Follow /releases/latest to the tag; no API token or rate limit involved.
  if ! effective="$(curl -fsSLI -o /dev/null -w '%{url_effective}' \
      "$RELEASES_URL/latest")"; then
    echo "Could not reach $RELEASES_URL/latest." >&2
    exit 1
  fi
  version="${effective##*/tag/v}"
  case "$version" in
    ""|*/*) version="" ;;
  esac
  if [ -z "$version" ]; then
    echo "Could not resolve the latest $BIN_NAME release." >&2
    echo "Set OMAIRC_TUI_VERSION to install a known version." >&2
    exit 1
  fi
fi

asset="$BIN_NAME-$version-$goos-$goarch.tar.gz"
url="${OMAIRC_TUI_DOWNLOAD_URL:-$RELEASES_URL/download/v$version/$asset}"

work="$(mktemp -d "${TMPDIR:-/tmp}/omairc-tui.XXXXXX")"
trap 'rm -rf -- "$work"' EXIT HUP INT TERM

echo "Downloading $asset"
if ! curl -fsSL -o "$work/$asset" "$url"; then
  echo "Could not download $url" >&2
  exit 1
fi

tar -xzf "$work/$asset" -C "$work"
binary="$work/$BIN_NAME"
if [ ! -f "$binary" ]; then
  echo "$asset did not contain $BIN_NAME." >&2
  exit 1
fi

got="$("$binary" --version)"
want="$BIN_NAME $version"
if [ "$got" != "$want" ]; then
  echo "Downloaded $BIN_NAME reports '$got', expected '$want'." >&2
  exit 1
fi

bin_dir="${OMAIRC_TUI_BIN_DIR:-$HOME/.local/bin}"
mkdir -p "$bin_dir"

# Stage beside the target and rename, so replacing a running binary never hits
# ETXTBSY and the swap is atomic.
staged="$bin_dir/.$BIN_NAME.new"
install -m 0755 "$binary" "$staged"
mv -f "$staged" "$bin_dir/$BIN_NAME"

echo "Installed $want to $bin_dir/$BIN_NAME"
case ":${PATH:-}:" in
  *":$bin_dir:"*) ;;
  *)
    echo "Add $bin_dir to PATH to run $BIN_NAME."
    ;;
esac
