#!/bin/bash

echo "Checking Autobahn service status..."
sudo systemctl status autobahn | grep Active

echo -e "\nChecking process..."
ps aux | grep autobahn | grep -v grep

echo -e "\nChecking recent logs..."
sudo journalctl -u autobahn -n 10 