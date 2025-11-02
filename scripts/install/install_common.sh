#!/bin/bash

apt-get update

if ! command -v rustup &> /dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source $HOME/.cargo/env
fi

rustup install stable
rustup default stable

apt-get install -y avahi-daemon avahi-utils libnss-mdns protobuf-compiler
