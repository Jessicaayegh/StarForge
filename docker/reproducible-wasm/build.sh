#!/bin/bash
set -e

echo "Starting reproducible WASM build..."

if [[ "$*" != *"--locked"* ]]; then
  echo "Error: Non-reproducible flag detected. You must use --locked to guarantee reproducible builds."
  exit 1
fi

if [[ "$*" != *"--release"* ]]; then
  echo "Error: Non-reproducible flag detected. You must use --release to guarantee reproducible builds."
  exit 1
fi

cargo build --target wasm32-unknown-unknown "$@"
