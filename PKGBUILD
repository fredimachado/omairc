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
  'hicolor-icon-theme'
  'qt6-base>=6.11'
  'qt6-declarative>=6.11'
  'qt6-svg>=6.11'
  'qt6-wayland>=6.11'
  'qtkeychain-qt6'
  'xdg-desktop-portal'
)
makedepends=(
  'qt6-base>=6.11'
  'qt6-declarative>=6.11'
  'qt6-svg>=6.11'
  'qt6-wayland>=6.11'
  'qtkeychain-qt6'
  'base-devel'
)
source=("$pkgname-$pkgver.tar.gz::https://github.com/fredimachado/omairc/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('SKIP')

build() {
  cd "$srcdir/$pkgname-$pkgver"
  local -a qmake_args=(PREFIX=/usr)
  if [ -n "${OMAIRC_BUILD_VERSION:-}" ]; then
    qmake_args+=(OMAIRC_BUILD_VERSION="$OMAIRC_BUILD_VERSION")
  fi
  qmake6 "${qmake_args[@]}"
  make
}

package() {
  cd "$srcdir/$pkgname-$pkgver"
  make INSTALL_ROOT="$pkgdir" install
}
