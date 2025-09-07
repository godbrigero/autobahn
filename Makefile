mac-deps:
	brew install sshpass

UBUNTU_TARGET = 10.47.65.12
SSH_PASSWORD = ubuntu
PYTHONPATH = ./.venv/bin/python

send-to-target:
	sshpass -p $(SSH_PASSWORD) rsync -av --progress --exclude-from=.gitignore --delete ./ ubuntu@$(UBUNTU_TARGET):~/Documents/autobahn/

hard-reset:
	sshpass -p $(SSH_PASSWORD) ssh ubuntu@$(UBUNTU_TARGET) 'sudo rm -rf /home/ubuntu/Documents/autobahn/ && mkdir -p /home/ubuntu/Documents/autobahn/'
	$(MAKE) send-to-target
	sshpass -p $(SSH_PASSWORD) ssh ubuntu@$(UBUNTU_TARGET) 'cd ~/Documents/autobahn/ && bash scripts/install.sh'

run-4mb-test:
	$(PYTHONPATH) -m tests.calc_stats