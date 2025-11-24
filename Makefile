mac-deps:
	brew install sshpass

UBUNTU_TARGET = tripli.local
SSH_PASSWORD = ubuntu
PYTHONPATH = ./.venv/bin/python
PATH_TO_PROJECT = /opt/blitz/autobahn/

deploy-restarting: send-to-target
	sshpass -p $(SSH_PASSWORD) ssh ubuntu@$(UBUNTU_TARGET) 'sudo systemctl restart autobahn'

send-to-target:
	sshpass -p $(SSH_PASSWORD) rsync -av --progress --exclude-from=.gitignore --exclude='.git' --exclude='.git/**' --exclude='.idea' --exclude='.vscode' --exclude='.pytest_cache' --exclude='__pycache__' --delete --no-o --no-g --rsync-path="sudo rsync" ./ ubuntu@$(UBUNTU_TARGET):$(PATH_TO_PROJECT)

hard-reset:
	sshpass -p $(SSH_PASSWORD) ssh ubuntu@$(UBUNTU_TARGET) 'sudo rm -rf /home/ubuntu/Documents/autobahn/ && mkdir -p /home/ubuntu/Documents/autobahn/'
	$(MAKE) send-to-target
	sshpass -p $(SSH_PASSWORD) ssh ubuntu@$(UBUNTU_TARGET) 'cd ~/Documents/autobahn/ && bash scripts/install.sh'

run-4mb-test:
	$(PYTHONPATH) -m tests.calc_stats