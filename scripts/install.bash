#!/bin/bash

cargo build --release

sudo cp target/release/autobahn /usr/local/bin/

sudo cp scripts/autobahn.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable autobahn

if [ ! -f /etc/autobahn/config.toml ]; then
    sudo mkdir -p /etc/autobahn
    sudo cp config.toml /etc/autobahn/
fi

sudo systemctl restart autobahn
