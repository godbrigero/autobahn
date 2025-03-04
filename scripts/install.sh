#!/bin/bash

cargo build --release
sudo cp target/release/autobahn /usr/local/bin/

sudo cp scripts/autobahn.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable autobahn

cat << EOF | sudo tee /etc/systemd/system/autobahn-update.service
[Unit]
Description=Autobahn Update Service
After=network.target

[Service]
Type=oneshot
WorkingDirectory=/home/ubuntu/autobahn
ExecStart=/home/ubuntu/autobahn/scripts/startup.sh
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
EOF

chmod +x scripts/startup.sh

sudo systemctl daemon-reload
sudo systemctl enable autobahn-update
sudo systemctl start autobahn-update

if [ ! -f /etc/autobahn/config.toml ]; then
    sudo mkdir -p /etc/autobahn
    sudo cp config.toml /etc/autobahn/
fi

sudo systemctl restart autobahn
