# Maintainer: Fredi Machado
pkgname=omairc
# pkgver is read from version.pri so a release bump is a one-line change.
pkgver=$(awk '/^VERSION / { print $3; exit }' version.pri)
pkgrel=1
pkgdesc='Simple IRC client for Omarchy'
arch=('x86_64')
url='https://github.com/fredimachado/omairc'
license=('MIT' 'OFL-1.1')
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

prepare() {
  cd "$srcdir/$pkgname-$pkgver"
  export RUSTUP_TOOLCHAIN=stable
  cargo fetch --locked --manifest-path client/Cargo.toml \
    --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
  cd "$srcdir/$pkgname-$pkgver"
  export OMAIRC_BUILD_VERSION="${OMAIRC_BUILD_VERSION:-$pkgver}"
  export RUSTUP_TOOLCHAIN=stable
  export CARGO_TARGET_DIR=target
  # makepkg's LDFLAGS inject -fuse-ld=lld; ring's asm objects then fail to link
  # (undefined ring_core_*). Prefer the default bfd/gold path through gcc.
  export LDFLAGS="${LDFLAGS//-fuse-ld=lld/}"
  cargo build --release --frozen --features product --manifest-path client/Cargo.toml
}

package() {
  cd "$srcdir/$pkgname-$pkgver"
  install -Dm755 target/release/omairc "$pkgdir/usr/bin/omairc"
  install -Dm644 data/omairc.desktop "$pkgdir/usr/share/applications/omairc.desktop"
  install -Dm644 data/icons/hicolor/scalable/apps/omairc.svg \
    "$pkgdir/usr/share/icons/hicolor/scalable/apps/omairc.svg"
  install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
  install -Dm644 fonts/OFL.txt "$pkgdir/usr/share/licenses/$pkgname/OFL.txt"
  install -Dm644 data/bash-completion/omairc \
    "$pkgdir/usr/share/bash-completion/completions/omairc"
}
