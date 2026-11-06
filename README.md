# neofand

Per-component fan control and SMC telemetry for Mac Pro 7,1 running Linux.
The daemon runs as a host systemd service, independently of Docker and the web UI.

The multiplatform monitoring application has moved to
**[ChassisDeck](https://github.com/akhilv1/hardware-dashboard)**.
That repository owns the full dashboard, GPU builder, Linux/P620 collectors,
Proxmox and Docker integrations, and Docker/LXC packaging. It can monitor Linux
and Proxmox without this daemon.

## What this repository owns

- Mac Pro chassis fan curves, damping, safety floors and cover interlock.
- Apple SMC decoding and NVIDIA/AMD sensor integration; optional NVIDIA fan control.
- A published status snapshot and optional SQLite telemetry/history.
- The optional native API and small self-contained diagnostic console.
- Host systemd installation and cleanup that returns fans to automatic control.

Raw SMC reads remain in the control thread because its key-selection register
cannot safely be shared with another reader. The web and logging threads never
probe SMC or block cooling. See [hardware findings](MACPRO71-FINDINGS.md).

## Quick start: Mac Pro

Requires Linux, Rust 1.75+, root for fan control, and a working fan driver.
T2 Macs need a compatible patched `applesmc` driver; see the
[requirements](docs/fan-control.md#requirements).

```bash
cargo build --release
sudo install -m 0755 target/release/neofand /usr/local/bin/neofand
sudo install -m 0644 neofand.service /etc/systemd/system/neofand.service
sudo install -m 0644 neofand.conf.macpro71 /etc/neofand.conf
sudo systemctl daemon-reload
sudo systemctl enable --now neofand
sudo journalctl -u neofand -b --no-pager
```

Review the [worked config](neofand.conf.macpro71) and startup log before relying
on cooling. Tune thresholds for your GPUs, keep the cover interlock enabled,
and verify each fan’s source. NVIDIA card fan control is opt-in; the optional
startup power cap defaults to disabled.

Build with `--features web` to include the daemon’s telemetry store and web
console. Use the separate [ChassisDeck](https://github.com/akhilv1/hardware-dashboard)
for the full application; the embedded console remains available for diagnostics.

```bash
neofand --status  # Read the published snapshot without probing hardware
```

## Development and documentation

```sh
cargo test --locked
cargo test --locked --all-features
node --test web/chart-gaps.test.mjs
```

[Fan configuration and troubleshooting](docs/fan-control.md) ·
[Mac Pro hardware findings](MACPRO71-FINDINGS.md) ·
[Repository migration](docs/REPOSITORY-SPLIT.md) ·
[ChassisDeck](https://github.com/akhilv1/hardware-dashboard)

The daemon API and existing service/configuration paths are unchanged by the
split. Existing users do not need to restart or reinstall the daemon.

GPL-3.0-or-later. Forked from [T2FanRD](https://github.com/GnomedDev/T2FanRD).

## Security

Read [SECURITY.md](SECURITY.md). Fan control requires host hardware access; keep
the native API private. Control endpoints are disabled unless explicitly
configured and authorized. The application cannot provide cooling failover.
