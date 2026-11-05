# Ubuntu event integration

Dashboard and Power initially use 7 days, Thermal 6 hours, Event log 30 days. Manual range changes are retained independently per page for the current browser session.

`system_events.py` runs as the existing `neo` host user (already in `adm`) under a hardened oneshot service every minute. It reads the Ubuntu journal and writes an atomic JSON snapshot to the existing read-only telemetry mount. It does not access SMC, change fan controls, or grant the container journal/root access. The memory API serves the snapshot at `/events`; Caddy exposes GET `/api/host-events` through the existing hardware routing.

The snapshot contains at most 2,000 newest warning/error journal records from 30 days plus up to 300 records from nut-monitor, nut-server and apcupsd. It reports truncation explicitly. UI merges these with hardware transitions, sorts by time, filters by source/range, and displays at most 500 records with an explicit limit notice. Logs are rendered as text; common credential and URL-query patterns are redacted. This is operational log access on the existing private dashboard, not a full journal export.

Boot timestamps are first retained records, not guaranteed exact boot instants. Shutdown timestamps are last retained records with systemd sequence evidence. An absent shutdown marker in the previous boot's newest 300 records is classified as an inferred unexpected restart: power loss, crash, forced reset and journal retention gaps remain possible. Only messages from the specified UPS services are labeled UPS power events. No UPS source means there is no confirmed outage detector. Journal persistence/rotation bounds available history; a requested range cannot recover logs already lost.

Collector failures preserve prior records and mark the snapshot unavailable; snapshots older than 180 seconds are stale in the UI. `/api/host-events` failure does not hide hardware transitions and vice versa. No reboot, power cut, or fan-control operation is used to test this integration.

For P620 deployment later, configure the service account and paths for that host, verify journal read access, and compare host/container data. Do not copy Mac Pro SMC control configuration onto the P620.
