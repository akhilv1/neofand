# Repository split

The project was separated from [neofand](https://github.com/akhilv1/neofand)
at source commit [98d1157f80cffacd59c8c86ae27a48350b322f12](https://github.com/akhilv1/neofand/commit/98d1157f80cffacd59c8c86ae27a48350b322f12).

- [neofand](https://github.com/akhilv1/neofand): Mac Pro fan daemon, SMC/NVML
  integration, telemetry store, native API and small embedded diagnostic console.
- [Hardware Dashboard](https://github.com/akhilv1/hardware-dashboard): multiplatform application, collectors,
  integrations, GPU builder, Docker/Portainer and Proxmox LXC packaging.

The new application repository retains the relevant Git history, author names
and dates. Filtering out daemon-only paths changes commit IDs; historical PRs
and original commit links remain in neofand. The original repository history is
not rewritten. The legacy dashboard README there points to the new project.
GPL-3.0-or-later and upstream T2FanRD attribution are retained.

## Existing installations

No service restarts, live source replacement, database moves, port changes or
credential changes are required for this split. Existing neofand-* systemd and
Compose names, NEOFAND_* settings, /opt/neofand and telemetry directories stay
valid. Obtain future application updates from hardware-dashboard and daemon
updates from neofand. Follow the platform's existing deployment/backup guide.

The new repo retains the dashboard/ source directory, so relative paths in the
Linux frontend generator and LXC builder remain valid. Clone the new repo into
a separate source checkout; do not change a live Compose bind-mount path merely
to rename the source repository. Repoint any Git-based Portainer stack to the
new repo and matching Compose path during its next planned update.

Build metadata now supports a repository field. Old metadata without that field
keeps linking to original neofand commits. New builds should use the app's
tools/write-build-info.py or LXC builder so commit links point to the new repo.

The November security review describes the pre-split source. Its findings and
remaining limitations still apply to the corresponding component; it is not a
new audit or certification of the split repositories.
