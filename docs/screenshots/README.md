# Live dashboard captures

Captured November 5, 2026 from the Mac Pro and ThinkStation P620 dashboards.
Only the sensor visualization regions are published. Identity headers, hostnames,
addresses, navigation and browser chrome have been removed by cropping; no
sensor readings were replaced. JPEG exports contain no EXIF metadata.

- `mac-pro-gpu-sensors.jpg`: three RTX A6000s, CPU, GPU intake and PSU load.
- `mac-pro-memory-sensors.jpg`: host memory/PSI and PSU blower.
- `p620-sensors.jpg`: Threadripper PRO, host memory and RTX 3090.

Images depict live telemetry over illustrative chassis artwork. Anchor positions
are approximate. P620 board variant/slot are not verified; its PSU consumption
and chassis fan speeds are unavailable. These are documentation samples, not
performance benchmarks.

- `proxmox-overview.jpg`: resource dials and VM/LXC tiles from the live Proxmox dashboard. Guest names and IDs were replaced with generic labels using `?redact=1&capture=1` presentation mode before capture; host identity, sidebar and network address were omitted before capture. Numeric readings and guest states are unchanged. This mode only affects display: it does not sanitize the API or make the dashboard safe to publish.
