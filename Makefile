mac-deps:
	brew install sshpass

UBUNTU_TARGET = tynan.local
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

.PHONY: test coverage coverage-html

test:
	cargo test

# Test coverage (% lines/regions) via cargo-llvm-cov.
# One-time setup:
#   cargo install cargo-llvm-cov
#   rustup component add llvm-tools-preview
coverage:
	@command -v cargo-llvm-cov >/dev/null 2>&1 || { \
		echo "Missing cargo-llvm-cov. Install with:"; \
		echo "  cargo install cargo-llvm-cov"; \
		echo "  rustup component add llvm-tools-preview"; \
		exit 1; \
	}
	cargo llvm-cov --workspace --summary-only --ignore-filename-regex "src/main\\.rs$$" --fail-under-lines 90

coverage-html:
	@command -v cargo-llvm-cov >/dev/null 2>&1 || { \
		echo "Missing cargo-llvm-cov. Install with:"; \
		echo "  cargo install cargo-llvm-cov"; \
		echo "  rustup component add llvm-tools-preview"; \
		exit 1; \
	}
	cargo llvm-cov --workspace --html --ignore-filename-regex "src/main\\.rs$$"

build-docker:
	docker build -t autobahn .

run-docker:
	docker run -p 8080:8080 autobahn