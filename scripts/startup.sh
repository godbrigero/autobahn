#!/bin/bash

cd /home/ubuntu/Documents/autobahn

# Update Rust toolchain if needed
rustup update

# Clean and rebuild
cargo clean
cargo build --release -Znext-lockfile-bump

# Only proceed if build was successful
if [ -f target/release/autobahn ]; then
    sudo cp target/release/autobahn /usr/local/bin/
    sudo systemctl daemon-reload
    sudo systemctl restart autobahn
else
    echo "Build failed - autobahn binary not created"
    exit 1
fi
