# Boop-GTK

<p align="center">
  <img src="screenshot.png">
</p>

<h2 align="center">A scriptable scratchpad for developers</h2>
<p align="center">Port of <a href="https://github.com/IvanMathy"><b>@IvanMathy</b></a>'s <a href="https://github.com/IvanMathy/Boop">Boop</a> to GTK</p>

![Continuous integration](https://github.com/mrbenshef/Boop-GTK/workflows/Continuous%20integration/badge.svg)
![Release](https://github.com/mrbenshef/Boop-GTK/workflows/Release/badge.svg?branch=release)
![Crates.io](https://img.shields.io/crates/v/boop-gtk)

### Get Boop-GTK

- Pre-build binarys, flatpak and snaps on [Github releases](https://github.com/mrbenshef/Boop-GTK/releases)
- Snap Store (soon)
- Flathub (soon)
- Package managers (maybe)
- Compile from source

### Building

#### Linux

```shell
sudo apt-get install -y libgtk-3-dev libgtksourceview-3.0-dev
cargo build
```

#### MacOS

```shell
brew install gtk+3 gtksourceview3
cargo build
```

#### Windows

```powershell
git clone https://github.com/wingtk/gvsbuild.git C:\gtk-build\github\gvsbuild

cd C:\gtk-build\github\gvsbuild

python .\build.py build -p=x64 --vs-ver=16 --msys-dir=C:\msys64 -k --enable-gi --py-wheel --py-egg gtk3 gdk-pixbuf gtksourceview3

$Env:RUSTFLAGS = "-L C:\gtk-build\gtk\x64\release\lib"; cargo build
```
