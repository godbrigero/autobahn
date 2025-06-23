#!/usr/bin/env python3
import sys
import time
import socket
import subprocess
from zeroconf import Zeroconf, ServiceBrowser, ServiceStateChange

SERVICE = "_deploy._udp.local."
DISCOVERY_TIMEOUT = 2.0
TARGET_FOLDER = "~/Documents/autobahn/"
SSH_PASSWORD = "ubuntu"

pis: dict[str, str] = {}


def on_state_change(zeroconf: Zeroconf, service_type, name, state_change):
    if state_change is ServiceStateChange.Added:
        info = zeroconf.get_service_info(service_type, name)
        if not info:
            return
        ip = socket.inet_ntoa(info.addresses[0])
        pis[name] = ip


def discover_pis():
    zc = Zeroconf()
    ServiceBrowser(zc, SERVICE, handlers=[on_state_change])
    time.sleep(DISCOVERY_TIMEOUT)
    zc.close()
    return pis


def deploy(project_dir):
    if not pis:
        print("❌ No Pis discovered. Check your network and that they're advertising.")
        sys.exit(1)

    for name, ip in pis.items():
        target = f"ubuntu@{ip}:{TARGET_FOLDER}"
        print(f"\n->  Deploying to {name} ({ip})…")

        print(f"->  Fixing permissions on {name}...")
        fix_permissions_cmd = [
            "sshpass",
            "-p",
            SSH_PASSWORD,
            "ssh",
            f"ubuntu@{ip}",
            f"sudo chown -R ubuntu:ubuntu {TARGET_FOLDER} && sudo chmod -R 755 {TARGET_FOLDER}",
        ]
        ret = subprocess.run(fix_permissions_cmd)
        if ret.returncode != 0:
            print(f"⚠️  Permission fix failed for {name}, continuing anyway...")

        rsync_cmd = [
            "sshpass",
            "-p",
            SSH_PASSWORD,
            "rsync",
            "-av",
            "--progress",
            "--exclude-from=.gitignore",
            "--delete",  # Temporarily disabled due to permission issues
            f"{project_dir.rstrip('/')}/",
            target,
        ]
        print(rsync_cmd)
        ret = subprocess.run(rsync_cmd)
        if ret.returncode != 0:
            print(f"❌ rsync failed for {name}")
            continue

        print(f"->  Restarting service on {name}...")
        ssh_cmd = [
            "sshpass",
            "-p",
            SSH_PASSWORD,
            "ssh",
            f"ubuntu@{ip}",
            f"cd ~/Documents/autobahn/ && bash ./scripts/install.sh",
        ]
        ret = subprocess.run(ssh_cmd)
        if ret.returncode == 0:
            print(f"✅ Service restarted successfully on {name}")
        else:
            print(f"❌ Service restart failed on {name}")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(f"Usage: {sys.argv[0]} <project_dir>")
        print(
            "Since no project_dir is provided, we will deploy using the current directory (.)"
        )
        project_dir = "."
        input("Press enter to continue...")
    else:
        project_dir = sys.argv[1]

    print("🔍 Discovering Pis on the LAN…")
    discover_pis()
    print(f"✅ Found {len(pis)} Pi(s):", ", ".join(pis.values()))

    deploy(project_dir)
