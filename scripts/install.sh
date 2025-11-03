#!/bin/bash

# Stop the service first to avoid "Text file busy" error
sudo systemctl stop autobahn

# Ensure Rust toolchain is configured
if ! command -v rustup &> /dev/null; then
    echo "Rustup not found. Installing Rust toolchain..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    if [ -f "$HOME/.cargo/env" ]; then
        source "$HOME/.cargo/env"
    fi
fi

# Set default toolchain if not already set
if ! rustup show | grep -q "default toolchain"; then
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
    sudo mkdir -p /etc/autobahn
    sudo cp config.toml /etc/autobahn/
fi

echo "Restarting autobahn service..."
sudo systemctl restart autobahn

echo "✅ Installation complete!"
