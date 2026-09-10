# Maintainer: Fredi Machado
pkgname=omairc
# pkgver is read from version.pri so a release bump is a one-line change.
pkgver=$(awk '/^VERSION / { print $3; exit }' version.pri)
pkgrel=1
pkgdesc='Dead-simple IRC client for Omarchy'
arch=('x86_64')
url='https://github.com/fredimachado/omairc'
license=('MIT' 'LGPL-3.0-or-later' 'OFL-1.1')
options=(!debug)
depends=(
  'qt6-base'
  'qt6-declarative'
  'qt6-svg'
  'qt6-wayland'
  'xdg-desktop-portal'
)
makedepends=(
  'qt6-base'
  'qt6-declarative'
  'qt6-svg'
  'qt6-wayland'
  'base-devel'
)
source=("$pkgname-$pkgver.tar.gz::https://github.com/fredimachado/omairc/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('SKIP')

build() {
  cd "$srcdir/$pkgname-$pkgver"
  qmake6 PREFIX=/usr
  make
}

package() {
  cd "$srcdir/$pkgname-$pkgver"
  make INSTALL_ROOT="$pkgdir" install
}
