# Reproducible WASM Builder for Soroban Contracts
# Use a specific, stable Rust version to guarantee reproducibility
FROM rust:1.80.0-slim

# Install necessary tools
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    jq \
    && rm -rf /var/lib/apt/lists/*

# Add WASM target
RUN rustup target add wasm32-unknown-unknown

COPY build.sh /usr/local/bin/build-wasm
RUN chmod +x /usr/local/bin/build-wasm

WORKDIR /workspace
ENTRYPOINT ["build-wasm"]
