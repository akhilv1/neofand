# NeoCharge Hardware: native Proxmox LXC template

Ubuntu 24.04 amd64 system-container template, based on Proxmox's official signed template catalog. Caddy and the Python API run as native systemd services, without Docker, nesting, a privileged LXC, host devices, or the Docker socket. The image starts with no telemetry or credentials. SSH host keys and machine ID are unique per booted instance; root is locked until configured through Proxmox.

## Install

1. Download the `.tar.zst` and matching `.sha256` from the private hardware downloads page. On the Proxmox host, place them in `/var/lib/vz/template/cache/`, then run `sha256sum -c IMAGE.tar.zst.sha256`.
2. Create an **unprivileged** container from the template using the Proxmox GUI. Suggested resources: 1 core, 512 MiB RAM, 2 GiB root disk, DHCP on your private management bridge. Set an SSH public key in the wizard if you want SSH; the Proxmox console also works. No nesting or GPU passthrough required.
3. Install the separately packaged `host-collector.tar.gz` on the **physical Proxmox host**: extract it, inspect `install-host-collector.sh`, then run it as root. It installs a dedicated unprivileged sampler user and three read-only collectors; it does not install GPU drivers or change cooling.
4. While the container is stopped, bind only the output directory read-only. Replace `120` with your unused CT ID:

```sh
pct set 120 -mp0 /var/lib/neofand-host,mp=/var/lib/neofand/telemetry,ro=1
pct start 120
```

Open `http://CONTAINER_IP:8080/hardware/`. Restrict access to your management network with the Proxmox firewall, or publish this route using your existing private Tailscale proxy. The API is bound to loopback inside the container. No public ingress, TLS certificate, Tailscale identity or shared password is baked into the template.

CLI creation example (choose an unused ID, actual template filename, storage, bridge and public-key path):

```sh
pct create 120 local:vztmpl/IMAGE.tar.zst --hostname hardware \
  --ostype ubuntu --unprivileged 1 --cores 1 --memory 512 --swap 0 \
  --rootfs local-lvm:2 --net0 name=eth0,bridge=vmbr0,ip=dhcp,firewall=1 \
  --ssh-public-keys /root/.ssh/authorized_keys
```

The Linux sampler publishes CPU, host RAM/PSI, readable hwmon temperatures and NVIDIA metrics when host nvidia-smi is available. It retains 30 days of chart history, samples live every 2 seconds and persists history every 30 seconds. Host journal and storage snapshots are refreshed separately. Missing PSU/fan/GPU readings remain unavailable. AMD amdgpu sysfs supplies GPU busy percent, VRAM bytes and temperature when exposed. Intel GPU utilization is not included. Integrated-GPU package power is not mislabeled as board power. On Proxmox, a separate root-local collector uses only `pvesh get` to publish node resource usage, VM/LXC status and recent task results every 60 seconds. No API token is needed and no guest controls are exposed; task usernames, raw logs and guest configurations are excluded. P620 artwork is enabled only when the host identifies as P620; placement and GPU board variant remain illustrative.

An LXC's `/proc` and cgroup readings describe its limits, not necessarily the physical host. Therefore the image deliberately does **not** run the collector itself. With no host bind mount, it shows unavailable telemetry instead of fabricated server readings. Only generated telemetry is shared, never the host root, `/proc`, `/sys`, journals or GPU devices. The snapshot directory is readable by the container's mapped user; SQLite/WAL files are created with umask 0022. Only sanitized event snapshots reach the API.

## Operations

- In the LXC: `systemctl status neofand-api neofand-web` and `journalctl -u neofand-api -u neofand-web`.
- On the host: `systemctl status neofand-host neofand-host-events.timer neofand-host-storage.timer`.
- Keep `/var/lib/neofand-host` backed up separately: Proxmox bind-mount data is not included in normal container backups.
- No power-outage proof without UPS events. Restart gaps are explicitly inferred.
- Build checks cover packages, native services, API behavior and rootfs identity cleanup. Deployment checks must verify `pct` boot, networking and service health on the target node; the download manifest records the verified deployment.

## Build

Run `build-lxc.sh REPOSITORY_ROOT OUTPUT_DIRECTORY` on an amd64 Linux builder with Docker, Python 3, curl, zstd and gpgv. It verifies Proxmox's catalog signature and the Ubuntu template SHA512, builds the native service image, exports a rootfs archive, and writes SHA256, build metadata and the host collector package. Docker is only a build tool. No running container or host telemetry is exported. Package versions and source commit are recorded in the image.

References: [Proxmox container documentation](https://github.com/proxmox/pve-docs/blob/master/pct.adoc), including system templates, unprivileged containers and read-only bind mounts.
