# Portainer / Docker host inventory

The Portainer page is a read-only view of the local Docker engine on bare-metal hosts. It lists running/stopped containers, configured health checks, Compose project labels, CPU and memory usage, restart counts and cumulative I/O. Management remains in Portainer via the same-origin `/portainer/` link, shown when a Portainer CE/EE container is detected.

This is **not a Portainer API integration**: there is no API token, RBAC lookup, Portainer-managed stack API or remote-environment discovery. The collector queries the local engine, including containers that may not be managed as Portainer stacks. Compose grouping comes from `com.docker.compose.project`; standalone containers remain labeled separately.

## Install on a Docker host

Install `portainer_collect.py` beside the dashboard scripts and use `neofand-portainer.service` / `.timer` as templates. Adjust `User`, `ExecStart` and `ReadWritePaths` for your installation. The existing service account must already be authorized to query `/var/run/docker.sock`; the installer does not add Docker-group membership or change socket permissions. The destination directory must be owned by that account and not group/world writable.

```sh
sudo install -m 644 dashboard/neofand-portainer.service dashboard/neofand-portainer.timer /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now neofand-portainer.timer
sudo systemctl start neofand-portainer.service
```

The timer writes `telemetry/portainer.json` every 30 seconds. The Linux API exposes it at `/hardware/api/portainer`; the Mac Pro memory API exposes `/portainer` behind the dashboard Caddy route. Install the matching frontend, API and Caddy route together. No dashboard container mount or privilege changes are needed. The native fan daemon is unaffected.

## Security and data scope

Docker socket access is root-equivalent. The collector is trusted host code, even though its fixed command set only uses `ps`, field-filtered `inspect`, `info`, and non-streaming `stats`. A systemd sandbox cannot turn a Docker socket into a read-only capability. Keep it out of the dashboard container. No container environment variables, commands, logs, mount paths, registry credentials or arbitrary labels are requested/published. Container names, image names and the Compose project label are operational metadata and should still be treated as private.

Commands use the explicit local Unix socket, time out, and publish only an allowlisted JSON shape using an exclusive temporary file and atomic rename. Failures retain the last successful timestamp and mark the snapshot unavailable. A partial resource read is labeled unavailable, not zero. Collection is limited to 128 containers. This page has no start/stop/exec/configuration endpoints.

Container RAM follows Docker CLI cache-excluded accounting. CPU may exceed 100% across cores; I/O values are cumulative counters, not rates. Missing health checks are not classified as failures. See [Docker stats](https://docs.docker.com/reference/cli/docker/container/stats/) and [SECURITY.md](../SECURITY.md).
