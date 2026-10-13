#!/usr/bin/env sh
# Installe wasm-bindgen à la version de Cargo.lock (binaire précompilé, sinon
# compilé), pour scripts/web.sh. Utilisé par la CI et le Dockerfile.
set -eu
cd "$(dirname "$0")/.."
version=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"$/\1/p')
dest="${CARGO_HOME:-$HOME/.cargo}/bin"
mkdir -p "$dest"
if command -v wasm-bindgen >/dev/null 2>&1 && wasm-bindgen --version | grep -q "$version"; then
    exit 0
fi
curl -sSfL "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/${version}/wasm-bindgen-${version}-x86_64-unknown-linux-musl.tar.gz" \
    | tar -xz --strip-components=1 -C "$dest" \
    || cargo install wasm-bindgen-cli --version "$version" --locked
wasm-bindgen --version
