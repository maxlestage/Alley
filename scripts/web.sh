#!/usr/bin/env sh
# Compile le navigateur d'Alley (client/, Rust) en WebAssembly, dans client/pkg/.
#
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version 0.2.129   # même version que client/Cargo.toml
#   scripts/web.sh
set -eu
cd "$(dirname "$0")/.."
cargo build -p alley-web --lib --target wasm32-unknown-unknown --profile wasm-release
wasm-bindgen --target web --no-typescript --out-dir client/pkg \
    target/wasm32-unknown-unknown/wasm-release/alley_web.wasm
ls -l client/pkg
