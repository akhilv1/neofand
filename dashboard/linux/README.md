# Linux / ThinkStation P620 dashboard

The Linux adapter collects CPU utilization, RAM/PSI, NVIDIA telemetry and readable hwmon temperatures without the Mac Pro SMC daemon. It never writes fan controls or firmware settings. The P620 identity is selected from DMI product family/version, not product_name; cheese reports `HPW (QEB2022B)` but both family/version identify ThinkStation P620.

Host: cheese / cheese.west-scoville.ts.net (100.96.103.107), Lenovo board 1046, S07KT6BA BIOS, Threadripper PRO 3945WX, 64 GB installed RAM and an RTX 3090 reported by NVIDIA at PCI 0000:61:00.0. Physical connector mapping and exact board vendor/variant remain unverified. Firmware slot descriptors do not map cleanly onto the six connector numbers. Sensor art uses an explicitly illustrative Founders Edition board in the primary GPU area.

## Deployment

Use the shared dashboard assets and `build_p620.py ../index.html DEST/index.html` to create the Linux page. Copy p620.js/css to DEST/assets and linux_api.py/linux_telemetry.py to DEST. Use this directory's Compose and Caddyfile. UID 1000 is the verified neo user on cheese; adjust it for another host.

The host `neofand-linux.service` updates an atomic JSON snapshot every two seconds and persists samples every 30 seconds to telemetry/linux.sqlite. History is retained for 30 days and begins at deployment. hourly deletes reuse freed SQLite pages; the database is not vacuumed while sampling. The API container reads the telemetry directory through a read-only mount and opens SQLite in read-only mode. No Docker socket or GPU device is mounted. Nvidia queries run as neo on the host. If a sample fails, old timestamps remain so the frontend shows a delay. Container health fails after 30 seconds without a sample.

Reuse the dashboard's Ubuntu event and storage collectors/timers. Journal snapshots are bounded to 30 days / 2000 warnings/errors, plus boot and UPS evidence. A missing clean shutdown is an inferred unexpected restart, never a confirmed power outage. No UPS integration is assumed.

`docker compose up -d` creates the neofand project, discoverable by Portainer. The dashboard binds only 127.0.0.1:4791; the API is internal to the Compose network. Publish privately using `tailscale serve --bg --set-path=/hardware http://127.0.0.1:4791`, retaining existing /portainer configuration. No Funnel. There are no control endpoints. Do not install/start the Apple SMC daemon on this platform.

The frontend uses shared theme, charts, ranges, auto/live controls, build stamp, storage and Ubuntu events. Dashboard/Power default 7 days, Thermal 6 hours and Event log 30 days. Total PSU consumption and chassis fan RPM are unavailable on this host; GPU board power is never labeled as system power. No invented fan readings or Apple cover/throttle status.

## Validation / rollback

`python3 -m unittest -v test_linux.py` verifies CPU deltas, NVIDIA units and unavailable values, bounded history and parameterized queries. Validate live samples against host nvidia-smi, /proc/meminfo and k10temp. Verify container health, API history, stale health behavior, events, storage, and existing Portainer route. Browser-check Dashboard, Sensors, Power, Thermal and Event log.

To remove only this deployment: stop the neofand Compose project and neofand-linux.service / neofand-events.timer / neofand-storage.timer, then remove only the /hardware Tailscale Serve mount. Keep telemetry for recovery and leave Portainer untouched.
