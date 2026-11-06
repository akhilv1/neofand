# neofand

Hardware findings for this machine — SMC key access, the fan-to-component map,
cover detection via `MSCO`, the power-zone tree, and the traps that produced
wrong answers along the way — live in
**[MACPRO71-FINDINGS.md](MACPRO71-FINDINGS.md)**.

Read that before probing hardware or changing fan behaviour. It is the document
that records what was measured rather than assumed.

User-facing configuration and behaviour are documented in [docs/fan-control.md](docs/fan-control.md).

The full multiplatform application is maintained in https://github.com/akhilv1/hardware-dashboard. Do not reintroduce its collectors or container packaging here; retain the native API and embedded diagnostic console.

## Build

```
cargo build --release                      # fan daemon only, unchanged behaviour
cargo build --release --features web       # + telemetry store + dashboard
```

`logging` and `web` are optional features and both default OFF. `web` implies
`logging`, because a dashboard with no history has nothing to show.

## The one architectural constraint everything else follows from

**`key_at_index` is a single shared index register.** Selecting a key and reading
its data are two separate operations, so two readers interleave and corrupt each
other. This has consequences that are not obvious from the code:

- Only the daemon's **control thread** may read raw SMC keys. Not the web thread,
  not the telemetry writer, not an external script.
- Running `tools/dump-smc-keys.sh` while the daemon is live disturbs both.
- This is *why* the telemetry logger lives inside this daemon rather than being a
  separate collector: no other process can reach the `P*` power space while
  neofand runs.
- `status.rs` publishes a file rather than serving a socket for the same reason.

Plain `fan*`/`temp*` sysfs attribute reads are **not** affected and are safe
concurrently.

## Threading rules

| thread | may touch | must never |
|---|---|---|
| control loop | SMC, fan sysfs, NVML | block — it holds `fan*_manual` |
| `neofand-logdb` | SQLite (write) | SMC |
| `neofand-web` | SQLite (read-only) | SMC, the control loop |

The control loop `try_send`s samples over a bounded channel; a full queue drops
the sample and counts it. **Losing a row always beats delaying a fan decision.**

`FanController::commanded_speed` is a state machine that steps **once per control
tick**. Telemetry runs at 10 s and control at 1 s — if telemetry called it too,
the slew limiter would step twice per cycle and silently double the ramp rate.
Telemetry reads `last_commanded_rpm()` instead. `calc_speed` stays pure so the
raw curve target can be published next to the damped command.

## SMC decoding traps

- **`P*` keys are not uniformly `flt`.** The `PZ*` zone gauges are, but
  `PC0P`/`PMLt`/`Pf*` are `spc3` and `PM0f` is `sp5a`. Decode by the declared
  type; `spNM` = signed with `M` (hex) fractional bits, so the family is generic.
- **`flt` is little-endian; every integer and fixed-point type is big-endian.**
  That asymmetry is in the hardware.
- **Types are space-padded to four characters** (`"flt "`). Trim before comparing
  — this silently broke a decoder once.
- **~111 channels report sentinels, not measurements**: `-127 C`, `-38 C`,
  `-26164 A`. These are unpopulated sensors. `telemetry::plausible()` drops them;
  do not "fix" it by widening the windows.

## Fan control: the failure mode to know about

The GPU ramp is auto-derived as **54–79 °C**, and the A6000s operate at
**43–61 °C** — they sit *on* the threshold. Combined with the `logarithmic`
curve (`raw.cbrt()`, which is near-vertical at the bottom of the ramp), a 1 °C
wobble swung the commanded speed by ~600 RPM, reversing direction on most
samples.

Damping is layered, and the order is deliberate:

1. **Smooth the target** (`SMOOTH_ALPHA`, ~12 s time constant) — removes the
   oscillation at its source.
2. **Deadband** (`HYSTERESIS_RPM`) — ignores residual chatter.
3. **Slew limit** (`SLEW_UP_RPM` / `SLEW_DOWN_RPM`) — bounds the rate.

Rising is faster than falling on purpose: being slow to speed up risks hardware,
being slow to slow down only costs noise. A rise over `BYPASS_RISE_RPM` skips
smoothing entirely, because junction temperature has been seen climbing 9 °C in
4 s and a 12 s filter must not delay that. **Safety floors bypass damping
outright** — with the cover off there is no chassis static pressure.

Lesson learned the hard way: output damping alone was measured as *insufficient*
here. Rate-limiting a control signal averaging 672 RPM of movement per sample
only slows the chase. If the fan is still jumpy, look at the **target**, not the
limiter — and consider whether `curve=linear` and a ramp that does not start at
the operating temperature would fix it at source.

## Web console

Control endpoints authenticate by **Tailscale identity** (`tsauth.rs`), so there
is no shared secret. This makes the bind address load-bearing rather than
cosmetic: address-derived identity is only sound if the packet had to arrive
through the tunnel, so `web.rs` **refuses to enable control on a wildcard bind**.
Tailnet membership is not authorisation — `control_users` names who may act, and
an empty allowlist authorises nobody.

Boot selection sets **`BootNext` only** — one-shot and firmware-cleared. Never
`BootOrder`: a wrong persistent choice leaves a machine that no longer boots the
OS you reach it over Tailscale with.

`web/dashboard.html` is deliberately **not** in `src/` and deliberately **not** a
submodule: `include_str!` resolves at compile time, so a submodule would make a
plain `git clone` produce a tree that does not build. It is self-contained (no
CDN, no build step) because a console for a machine you are debugging must not
need the internet to render.

**CSS trap:** the UA stylesheet's `[hidden] { display: none }` is a single
attribute selector, so *any* class that sets `display` beats it. The tab panes
are `<div class="grid" hidden>`, and `.grid { display: grid }` silently won —
every tab rendered stacked at once while only the active one received data. The
global `[hidden] { display: none !important; }` must stay above the layout
classes.

## Testing UI changes

There is no browser here. `node --check` on the extracted `<script>` catches
parse errors, and **jsdom against the live daemon** catches the rest — that is
how the `[hidden]` bug and a `matchMedia` crash were found. Both were invisible
to reading the code and to testing the JSON API, which returned correct data the
whole time.

When installing jsdom: pin `jsdom@22` (later versions need a newer Node), and
install `fetch` via `beforeParse` — assigning it after construction is too late,
the page's first `refresh()` has already run and failed.
