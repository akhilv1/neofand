# Security review — November 2026

An Astra agent reviewed the LXC, Docker, native web server, host collectors and archive exporter at baseline `a8ee6cb`. Findings were reproduced with synthetic data where practical, then reviewed again after remediation. This is an AI-assisted code and deployment review, not an independent security certification.

| Finding | Remediation |
| --- | --- |
| Root collector wrote predictable temporary files inside a lower-privilege user's directory | Root-owned parent, separate sampler directory, randomized exclusive temporary files, no-follow reads, bounded untrusted snapshots |
| Native server accepted unbounded sockets/requests and expensive queries | Connection, request, header/body, time, series and response budgets; SQLite execution deadline |
| Native privileged actions relied on peer address without browser CSRF protection | Explicit control header and origin/authority checks; controls remain opt-in |
| Unauthenticated LXC telemetry available across its network | New images loopback-only; explicit source-CIDR configuration; private authenticated proxy guidance; unused SSH/mail services disabled |
| Journal redaction missed some credential formats | Message bodies omitted by default; explicit private opt-in, improved best-effort scrubbing |
| Archive output could follow a per-host directory symlink | Trusted private directory, no-follow directory descriptor, descriptor-relative publication, private archive permissions |
| Container/service sandbox was incomplete | Non-root frontend, empty capability set, read-only mounts, systemd restrictions, process/memory/CPU limits, internal Docker API network |

## Verification

- Python regression tests cover the original temporary-file exploit, symlink rejection, archive permissions, journal privacy, bounded queries and split-directory compatibility.
- Rust security tests cover request framing/size/deadlines, connection limits, query bounds and browser-control authorization. Existing chart tests remain passing.
- The PVE deployment was checked for effective service restrictions, root/sampler ownership, successful SMART and guest collection, retained alert/history data, allowed management access and rejected non-GET API requests.
- Docker was validated in a separate temporary stack before deployment changes.
- Gitleaks `v8.30.1` image digest `sha256:b109bc5f8f76a38196a3e413704fc5b9e3c32360bce4e4b603bd6f45b3721dbb` scanned 62 reachable commits (119 MB) with no findings. Reports were redacted and retained privately. This does not prove the absence of secrets, particularly in binary images, deleted/dangling objects or external artifacts.

## Remaining boundaries

The root Proxmox/SMART collector is trusted host code with raw device privileges. LXC shares the host kernel. A read-only telemetry mount prevents writes but does not prevent disclosure. Source-CIDR restrictions are not user authentication, and the native Mac deployment retains host networking. The native web server shares a process with fan control; some subprocess-backed read endpoints still lack explicit child-process deadlines. Python has bounded concurrency and socket inactivity timeouts, not a total slow-client wall-clock deadline. Do not expose these services directly to the internet.

Native source fixes do not automatically update a running hardware daemon. Operators must rebuild and schedule that upgrade separately. Rebuild container images for dependency updates; mutable upstream image tags are not immutable releases. Published checksums are integrity checks, not signatures. See [SECURITY.md](../SECURITY.md) for access, privacy and reporting guidance.
