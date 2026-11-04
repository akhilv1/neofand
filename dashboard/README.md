# NeoCharge hardware dashboard

Live Sensors view: https://neo-mac-pro.west-scoville.ts.net/hardware/#sensors

This directory is the source of the separate Portainer `neofand` dashboard stack. It serves a NeoCharge-styled monitoring frontend and proxies read-only requests to the host daemon. The daemon's original embedded console remains in `web/dashboard.html`.

## Files

- `index.html`: dashboard, sensor browser, charts, and diagram markup.
- `assets/sensor-diagram.js`: updates the callouts from the existing `/api/latest` result and switches chassis views.
- `assets/sensor-diagram.css`: engineering diagram layout and responsive reading cards.
- `assets/macpro-gpu-approved.png`: approved illustration with three A6000 cards.
- `assets/macpro-fan-side.png`: reverse chassis view supplied as a reference.
- `assets/rtx-a6000-card.png`: approved transparent card artwork for future composition edits.
- `Caddyfile` and `compose.yaml`: read-only frontend deployment.

The HTML requests the proxy at `/hardware/api/*`. The main router removes `/hardware` before forwarding to this container on loopback port 4791. Caddy accepts only GET requests to `/api/*` and forwards them to the daemon at its Tailscale address on port 9977. The existing router identity gate controls access to the page.

## Callout sources

Each A6000 displays its `gpuN_temp_c`, `gpuN_fan_pct`, and `gpuN_power_w` channels. CPU package temperature comes from `hwmon/coretemp_package_id_0`. PSU temperature is the maximum of `smc_temp/Tp0P`, `Tp1P`, and `Tp2P`.

The fan map follows the measured roles in `MACPRO71-FINDINGS.md`: fan 1 is the PSU, fan 4 is CPU cooling, and fans 2 and 3 serve the GPU bay. The latter two have not been distinguished individually, so their annotation shows a shared cooling zone. All image anchor positions are approximate.

The GPU callouts are arranged top to bottom as GPU 2, GPU 0, GPU 1 based on PCI ancestry and firmware slot labels observed on the server: `fc:00.0` / Slot 5, `0d:00.0` / Slot 4, and `0f:00.0` / Slot 3. Recheck this map after moving cards or changing GPU enumeration. Raster placement is illustrative.

The callouts follow the existing 30 second refresh toggle. A failed refresh marks readings unavailable instead of presenting the previous values as fresh. Missing channels display a dash.

## Deployment

The deployed source directory is `/home/neo/services/neofand-dashboard`. Portainer owns the `neofand` stack (endpoint 1, stack ID 9). Copy this directory's page, assets, and Caddyfile there, then apply `compose.yaml` through the stack editor. Its volumes mount the page and assets read-only; the host systemd service continues collecting telemetry and controlling fans.

The page is static and has no build step. Check JavaScript syntax with `node --check assets/sensor-diagram.js`, then verify the live Sensors view, changing values, both chassis views, and the reading-card layout on narrow screens.
