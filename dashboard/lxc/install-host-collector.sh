#!/bin/sh
set -eu
# Run on the PHYSICAL Proxmox host, never inside the LXC. Uses no raw SMC access.
[ "$(id -u)" = 0 ] || { echo 'Run as root on the physical host.' >&2; exit 1; }
if systemd-detect-virt --container --quiet; then
  echo 'Refusing to collect container limits as physical-host telemetry.' >&2; exit 1
fi
for bin in python3 journalctl systemctl; do command -v "$bin" >/dev/null; done
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
getent passwd neofand-collector >/dev/null || useradd --system --home-dir /var/lib/neofand-host --shell /usr/sbin/nologin neofand-collector
for group in systemd-journal; do
  if getent group "$group" >/dev/null; then usermod -aG "$group" neofand-collector; fi
done
install -d -m 755 /opt/neofand-host
# Stop writers before migrating SQLite together with its WAL/SHM files.
systemctl stop neofand-host-proxmox.timer neofand-host-events.timer neofand-host-storage.timer 2>/dev/null || true
systemctl stop neofand-host neofand-host-events neofand-host-storage neofand-host-proxmox 2>/dev/null || true
python3 - <<'MIGRATE'
import os,pwd,stat
from pathlib import Path
root=Path('/var/lib/neofand-host')
if root.is_symlink():raise SystemExit('Refusing symlink collector directory')
root.mkdir(mode=0o755,exist_ok=True)
os.chown(root,0,0);root.chmod(0o755)
child=root/'telemetry'
if child.is_symlink():raise SystemExit('Refusing symlink telemetry directory')
child.mkdir(mode=0o755,exist_ok=True)
user=pwd.getpwnam('neofand-collector')
os.chown(child,user.pw_uid,user.pw_gid);child.chmod(0o755)
for name in ('linux.json','linux.sqlite','linux.sqlite-wal','linux.sqlite-shm','system-events.json','storage.json'):
    old=root/name;new=child/name
    if old.is_symlink() or new.is_symlink():raise SystemExit('Refusing symlink data file: '+name)
    if old.exists():
        if not stat.S_ISREG(old.stat().st_mode) or new.exists():raise SystemExit('Ambiguous migration: '+name)
        old.rename(new)
    if new.exists():os.chown(new,user.pw_uid,user.pw_gid)
for name in ('proxmox.json','drive-health-history.json'):
    path=root/name
    if path.is_symlink():raise SystemExit('Refusing symlink privileged snapshot')
    if path.exists():os.chown(path,0,0);path.chmod(0o644)
MIGRATE
# The sampler needs journals, not general log/graphics administrative groups.
for group in adm video render; do gpasswd -d neofand-collector "$group" >/dev/null 2>&1 || true; done
install -m 644 "$script_dir"/linux_telemetry.py "$script_dir"/system_events.py "$script_dir"/storage_collect.py "$script_dir"/storage_topology.py /opt/neofand-host/
install -m 644 "$script_dir"/export_history.py "$script_dir"/PERSISTENCE.md /opt/neofand-host/
cat > /etc/systemd/system/neofand-host.service <<'UNIT'
[Unit]
Description=NeoCharge physical-host telemetry
After=multi-user.target
[Service]
User=neofand-collector
ExecStart=/usr/bin/python3 -B /opt/neofand-host/linux_telemetry.py /var/lib/neofand-host/telemetry
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectKernelLogs=true
ProtectControlGroups=true
RestrictSUIDSGID=true
RestrictRealtime=true
LockPersonality=true
TasksMax=64
MemoryMax=256M
ReadWritePaths=/var/lib/neofand-host/telemetry
CapabilityBoundingSet=
RestrictAddressFamilies=AF_UNIX
UMask=0022
[Install]
WantedBy=multi-user.target
UNIT
for kind in events storage; do
  case "$kind" in
    events) script=system_events.py; output=system-events.json ;;
    storage) script=storage_collect.py; output=storage.json ;;
  esac
  cat > "/etc/systemd/system/neofand-host-$kind.service" <<UNIT
[Unit]
Description=NeoCharge physical-host $kind
[Service]
Type=oneshot
User=neofand-collector
ExecStart=/usr/bin/python3 -B /opt/neofand-host/$script /var/lib/neofand-host/telemetry/$output
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectKernelLogs=true
ProtectControlGroups=true
RestrictSUIDSGID=true
RestrictRealtime=true
LockPersonality=true
TasksMax=64
MemoryMax=256M
ReadWritePaths=/var/lib/neofand-host/telemetry
CapabilityBoundingSet=
RestrictAddressFamilies=AF_UNIX
TimeoutStartSec=90
UMask=0022
UNIT
  cat > "/etc/systemd/system/neofand-host-$kind.timer" <<UNIT
[Unit]
Description=Refresh NeoCharge physical-host $kind
[Timer]
OnBootSec=30s
OnUnitActiveSec=15s
[Install]
WantedBy=timers.target
UNIT
done
if command -v pvesh >/dev/null; then
  install -m 644 "$script_dir/proxmox_collect.py" /opt/neofand-host/
  cat > /etc/systemd/system/neofand-host-proxmox.service <<'UNIT'
[Unit]
Description=Read-only local Proxmox inventory snapshot
[Service]
Type=oneshot
ExecStart=/usr/bin/python3 -B /opt/neofand-host/proxmox_collect.py /var/lib/neofand-host/proxmox.json
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectKernelLogs=true
ProtectControlGroups=true
RestrictSUIDSGID=true
RestrictRealtime=true
LockPersonality=true
TasksMax=64
MemoryMax=256M
ReadWritePaths=/var/lib/neofand-host
ReadOnlyPaths=/var/lib/neofand-host/telemetry
RestrictAddressFamilies=AF_UNIX AF_NETLINK AF_INET AF_INET6
TimeoutStartSec=90
UMask=0022
UNIT
  cat > /etc/systemd/system/neofand-host-proxmox.timer <<'UNIT'
[Unit]
Description=Refresh Proxmox inventory
[Timer]
OnBootSec=30s
OnUnitActiveSec=60s
[Install]
WantedBy=timers.target
UNIT
  systemctl daemon-reload
  systemctl enable --now neofand-host-proxmox.timer
fi
systemctl daemon-reload
systemctl enable --now neofand-host.service neofand-host-events.timer neofand-host-storage.timer
printf '\nCollector started. Bind ONLY /var/lib/neofand-host into the LXC read-only.\n'
