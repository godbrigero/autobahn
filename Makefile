mac-deps:
	brew install sshpass

UBUNTU_TARGET = tripli.local agathaking.local
SSH_PASSWORD = ubuntu
PYTHONPATH = ./.venv/bin/python
PATH_TO_PROJECT = /opt/blitz/autobahn/

deploy:
	$(PYTHONPATH) scripts/deploy.py .

deploy-restarting: send-to-target
	for item in $(UBUNTU_TARGET); do \
		sshpass -p $(SSH_PASSWORD) ssh ubuntu@$$item 'cd $(PATH_TO_PROJECT) && sudo bash ./scripts/install.sh' & \
	done; \
	wait 

send-to-target:
	for item in $(UBUNTU_TARGET); do \
		sshpass -p $(SSH_PASSWORD) rsync -av --progress --exclude-from=.gitignore --exclude='.git' --exclude='.git/**' --exclude='.idea' --exclude='.vscode' --exclude='.pytest_cache' --exclude='__pycache__' --delete --no-o --no-g --rsync-path="sudo rsync" ./ ubuntu@$$item:$(PATH_TO_PROJECT) & \
	done; \
	wait \

hard-reset:
	sshpass -p $(SSH_PASSWORD) ssh ubuntu@$(UBUNTU_TARGET) 'sudo rm -rf /home/ubuntu/Documents/autobahn/ && mkdir -p /home/ubuntu/Documents/autobahn/'
	$(MAKE) send-to-target
	sshpass -p $(SSH_PASSWORD) ssh ubuntu@$(UBUNTU_TARGET) 'cd ~/Documents/autobahn/ && bash scripts/install.sh'

run-4mb-test:
	$(PYTHONPATH) -m tests.calc_stats