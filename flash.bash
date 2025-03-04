#!/bin/bash

apt update
apt install -y git

git clone https://github.com/PinewoodRobotics/autobahn.git
cd autobahn

./scripts/install.bash

