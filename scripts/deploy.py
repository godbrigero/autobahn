#!/usr/bin/env python3
import sys
import os
import time
import socket
import subprocess
from pathlib import Path
from zeroconf import Zeroconf, ServiceBrowser, ServiceStateChange

SERVICE = "_autobahn._udp.local."
DISCOVERY_TIMEOUT = 2.0
TARGET_FOLDER = "/opt/blitz/autobahn/"
SSH_PASSWORD = "ubuntu"


def discover_pis():
    pis: dict[str, str] = {}

    def on_state_change(zeroconf: Zeroconf, service_type, name, state_change):
        if state_change == ServiceStateChange.Added:
            info = zeroconf.get_service_info(service_type, name)
            if not info or not info.addresses:
                return
            ip = socket.inet_ntoa(info.addresses[0])
            pis[name] = ip

    zc = Zeroconf()
    ServiceBrowser(zc, SERVICE, handlers=[on_state_change])
    time.sleep(DISCOVERY_TIMEOUT)
    zc.close()
    return pis


def send_to_target(pis: dict[str, str], project_root: Path):
    """Sync files to all discovered Pis using rsync."""
    if not pis:
        print("❌ No Pis discovered. Check your network and that they're advertising.")
        sys.exit(1)

    procs = []
    for name, ip in pis.items():
        print(f"-> Syncing files to {name} ({ip})")
        # Build SSH command with options to disable host key checking
        # rsync uses -e to specify the SSH command, and sshpass wraps it
        ssh_cmd = (
            f"sshpass -p {SSH_PASSWORD} ssh "
            "-o StrictHostKeyChecking=no "
            "-o UserKnownHostsFile=/dev/null"
        )
        rsync_cmd = [
            "rsync",
            "-av",
            "--progress",
            "--exclude-from=.gitignore",
            "--exclude=.git",
            "--exclude=.git/**",
            "--exclude=.idea",
            "--exclude=.vscode",
            "--exclude=.pytest_cache",
            "--exclude=__pycache__",
            "--delete",
            "--no-o",
            "--no-g",
            "--rsync-path=sudo rsync",
            "-e",
            ssh_cmd,
            "./",
            f"ubuntu@{ip}:{TARGET_FOLDER}",
        ]
        print(f"Running: {' '.join(rsync_cmd)}")
        proc = subprocess.Popen(rsync_cmd, cwd=project_root)
        procs.append((name, proc))

    # Wait for all to complete
    errors = 0
    for name, proc in procs:
        ret = proc.wait()
        if ret == 0:
            print(f"✅ Synced files to {name}")
        else:
            print(f"❌ Failed to sync files to {name} (exit code {ret})")
            errors += 1

    return errors == 0


def deploy_restarting(pis: dict[str, str], project_root: Path):
    if not pis:
        print("❌ No Pis discovered. Check your network and that they're advertising.")
        sys.exit(1)

    # First, sync files to all Pis
    print("\n📦 Syncing files to Pis...")
    if not send_to_target(pis, project_root):
        print("❌ File sync failed. Aborting restart.")
        sys.exit(1)

    # Then restart services
    print("\n🔄 Restarting services on Pis...")
    procs = []
    for name, ip in pis.items():
        print(f"-> Restarting autobahn service on {name} ({ip})")
        # Use exactly the Makefile restart command format
        # The command must be wrapped in quotes to execute on remote side
        ssh_cmd = f"cd {TARGET_FOLDER} && sudo bash ./scripts/install.sh"
        full_cmd = [
            "sshpass",
            "-p",
            SSH_PASSWORD,
            "ssh",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            f"ubuntu@{ip}",
            ssh_cmd,
        ]
        print(f"Running: {' '.join(full_cmd)}")
        proc = subprocess.Popen(full_cmd)
        procs.append((name, proc))

    # Wait for all to complete
    errors = 0
    for name, proc in procs:
        ret = proc.wait()
        if ret == 0:
            print(f"✅ Restarted service on {name}")
        else:
            print(f"❌ Failed to restart service on {name} (exit code {ret})")
            errors += 1

    if errors:
        print(f"\nDeployment finished with {errors} error(s)")
    else:
        print("\n🚀 Deployment & restart complete!")


if __name__ == "__main__":
    # Get project root from command line argument or use current directory
    project_root = Path(sys.argv[1] if len(sys.argv) > 1 else os.getcwd()).resolve()

    print("🔍 Discovering Pis on the LAN…")
    pis = discover_pis()
    print(f"✅ Found {len(pis)} Pi(s):", ", ".join(pis.values()))

    deploy_restarting(pis, project_root)
