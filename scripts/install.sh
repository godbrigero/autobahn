#!/bin/bash

sudo systemctl stop autobahn

if ! command -v rustup &> /dev/null; then
    echo "Rustup not found. Installing Rust toolchain..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

if ! rustup toolchain list | grep -q "(default)"; then
    echo "Setting up default Rust toolchain..."
    rustup default stable
fi

echo "Building autobahn..."
cargo build --release

if [ ! -f target/release/autobahn ]; then
    echo "Build failed - binary not found"
    exit 1
fi

echo "Installing binary..."
sudo cp target/release/autobahn /usr/local/bin/

sudo cp scripts/autobahn.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable autobahn

if [ ! -f /etc/autobahn/config.toml ]; then
    sudo mkdir -p /etc/autobahn/
fi

sudo cp ./config.toml /etc/autobahn/config.toml

echo "Restarting autobahn service..."
sudo systemctl restart autobahn

echo "✅ Installation complete!"
