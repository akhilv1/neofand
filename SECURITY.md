# Security model and reporting

NeoFand monitors real hardware. Treat the dashboard, telemetry archives and collector code as sensitive administrative software. A review and regression tests reduce risk; they are not a guarantee of isolation or an independent penetration-test certification.

## Supported deployment boundary

- The Proxmox UI runs in an **unprivileged LXC**. Give it only `/var/lib/neofand-host` mounted **read-only** at `/var/lib/neofand/telemetry`. No host root, `/dev`, `/proc`, `/sys`, journals, Docker socket, nesting or privileged-container flag is required.
- Web and API services run as an unprivileged account, with no Linux capabilities, a read-only service filesystem, private temporary/device views, syscall restrictions, and CPU/memory/process limits. Among dashboard services, only Caddy's port is externally reachable; the API listens on loopback. New LXC images disable SSH; use the Proxmox console or explicitly configure private SSH if needed. Host administrators still control the LXC kernel and policy.
- New LXC images bind Caddy to **loopback by default**, with a loopback source allowlist. Configure a specific management interface/CIDR or use an authenticated private proxy. A source-IP allowlist is **not user authentication**. Never forward the unauthenticated dashboard to the public internet. Do not trust `X-Forwarded-For` as proof of authorization.
- Linux Docker isolates the telemetry API on an internal network, with a separate loopback-published frontend, read-only mounts/root filesystems, non-root users, dropped capabilities and resource limits. Native Mac deployment still uses host networking to reach the Tailscale-bound daemon; that variant has a weaker network boundary and is not equivalent to LXC isolation.
- Read-only mounts prevent modification, not disclosure. An application compromise can read any supplied telemetry. The LXC shares the host kernel: keep Proxmox, the container OS, Caddy, Python and Docker patched.

## Host privileges

The ordinary sampler has a dedicated account and journal-read membership. Its writable directory is `/var/lib/neofand-host/telemetry`. The parent is root-owned and not group/world writable. Root-owned Proxmox inventory and SMART history stay in the parent. Root publications use exclusive randomized temporary files and atomic replacement; they do not follow attacker-selected temporary symlinks.

The separate Proxmox collector runs as root because local Proxmox inventory/guest-agent helpers and SMART reads require host privileges. It performs read-only operations but remains **trusted root code**; a read-only command convention is not a kernel-enforced read-only capability. Do not run unreviewed collector updates, grant dashboard users host shell access, or expose collector arguments as web controls. Ordinary persistent filesystem writes are restricted to the snapshot directory; the service also has private temporary storage and raw-device privileges. It cannot write the ordinary sampler's subdirectory through its filesystem mount.

## Data privacy and backups

General journal message bodies are omitted by default. Timestamps, unit names, severity and structured reboot/UPS/SMART events remain visible. For a private installation only, `NEOFAND_INCLUDE_JOURNAL_MESSAGES=1` enables best-effort-redacted message bodies. Redaction cannot reliably recognize arbitrary secrets. Treat opted-in logs as confidential and re-run the event collector after disabling the option; historical backups may retain old content.

Archives use mode `0600` in an exporter-owned `0700` per-host directory. Provision a trusted destination that is not group/world writable; a hostile storage server or administrator is outside this boundary. Use `--mount-root` to refuse exports when the expected mount is absent. Live SQLite/WAL databases stay on local disk; central storage receives consistent archive exports. Back up host bind-mount data separately from LXC rootfs backups.

## Native control API

Reboot/boot-selection controls are separate from monitoring and disabled unless configured. Keep them disabled unless necessary. Authorized callers must include `X-Neofand-Control: 1`; browser requests also require a matching origin. Dashboard proxies expose GET-only API routes. Request concurrency, body/header size, deadlines and database work are bounded. The native daemon performs hardware control and should not be deployed as an internet service.

## Reporting a vulnerability

Use GitHub's **private vulnerability reporting / Report a vulnerability** for this repository when available. If unavailable, open an issue asking for a private contact channel without including exploit details, secrets, addresses or logs. Include the affected commit, deployment type and a minimal synthetic reproduction through the private channel. Never test against systems you do not own.

Before announcing a release, verify the source commit, review the threat model, run regression checks and a whole-history secret scan, and publish checksums for new artifacts. Checksums establish integrity relative to the published manifest; they are not signatures. Container dependencies must be rebuilt regularly. An AI-assisted review does not mean the project is security certified.
