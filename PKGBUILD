# Maintainer: Fredi Machado
pkgname=omairc
pkgver=0.1.0
pkgrel=1
pkgdesc='Dead-simple IRC client for Omarchy'
arch=('x86_64')
url='https://github.com/fredimachado/omairc'
license=('MIT' 'LGPL-3.0-or-later' 'OFL-1.1')
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
