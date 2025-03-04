#!/bin/bash

# Build and install the binary
cargo build --release
sudo cp target/release/autobahn /usr/local/bin/

# Install the main service
sudo cp scripts/autobahn.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable autobahn

# Create and install the update service with correct path
cat << EOF | sudo tee /etc/systemd/system/autobahn-update.service
[Unit]
Description=Autobahn Update Service
After=network.target

[Service]
Type=oneshot
WorkingDirectory=/home/ubuntu/Documents/autobahn
ExecStart=/home/ubuntu/Documents/autobahn/scripts/startup.bash
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
EOF

# Make startup script executable
chmod +x scripts/startup.bash

# Enable and start the update service
sudo systemctl daemon-reload
sudo systemctl enable autobahn-update
sudo systemctl start autobahn-update

# Create config directory if it doesn't exist
if [ ! -f /etc/autobahn/config.toml ]; then
    sudo mkdir -p /etc/autobahn
    sudo cp config.toml /etc/autobahn/
fi

# Restart the main service
sudo systemctl restart autobahn
