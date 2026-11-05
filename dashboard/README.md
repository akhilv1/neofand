# NeoCharge hardware dashboard

Live Sensors view: https://neo-mac-pro.west-scoville.ts.net/hardware/#sensors

This directory is the source of the separate Portainer `neofand` dashboard stack. It serves a NeoCharge-styled monitoring frontend and proxies read-only requests to the host daemon. The daemon's original embedded console remains in `web/dashboard.html`.

## Files

- `index.html`: dashboard, charts, and two annotated chassis panels. The former raw sensor-reading section is removed.
- `assets/sensor-diagram.js`: updates the callouts and utilization bars from `/api/latest`, plus the independent host-memory snapshot.
- `assets/sensor-diagram.css`: engineering diagram layout and responsive reading cards.
- `assets/runtime.js` and `assets/runtime.css`: Auto/Live switches, refresh scheduling, and the build/sample footer.
- `assets/macpro-gpu-approved.png`: approved illustration with three A6000 cards.
- `assets/macpro-memory-cutout.png`: reverse chassis view with its gray background removed.
- `assets/rtx-a6000-card.png`: approved transparent card artwork for future composition edits.
- `assets/gpu-library/`: searchable catalog, primary specification links, two dimensioned SVG views per GPU profile, and an illustrative A6000 system perspective render. See its README for dimension assumptions.
- `Caddyfile` and `compose.yaml`: read-only frontend deployment.
- `memory_api.py`: a standard-library Python companion that reads only the mounted host `meminfo` and memory PSI files.

The HTML requests the proxy at `/hardware/api/*`. The main router removes `/hardware` before forwarding to Caddy on loopback port 4791. Caddy accepts only GET requests: `/api/host-memory` goes to the companion's loopback port 4792, while other API reads go to the daemon on port 9977. The existing router identity gate controls access to the page. The companion runs without root, with its source and two host files mounted read-only.

## Callout sources

Each A6000 displays its `gpuN_temp_c`, `gpuN_fan_pct`, and `gpuN_power_w` channels, with a utilization bar from `gpuN_util_gpu_pct`. CPU package temperature comes from `hwmon/coretemp_package_id_0`; CPU load is `host/cpu_pct`. The GPU-side PSU box shows total system draw from `smc_power/PSTR` without a leader line. PSU temperature is the maximum of `smc_temp/Tp0P`, `Tp1P`, and `Tp2P`.

The reverse panel annotates the memory banks and PSU blower. Host RAM used is `MemTotal - MemAvailable`; free, available, reclaimable cache, and swap usage remain separate readings. Pressure is Linux PSI `some avg10` (the percent of the last ten seconds with at least one task waiting on memory); full-stall pressure is also shown. This is actual host PSI, not a color inferred from RAM utilization. See https://docs.kernel.org/accounting/psi.html. Both chassis image elements have identical dimensions.

The fan map follows the measured roles in `MACPRO71-FINDINGS.md`: fan 1 is the PSU, fan 4 is CPU cooling, and fans 2 and 3 serve the GPU bay. The latter two have not been distinguished individually, so their annotation shows a shared cooling zone. All image anchor positions are approximate.

The GPU callouts are arranged top to bottom as GPU 2, GPU 0, GPU 1 based on PCI ancestry and firmware slot labels observed on the server: `fc:00.0` / Slot 5, `0d:00.0` / Slot 4, and `0f:00.0` / Slot 3. Recheck this map after moving cards or changing GPU enumeration. Raster placement is illustrative.

Auto refreshes the current page every 30 seconds. Live mode is available on Dashboard and Sensors: readings update every two seconds, and Dashboard charts refresh every ten seconds. Turning Auto off pauses periodic requests; manual Refresh remains available. Preferences are saved locally. Background tabs do not poll, and refresh requests do not overlap. A failed refresh marks readings unavailable instead of presenting the previous values as fresh. Missing channels display a dash.

Callout text and spacing use container-relative `clamp()` sizes with explicit minima and maxima. Below a 760 px panel width, reading cards move below the chassis image to avoid collisions.

The lower-left footer shows deployed source metadata and the latest recorded telemetry timestamp (the maximum `ts` from `/api/latest`, not the time an HTTP request completed). Deadband logging can leave unchanged channels with older timestamps. Live status requires a successful recent snapshot; stale samples and failed requests show Delayed or Disconnected.

## Deployment

The deployed source directory is `/home/neo/services/neofand-dashboard`. Portainer owns the `neofand` stack (endpoint 1, stack ID 9). Copy this directory's page, assets, and Caddyfile there, then apply `compose.yaml` through the stack editor. Its volumes mount the page and assets read-only; the host systemd service continues collecting telemetry and controlling fans.

The page is static and has no build step. Check JavaScript syntax with `node --check assets/sensor-diagram.js`, then verify both live panels, changing values, utilization bars, host memory, equal image dimensions, and the reading-card layout on narrow screens. The memory companion has its own health check; its failure leaves the daemon callouts updating and marks the memory readings unavailable.

After committing the source, write deployment-only `assets/build-info.json` with the full source commit, branch, and an optional PR number. Example: `{"commit":"<full SHA>","branch":"codex/sensor-callouts","pr_number":null}`. Do not commit this generated file or invent a PR number. With no PR, the footer labels the deployment as a branch build. Recreate the dashboard container after replacing its bind-mounted `index.html` so it sees the new inode.

The GPU library currently contains 63 board profiles and 126 SVG projections. Published dimensions determine each envelope; unpublished cooler thickness uses a labeled nominal slot envelope. Apple MPX templates remain explicitly uncalibrated until mechanical dimensions are available. They are not used for dimensional fit claims. The generated 3D preview is an illustration, not a mechanically verified 3D mesh.

## Storage page

`#storage` shows physical disks, partitions, mount points, and filesystem used/available space. Loop devices are excluded. Binary units are explicit. Raw drive capacity is kept separate from filesystem capacity. Unmounted partitions (including APFS) have unknown usage, never zero usage. Totals deduplicate mounted filesystems by device ID; reserved filesystem space is separate from available space.

`storage_collect.py` runs on the host as user neo, using read-only lsblk and statvfs calls. It checks mount device identity to avoid measuring a parent filesystem after an unmount. Install `neofand-storage.service` and `.timer` in /etc/systemd/system, create the deployment's telemetry directory owned by neo, and enable/start the timer. A failed collection leaves the old timestamp intact. The snapshot is atomically replaced every 15 seconds. No drives are mounted or modified.

Only the telemetry directory is mounted read-only into the memory companion; `/api/host-storage` proxies GET /storage. The page follows Auto (30s) and manual Refresh, marks snapshots older than 60s stale, and shows unavailable on fetch failure. The chart range does not alter this latest snapshot.

### Storage categories

`storage_breakdown.py` performs a metadata-only allocated-block scan as user neo. Its systemd timer runs 30 minutes after the previous scan completes, with idle I/O priority, nice 19, and a 50% of one CPU quota. It never reads file contents or follows symlinks; nested host mounts are skipped, device identity is checked, and hard links are counted once per filesystem. A scan has an 1100-second budget and explicitly marks incomplete results.

Categories use folder and extension heuristics: backup folders take precedence, model folders and formats (including Hugging Face/Ollama caches) are Models; remaining files are classified as media, archives, development files, containers, caches/logs, documents, system, or Other. Values use allocated blocks, not logical sizes. Shared copy-on-write extents and concurrent writes can make file estimates differ from filesystem used space; the UI flags overestimates and shows positive remainder as Unclassified & overhead. Protected files cannot be classified by this unprivileged scan. Unmounted partitions remain unavailable.

The companion merges the separate atomic `telemetry/storage-breakdown.json` snapshot into its storage response. A missing breakdown never blocks current space totals. Each filesystem shows its own scan time, unreadable count, category drilldowns, and largest directory groups. No individual filenames or contents are exported. Category data older than 90 minutes is labeled outdated. Refresh fetches the cached scan; it does not start a disk traversal. Install/enable `neofand-storage-breakdown.service` and `.timer` alongside the basic storage timer. Run `python3 dashboard/test_storage_breakdown.py` to check classification, mount boundaries, hard-link deduplication, symlink handling, and deadlines.

The collector includes the OS-reported GPT partition type and name. The UI hides EFI system and auxiliary metadata types (BIOS boot, Microsoft reserved, Apple boot/label, or explicitly named metadata types), rather than hiding all FAT partitions or guessing from size. Hidden partitions remain in the read-only inventory; displayed usage totals cover visible filesystems and raw capacity still includes all partitions. Unreadable data partitions have a hatched Unknown storage usage bar with no fabricated percentage. Filesystem badges use familiar APFS / exFAT / XFS / ext4 labels, with the partition type and GUID available in the tooltip.


## OEM GPU system builder

The GPU library now contains 55 OEM/reference profiles and a Mac Pro chassis builder with mixed-card placement, measured overrides, saved layouts, JSON import/export, and collision/length checks. It records published slot widths, derives connector spacing from a configurable nominal 20.32 mm pitch, reserves Apple I/O and MPX connector pairs, and limits MPX to two modules. Missing dimensions remain visibly uncertain. Popular OEM families use clean installed-side renders; other models have distinct board-family illustrations. See `assets/gpu-library/README.md` and `assets/gpu-library/builder/SLOT-MECHANICS.md`.


### Storage topology and persistent indexing

The current collector collapses repeated lsblk nodes by device identity, so Linux md RAID and LVM filesystems are shown once with their backing drives. System identifies the running root filesystem; Boot identifies a currently mounted /boot, /boot/efi or /efi filesystem. Roles propagate to backing drives, including hidden EFI partitions. An unused EFI partition alone does not receive a Boot badge.

When installed, unprivileged zpool/zfs commands discover pool health, vdev layout (including mirrors/RAIDZ), dataset mounts and snapshots. Pool root used/available values enter totals once; child dataset availability is shared and never summed. Raw pool allocation is shown separately. Missing tools or permissions produce an explicit limitation. No pool is created or changed. LVM filesystem usage is supported; unallocated VG and thin-pool headroom are not included. ZFS/md/LVM behavior is fixture-tested; this deployment currently has none of those active pools.

storage_index.py supersedes the periodic one-shot scanner, retaining its category helpers. It stores per-directory metadata and classifications in telemetry/storage-index.sqlite3, with a persistent pending-directory queue, resumable reconciliation, inode-based hard-link deduplication and unchanged-entry cache reuse. It reads metadata only and skips nested mounts and symlinks. Atomic JSON snapshots retain the last completed categories while progress reports checked files, discovered directories remaining, and cache hits. The total file count is unknown during traversal, so the UI uses an indeterminate progress bar rather than a fabricated percentage.

The index timer wakes five minutes after a run; completed filesystems are reconciled after 30 minutes, and unfinished work resumes. Each run has an 1100-second budget, nice 19, idle I/O, 50% CPU quota and 512 MiB limit. These settings supersede the older scan schedule above. This is a persistent metadata cache with periodic reconciliation, not native filesystem journal or event tracking. Protected files remain unclassified. Current capacity samples continue every 15 seconds independently. Refresh reads cached data and does not traverse drives.

Range controls appear only on history-based Dashboard, Power, Thermal and Event log panels. Storage, Sensors, Services and Boot show current snapshots.

Validation: python3 -B -m unittest test_storage_breakdown test_storage_topology test_storage_index from dashboard/. Tests cover duplicate md/LVM paths, ZFS shared accounting and role propagation, mount boundaries, cache reuse, deletion, restart/resume, hard links and symlinks.

References: https://man7.org/linux/man-pages/man8/lsblk.8.html ; https://docs.kernel.org/admin-guide/md.html ; https://openzfs.github.io/openzfs-docs/man/v2.2/8/zpool-status.8.html ; https://openzfs.github.io/openzfs-docs/man/v2.2/7/zfsprops.7.html
