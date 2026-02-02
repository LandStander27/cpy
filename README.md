# cpy

[![build and test status](https://codeberg.org/Land/cpy/actions/workflows/build_and_test.yaml/badge.svg)](https://codeberg.org/Land/cpy/actions?workflow=build_and_test.yaml)

**Your common `cp` but faster, modern, and hopefully better**

## ✨ Features
- Progress bars
- Made in Rust
- Parallel copying
- [Reflink/CoW](https://btrfs.readthedocs.io/en/latest/Reflink.html) support

## 📦 Installation

### 🧪 Arch-based distros, via the `landware` repo (Arch, EndeavourOS, CachyOS, Manjaro, etc.)
```sh
# Install pacsync command
sudo pacman -S --needed pacutils

# Add repo
echo "[landware]              
Server = https://repo.kage.sj.strangled.net/landware/x86_64
SigLevel = DatabaseNever PackageNever TrustedOnly" | sudo tee -a /etc/pacman.conf

# Sync the repo
sudo pacsync landware

# Install like a normal package
sudo pacman -S cpy-git
```
> Note: it is recommended to run a full system update (via `pacman -Syu`) after syncing any repository

### 🔧 Manually
```sh
# Install deps
## Arch Linux
sudo pacman -S --needed git rust sed libgit2 gzip

# Clone the repo
git clone https://codeberg.org/Land/autoclicker.git
cd autoclicker

# Generate zsh shell completion + manpage
cargo b --locked --release --features=generators
./target/release/cpy . . --generate-man cpy.1 --generate-shell zsh > _cpy
gzip cpy.1

# Build cpy
cargo b --release

# Install binary and license file
sudo install -Dm755 "target/release/cpy" "/usr/bin/cpy"

# Install LICENSE, zsh completions, and manpage
sudo install -Dm644 "LICENSE" -t "/usr/share/licenses/cpy/"
sudo install -Dm644 "_cpy" -t "/usr/share/zsh/site-functions/"
sudo install -Dm644 "cpy.1.gz" -t "/usr/share/man/man1/"

# Optional cleanup
sudo pacman -Rs git rust sed libgit2 gzip
```

## 🛠️ Options
```sh
user@arch ~ $ cpy --help
cp but better (hopefully)

Usage: cpy [OPTIONS] <SRC>... <DEST>

Arguments:
  <SRC>...  sources to copy
  <DEST>    destination

Options:
  -h, --help               display help
      --version            print version
  -v, --verbose...         increase verbosity (-v: info, -vv: debug, -vvv: trace, -vvvv: trace, more detailed errors)
  -r, --recursive          copy directories recursively [aliases: -R]
  -a, --archive            preserves all file attributes
  -j, --threads <THREADS>  threads to use for copying [default: 4]
  -f, --force              if an existing destination file cannot be created, remove it and try again
  -u, --update             ignore files with destinations that already exist
      --reflink <MODE>  copy files as CoW copies. see https://btrfs.readthedocs.io/en/latest/Reflink.html [default: auto] [possible values: never, always, auto]
```

> Inspired by [cpx](https://github.com/11happy/cpx/tree/7ab459fcdd9b2e94d21105c1e6706a8445056bb4)