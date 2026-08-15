#!/usr/bin/env bash
# Build the WASM playground runtime from source, offline at run time.
#
# This environment's rustc is packaged by a distro and has no prebuilt
# wasm32-unknown-unknown std, and rustup is unavailable. We therefore build a
# local sysroot from the official rust-std/rust-src tarballs for the *exact*
# installed rustc version, then use `-Z build-std` (via RUSTC_BOOTSTRAP=1) so the
# standard library is compiled by the same compiler as the rest of the code.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$(pwd)
SETUP=${SUUKTI_WASM_SETUP:-/tmp/opencode/wasm-setup}
VER=$(rustc --version | awk '{print $2}')
HOST=$(rustc -vV | awk '/^host:/{print $2}')
mkdir -p "$SETUP"
RUSTFLAGS_KEEP="${RUSTFLAGS:-}"

fetch() { # url dest
  if [ ! -s "$2" ]; then
    echo "[wasm] downloading $1"
    curl -fsSL --max-time 300 -o "$2" "$1"
  fi
}

if [ ! -x "$SETUP/rustc-wrapper.sh" ]; then
  fetch "https://static.rust-lang.org/dist/rust-std-${VER}-wasm32-unknown-unknown.tar.xz" \
        "$SETUP/rust-std-wasm.tar.xz"
  fetch "https://static.rust-lang.org/dist/rust-src-${VER}.tar.xz" \
        "$SETUP/rust-src.tar.xz"
  echo "[wasm] preparing local sysroot at $SETUP/sysroot"
  mkdir -p "$SETUP/extract" "$SETUP/sysroot/lib/rustlib"
  tar -xf "$SETUP/rust-std-wasm.tar.xz" -C "$SETUP/extract"
  cp -a "$SETUP/extract"/rust-std-${VER}-wasm32-unknown-unknown/*/lib/rustlib/wasm32-unknown-unknown \
        "$SETUP/sysroot/lib/rustlib/"
  cp -a "$(rustc --print sysroot)/lib/rustlib/$HOST" "$SETUP/sysroot/lib/rustlib/"
  tar -xf "$SETUP/rust-src.tar.xz" -C "$SETUP/extract"
  mkdir -p "$SETUP/sysroot/lib/rustlib/src"
  cp -a "$SETUP/extract"/rust-src-${VER}/rust-src/lib/rustlib/src/rust "$SETUP/sysroot/lib/rustlib/src/"
  cat > "$SETUP/rustc-wrapper.sh" <<EOF
#!/bin/sh
exec $(command -v rustc) --sysroot=$SETUP/sysroot "\$@"
EOF
  chmod +x "$SETUP/rustc-wrapper.sh"
fi

echo "[wasm] building sukti-wasm (this compiles std once, ~1 min)"
RUSTC="$SETUP/rustc-wrapper.sh" RUSTC_BOOTSTRAP=1 RUSTFLAGS="$RUSTFLAGS_KEEP" \
  cargo build -Z build-std=std,panic_abort -p sukti-wasm --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/sukti_wasm.wasm playground/sukti_wasm.wasm
ls -lh playground/sukti_wasm.wasm
echo "[wasm] OK — open playground/index.html via a local HTTP server"
