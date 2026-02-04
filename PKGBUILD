pkgname=waft-overview-git
pkgver=0.1.0
pkgrel=1
pkgdesc="A GTK4/libadwaita overlay shell for Linux desktops"
arch=('x86_64' 'aarch64')
url="https://github.com/readyplayernan/waft"
license=('MIT')
depends=('gtk4' 'libadwaita' 'gtk4-layer-shell')
makedepends=('cargo' 'git')
provides=('waft-overview')
conflicts=('waft-overview')
source=("$pkgname::git+file://$(pwd)")
sha256sums=('SKIP')

pkgver() {
  cd "$srcdir/$pkgname"
  printf "r%s.%s" "$(git rev-list --count HEAD)" "$(git rev-parse --short HEAD)"
}

prepare() {
  cd "$srcdir/$pkgname"
  export RUSTUP_TOOLCHAIN=stable
  cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
  cd "$srcdir/$pkgname"
  export RUSTUP_TOOLCHAIN=stable
  export CARGO_TARGET_DIR=target
  cargo build --frozen --release --all-features
}

check() {
  cd "$srcdir/$pkgname"
  export RUSTUP_TOOLCHAIN=stable
  cargo test --frozen --all-features
}

package() {
  cd "$srcdir/$pkgname"
  install -Dm755 "target/release/waft-overview" "$pkgdir/usr/bin/waft-overview"
  install -Dm644 "LICENSE" "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
  install -Dm644 "crates/overview/default.toml" "$pkgdir/usr/share/doc/$pkgname/default.toml"
  install -Dm644 "README.md" "$pkgdir/usr/share/doc/$pkgname/README.md"
}
