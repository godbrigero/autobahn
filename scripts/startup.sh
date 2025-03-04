#!/bin/bash

cd /home/ubuntu/autobahn

git pull

cargo build --release

sudo cp target/release/autobahn /usr/local/bin/

sudo systemctl daemon-reload
sudo systemctl restart autobahn
