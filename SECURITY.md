# Daemon security model and reporting

neofand is trusted host software for Mac Pro hardware control. It needs writable
SMC fan attributes and may use NVML for configured GPU controls. Run it as a host
systemd service, not inside the unprivileged application container. Do not run
multiple controllers against the same fans or probe raw SMC keys concurrently.

The control loop must never block on web requests or database writes. Telemetry
is passed through a bounded non-blocking channel; overload drops samples rather
than delaying fan decisions. The web thread reads SQLite and never touches SMC.
Graceful shutdown returns fans to automatic control. The supplied systemd
ExecStopPost hook attempts the same cleanup after forced termination; verify
that its sysfs path is valid for the installed driver and hardware.

The optional native API is not an internet service. Read endpoints reveal
hardware and operational telemetry to anyone who can reach the listener. Bind
and firewall it to a trusted private interface. Reboot/boot-selection controls
are disabled unless configured, require the Tailscale identity allowlist and
X-Neofand-Control: 1, and enforce browser origin checks. Wildcard binding cannot
enable controls. Tailnet membership alone does not authorize a control action.
Request concurrency, sizes, deadlines and database work are bounded.

The separate [ChassisDeck security model](https://github.com/akhilv1/hardware-dashboard/blob/master/SECURITY.md)
covers containers, host collectors, persistence and ingress. Its GET-only proxy
does not grant access to native write endpoints. Keep that boundary intact.

Use GitHub private vulnerability reporting when available. Otherwise open an
issue requesting a private contact channel without exploit details or secrets.
Report dashboard/collector defects to the application repository, and fan/SMC or
native API defects here. Include affected commits and a synthetic reproduction.

The [November review](docs/security-review-2026-11.md) applies to the historical
combined project; see [migration notes](docs/REPOSITORY-SPLIT.md). It is not an
independent penetration-test certification or guarantee of isolation. Preserve
reviewed safety behavior, run regression checks, and review dependencies and
release artifacts before distributing updates.
