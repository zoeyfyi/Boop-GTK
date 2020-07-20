# Maintainer: Ben Sheffield <me@mrbenshef.co.uk>
pkgname=boop-gtk
pkgver=0.2.0
pkgrel=1
epoch=
pkgdesc="Port of IvanMathy's Boop to GTK, a scriptable scratchpad for developers."
arch=()
url="https://boop-gtk.mrbenshef.co.uk"
license=('MIT')
depends=("gtk3", "gtksourceview3")
makedepends=("rust")

# source=("$pkgname-$pkgver.tar.gz"
#         "$pkgname-$pkgver.patch")

validpgpkeys=()

# prepare() {
# 	cd "$pkgname-$pkgver"
# 	patch -p1 -i "$srcdir/$pkgname-$pkgver.patch"
# }

build() {
	cd $pkgname-$pkgver
	cargo build --release --locked --all-features
}

check() {
	cd $pkgname-$pkgver
	cargo test --release --locked
}

package() {
	cd $pkgname-$pkgver
	install -Dm 755 target/release/${pkgname} -t "${pkgdir}/usr/bin"
}
