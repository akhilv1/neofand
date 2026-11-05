# Persistence and centralized archives

The dashboard container is a reader. Telemetry and alerts are written by host collectors to persistent host storage, then mounted read-only into Docker or LXC. Replacing the container does not erase these files. Destroying the host data or failing to back it up still loses history.

| Deployment | Persistent data | Retention |
| --- | --- | --- |
| Native Mac Pro + Docker UI | `/var/lib/neofand/telemetry.db` and the dashboard `telemetry/` directory | Native telemetry: configurable `[Logging] retention_days` (default 365); Ubuntu event snapshots: up to 30 days |
| Linux/P620 + Docker | Dashboard `telemetry/`, mounted at `/host/telemetry` | Linux chart samples: 30 days; Ubuntu snapshots: up to 30 days |
| Proxmox + LXC | `/var/lib/neofand-host` on the physical host, mounted at `/var/lib/neofand/telemetry` in the LXC | Linux chart samples: 30 days; Ubuntu snapshots: up to 30 days; drive-health events: 365 days, at most 10,000 |

Linux Docker Compose accepts `NEOFAND_HOST_DATA_DIR` for the host-side telemetry mount. The collector must write into that same directory. The Linux API accepts `NEOFAND_DATA_DIR` for its container-side location. For LXC, use a read-only bind mount; Proxmox container backups do **not** include host bind-mount contents, so back up the host directory separately.

Drive health is sampled once a minute on PVE. `drive-health-history.json` stores both the last known health baseline and durable event history. Preserve the complete file to prevent duplicate initial alerts after replacement. `NEOFAND_DRIVE_EVENT_RETENTION_DAYS` on `neofand-host-proxmox.service` changes retention (1–3650 days). Meaningful state/counter/wear/spare changes are logged; temperature and power-on-hour fluctuations are not. Three failed reads or missing-drive samples produce an alert; sleeping drives are neutral. Initial warnings are explicitly labeled as existing conditions. No SMART self-tests are started.

## Central archive storage

Keep live SQLite/WAL files on a local filesystem. WAL databases are not supported on NFS/SMB: https://www.sqlite.org/wal.html. Each host has its own writer and database; do not point several collectors at one file.

`dashboard/export_history.py` creates a consistent SQLite backup using the online backup API, adds atomic JSON snapshots and health history, then publishes a timestamped archive under a unique host directory. The archive destination may be a mounted NAS share. `--mount-root` prevents falling back to the host disk when that share is unmounted.

Example after mounting your archive share at `/mnt/neofand-archive`:

```sh
python3 export_history.py \
  --source /var/lib/neofand-host \
  --database /var/lib/neofand-host/linux.sqlite \
  --destination /mnt/neofand-archive \
  --mount-root /mnt/neofand-archive \
  --host pve-turret
```

On P620, use the dashboard `telemetry/` directory and its `linux.sqlite`. On the Mac Pro, use its dashboard `telemetry/` directory plus `--database /var/lib/neofand/telemetry.db`. Run from the host with read access to those paths. Schedule with a systemd timer or your existing backup scheduler; monitor export failures. Archives are immutable, do not delete older archives automatically, and need a retention policy on the archive storage.

This is centralized archival, not a shared live backend or multi-host query service. No archive destination is configured by default. To restore, stop the host collectors, extract into a temporary directory, verify the manifest, restore the database and JSON files to their original host data paths with correct ownership, then restart the collectors. Never overwrite an active SQLite database.
