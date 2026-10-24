# Maintainer: Fredi Machado
pkgname=omairc
# pkgver is read from version.pri so a release bump is a one-line change.
pkgver=$(awk '/^VERSION / { print $3; exit }' version.pri)
pkgrel=1
pkgdesc='Simple IRC client for Omarchy'
arch=('x86_64')
url='https://github.com/fredimachado/omairc'
license=('MIT' 'LGPL-3.0-or-later' 'OFL-1.1')
options=(!debug)
depends=(
  'fontconfig'
  'freetype2'
  'hicolor-icon-theme'
  'libxcb'
  'libxkbcommon'
  'libxkbcommon-x11'
  'vulkan-icd-loader'
  'wayland'
  'xdg-desktop-portal'
)
makedepends=(
  'base-devel'
  'cargo'
  'rust'
)
source=("$pkgname-$pkgver.tar.gz::https://github.com/fredimachado/omairc/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('SKIP')

build() {
  cd "$srcdir/$pkgname-$pkgver"
  export OMAIRC_BUILD_VERSION="${OMAIRC_BUILD_VERSION:-$pkgver}"
  cargo build --release --locked --features product --manifest-path poc/Cargo.toml
}

package() {
  cd "$srcdir/$pkgname-$pkgver"
  install -Dm755 poc/target/release/omairc "$pkgdir/usr/bin/omairc"
  install -Dm644 data/omairc.desktop "$pkgdir/usr/share/applications/omairc.desktop"
  install -Dm644 data/icons/hicolor/scalable/apps/omairc.svg \
    "$pkgdir/usr/share/icons/hicolor/scalable/apps/omairc.svg"
  install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
  install -Dm644 fonts/OFL.txt "$pkgdir/usr/share/licenses/$pkgname/OFL.txt"
  install -Dm644 src/irc/COPYING "$pkgdir/usr/share/licenses/$pkgname/COPYING-LGPL"
  install -Dm644 data/bash-completion/omairc \
    "$pkgdir/usr/share/bash-completion/completions/omairc"
}
