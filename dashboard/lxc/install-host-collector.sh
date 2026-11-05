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
for group in adm systemd-journal video render; do
  if getent group "$group" >/dev/null; then usermod -aG "$group" neofand-collector; fi
done
install -d -m 755 /opt/neofand-host
install -d -m 755 -o neofand-collector -g neofand-collector /var/lib/neofand-host
install -m 644 "$script_dir"/linux_telemetry.py "$script_dir"/system_events.py "$script_dir"/storage_collect.py "$script_dir"/storage_topology.py /opt/neofand-host/
cat > /etc/systemd/system/neofand-host.service <<'UNIT'
[Unit]
Description=NeoCharge physical-host telemetry
After=multi-user.target
[Service]
User=neofand-collector
ExecStart=/usr/bin/python3 -B /opt/neofand-host/linux_telemetry.py /var/lib/neofand-host
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=/var/lib/neofand-host
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
ExecStart=/usr/bin/python3 -B /opt/neofand-host/$script /var/lib/neofand-host/$output
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=/var/lib/neofand-host
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
ProtectHome=read-only
ReadWritePaths=/var/lib/neofand-host
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
