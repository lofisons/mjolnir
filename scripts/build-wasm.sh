#!/bin/sh
# Build the engine as WebAssembly and put it where the web app serves it.
set -e
cd "$(dirname "$0")/.."
cargo build --release -p mjolnir-web --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/mjolnir_web.wasm ui/public/mjolnir.wasm
