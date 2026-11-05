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
