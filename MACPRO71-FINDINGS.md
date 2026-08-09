# MacPro7,1 findings

Hardware facts about running a Mac Pro 7,1 under Linux, gathered while building
neofand. Most of it took a physical experiment to establish and **cannot be
re-derived from the code** — fan-to-component mapping needed a hand stalling each
fan, and cover detection needed a magnet on a Hall sensor.

Published because none of it is documented anywhere else that I could find. If
you are working on T2 thermal management, start here rather than repeating the
experiments.

Reference machine: `MacPro7,1` / `Mac-27AD2F918AE68F61`, Ubuntu 24.04 userspace
on kernel `7.0.0-28-generic`, 28-core Xeon, Vega II Duo (2x gfx906) plus a third
AMD card. Treat RPM figures and sensor indices as specific to that configuration;
the mechanisms are general.

Where a claim is unverified, it says so. See "Operational safety" before touching
fans on a machine you care about.

Goal: neofand as a **full T2 replacement** for thermal/system health under Linux,
not just a GPU fan curve. The T2 still owns the behaviours listed under "What the
T2 still does on its own".

## SMC access

The SMC lives inside the T2. Mainline `applesmc` fails to probe with `-EIO`.
This machine uses the **patched out-of-tree DKMS `applesmc`** from
[t2-ubuntu-repo](https://github.com/AdityaGarg8/t2-ubuntu-repo) (package
`applesmc-t2`). It taints the kernel (unsigned) — that is expected, not a fault.

It binds to the ACPI node, **not** a platform device:

```
/sys/bus/acpi/devices/APP0001:00        <- all attributes live here
ACPI path: \_SB_.PCI0.LPCB.SMC_
```

Probe line to look for: `applesmc: key=1363 fan=4 temp=254 index=239 acc=0 lux=0 kbd=0`

### Gotcha: the hwmon node is a decoy

`applesmc` also registers a bare hwmon device (`/sys/class/hwmon/hwmonN` with an
**empty** `name` and no sensor attributes). It looks like a failed probe. It is
not — the real attributes are all on `APP0001:00`. Do not conclude applesmc is
broken from the empty hwmon dir.

### Raw key interface

1363 SMC keys are reachable by index. All reads/writes need root:

```
key_count                 -> 1363
key_at_index              <- write the index first (shared register!)
key_at_index_name         -> e.g. "F0Tg"
key_at_index_type         -> flt / ui8 / flag / hex_ / sp78 / fp88 / ch8* ...
key_at_index_data_length
key_at_index_data         -> raw bytes
```

`key_at_index` is a **single shared index register**. Two readers interleaving
will read each other's keys. Never run two key-reading processes at once — a
full dump and a poll loop will corrupt each other. Plain `fan*`/`temp*`
attribute reads are safe to run concurrently with key reads.

`flt` values are little-endian IEEE-754 as-laid-out: `0000fa43` -> `0x43fa0000`
-> `500.0`.

Tools in `tools/`:

- `dump-smc-keys.sh` — dumps all 1363 keys as `index<TAB>name<TAB>type<TAB>len<TAB>hex`. ~35 s.
- `cmp-smc-dumps.awk` — given dumps named `*open*` and `*closed*`, prints keys
  stable within each state but differing between them. **Always take ≥2 dumps per
  state**; many keys self-toggle and a single-shot diff produces false positives
  (see "LS\* trap").
- `fan-isolation-test.sh` — ramps each fan alone, proving independent control.

## Fan map (established by physical stall test, 2026-08-05)

The owner stalled fans by hand while RPMs were sampled at 1 Hz. This is the only
reliable mapping method found; SMC key names do not encode it.

| sysfs | SMC | min | `_max` | role | evidence |
|---|---|---|---|---|---|
| `fan1` | `F0` | 500 | 1200 | **PSU** | stalled -> 1200→653→43 RPM |
| `fan2` | `F1` | 500 | 2500 | GPU / MPX bay | by elimination |
| `fan3` | `F2` | 500 | 2500 | GPU / MPX bay | by elimination |
| `fan4` | `F3` | 500 | 2500 | **CPU** | stalled -> 519→348→155 RPM |

`fan2` vs `fan3` have **not** been individually distinguished — both are MPX/GPU
bay fans but which serves which bay is unknown. Stall one to settle it.

`fan1` (PSU) is also structurally the odd one out in the key space: `F0E0`/`F0E1`
are zero while `F1/F2/F3` carry `00000f02`/`00004f02`.

### `fan1_max` is a soft ceiling — it lies

`fan1_max` reports **1200**, but the PSU fan was observed at **1522 RPM** under
SMC control. neofand clamps to `_max`, so it is currently leaving real PSU
cooling headroom unused. Do not treat `_max` as physical.

### Suggested sensor pairing

Now that fans are mapped, each should curve off its own source rather than all
four following GPU junction:

- `fan1` (PSU) → `Tp0P` / `Tp1P` / `Tp2P` (SMC temp idx 232 / 235 / 238)
- `fan4` (CPU) → `coretemp` package, or SMC `TCXc` (idx 25)
- `fan2`,`fan3` (GPU) → amdgpu `junction`, per card (see below)

GPUs present: `card1` = `0000:0b:00.0` and `card2` = `0000:0e:00.0` (the two
Vega 20 dies of the Vega II Duo, both expose `edge`/`junction`/`mem`);
`card3` = `0000:17:00.0` exposes `edge` only.

## Cover / housing-open detection — DETECTABLE

**Confirmed:** the T2 ramps the fans on cover-open **entirely on its own, under
Linux, with no OS involvement.** Verified with neofand stopped and
`fan*_manual=0000`: idle RPMs went from ~680–740 to **1200 / 2250 / 2255 / 2260**
and held there for minutes. The ramp began ~8 s *before* a fan was stalled, so it
is the cover, not dead-fan compensation. So the macOS behaviour is a T2 function,
not a macOS daemon function.

The cover sensor is a **Hall sensor** — it can be tripped with a magnet while the
cover is physically off, which isolates it from every other variable. Doing that
produced a clean, **reversible** signal (verified open → tripped → open,
2026-08-05, neofand stopped and cover physically off the whole time):

| state | fan1 / fan2-4 | `F0Tg` | `F1Tg` | `LSOF` | `LS0P` |
|---|---|---|---|---|---|
| cover open | 1200 / ~2250 | 1200.0 | 2250.0 | **toggling** `00`/`01` | **toggling** `0000`/`439a` |
| sensor tripped ("closed") | ~690 / ~1224 | 691.0 | 1224.0 | steady `00` | steady **`5799`** |

Transition is a smooth ~15 s ramp, not a step.

### THE ANSWER: `MSCO` is the cover flag

```
MSCO  (flag, 1 byte)   01 = cover OPEN,  00 = cover CLOSED
MSCo  (flag, 1 byte)   duplicate of MSCO, same values
```

Read it by key index (2026-08-05 dump: `MSCO` = 759, `MSCo` = 761, `GTHR` = 638
— **resolve by name, indices are not stable across driver versions**). It is a
plain flag of type `flag`/1 byte, independent
of fan mode, so **neofand can read it while holding `fan*_manual=1`.** This is
the detector to use.

Verified: stable and identical across two independent magnet-held dumps, stable
across two independent cover-open dumps, and differing between the two states,
with the cover physically off and neofand stopped in *both* states — so the only
variable was the Hall sensor.

### The cover-open response cluster (all confirmed, same conditions)

The T2 does considerably more than spin fans up. Every one of these flips with
the cover:

| key | open | closed | meaning |
|---|---|---|---|
| `MSCO`, `MSCo` | `01` | `00` | **cover open** |
| `GTHR` | `07` | `00` | **GPU throttle active** |
| `GT0R`,`GT1R`,`GT2R` | `80` | `00` | per-GPU throttle reason |
| `GT0A`,`GT1A`,`GT2A` | `0400` | `0000` | per-GPU throttle amount |
| `LSCS` | `01` | `00` | LED status (amber fault) |
| `MSPA` | `0400` | `0000` | (unidentified) |
| `SPHR` | `00000080` | `00000000` | (unidentified) |
| `SPHT` | `0100` | `0000` | (unidentified) |
| `PMLt` | `004a` | `0053` | power limit |
| `F0Tg` | 1200.0 | ~691 | PSU fan target |
| `F1Tg`–`F3Tg` | 2250.0 | ~1224 | other fan targets |

**`GTHR`/`GT*R`/`GT*A` matter for benchmarking:** with the cover off the T2
throttles all three GPUs. Any benchmark run with the cover off is not comparable
to one run closed. Check `GTHR == 00` before trusting a result.

**Quantified 2026-08-05 — it costs 20x.** Same boot, same model, same binary,
cover the only variable (llama.cpp, qwen3.6-35B-A3B Q8_0, `-ngl 99`, layer split;
weights confirmed VRAM-resident at 17.7 + 16.7 GiB in both):

| cover | sclk under load | power | Tj | pp512 t/s | tg128 t/s |
|---|---|---|---|---|---|
| closed | 1730 MHz (DPM 8) | 89 W | 61 C | **987.6 ± 5.7** | 48.8 ± 3.7 |
| open (`GTHR=07`) | 1000 MHz (DPM 1) | 39 W / 24 W | 48 C | **48.8 ± 0.3** | 6.4 ± 0.0 |

Two things this settles:

- **`mclk` is not throttled** — normal 1000 MHz (HBM2 full speed, `DPM disabled`
  by design), so memory bandwidth is not the mechanism.
- **Junction is 48 C**, so it is not thermal.

### The mechanism is issue-rate gating, not just DVFS

**Controlled experiment, 2026-08-05.** Same binary, same boot, cover the only
variable. A pure-FMA kernel with no memory traffic, verified clean (VGPRs 18,
ScratchSize 0, no spills, 8 waves/SIMD max occupancy). The kernel launches in
~52 ms bursts with a sync between, which never triggers a DPM ramp — so the GPU
sat at **DPM level 1 in both states**, holding clock and voltage constant for
free. That makes this an unusually clean isolation:

| cover | actual sclk (`freq1_input`) | vddgfx | mclk / fclk / socclk | power | achieved |
|---|---|---|---|---|---|
| open (`GTHR=07`) | 999 MHz | 731 mV | 1000 / 1166 / 971 | 38 W | **0.13 TFLOP/s** |
| closed (`GTHR=00`) | 999 MHz | 731 mV | 1000 / 1166 / 971 | 63 W | **2.65 TFLOP/s** |

**20.4x more delivered FLOPs with not one clock or voltage variable moved.** The
throttle holds off wave issue; it does not scale frequency. Power rising 38 -> 63 W
at constant frequency confirms it: more instructions issuing means more dynamic
power. Cover-open sits ~15 W above the 23 W idle — far above the ~0.5 W of
*power*-gated CUs, far below what executing CUs draw. The CUs stay clocked and
powered while issue is gated.

Reference points: 2.65 TFLOP/s is 32% of the 8.18 TFLOP/s peak implied by
64 CU x 64 lanes x 2 flop x 0.999 GHz — a sane figure for an FMA burn with launch
overhead, which validates the kernel and confirms the cover-open 1.6% was the
throttle rather than a measurement flaw.

**Consequence for tooling: neither `pp_dpm_sclk` nor `freq1_input` can detect
this throttle.** Both read identical in the two states above. Achieved
throughput, delivered FLOP/s, or `GTHR` itself are the only reliable detectors —
which is why `bench.sh`'s DPM-level warning is a backstop, not a test.

### Clearing `GTHR` does NOT restore the boost ceiling (cover-open damage persists)

Refitting the cover clears `GTHR` — neofand logs `SMC GPU throttling cleared`,
and the issue-gating does lift. **But the GPUs stay pinned at DPM level 1 /
1000 MHz afterwards, and the driver cannot raise them:**

- `power_dpm_force_performance_level=high` -> still DPM 1, 999 MHz.
- `power_dpm_force_performance_level=manual` + `echo 8 > pp_dpm_sclk` -> still DPM 1.
- `pp_dpm_sclk` still *advertises* all nine levels up to 1730 MHz; `amdgpu_pm_info`
  reports `PSTATE_SCLK 999 MHz`. Power cap is untouched at 200 W and temps are
  cold (Tj 40 C), so nothing legitimate is holding it down.

The clamp is enforced SMU-side, below the driver. Measured on the same boot,
llama.cpp qwen3.6-35B Q8_0 pp512:

| state | pp512 | DPM | issue-gating |
|---|---|---|---|
| cover open | 48.84 | 1 (1000 MHz) | on |
| cover refitted, `GTHR` cleared | **614.64** | 1 (1000 MHz) | off |
| same boot, cover never opened | 987.61 | 8 (1730 MHz) | off |

So the two effects separate cleanly: issue-gating is worth **12.6x** and the DPM
ceiling **1.61x** (against the 1.73x the clock ratio predicts), for the 20.2x total.

**Operational consequence: opening the cover costs ~1.6x for the rest of the
boot, even after you close it.** A trustworthy benchmark requires a boot in
which the cover was never opened. This is very likely what really happened on
2026-08-04 — the 20x recovery from `qwen-q8-baseline` to
`qwen-q8-postreboot-default` spans *both* a cover close and a reboot, and it now
looks like both were needed.

**Untested:** that a reboot is what clears it. Strongly implied by the 08-04
sequence but not deliberately verified. Worth confirming, along with whether a
cheaper reset (amdgpu module reload, suspend/resume) also works.

Signature to recognise on this model: **~48.8 pp512 / ~6.4 tg128**. That
signature retroactively invalidated the 2026-08-04 `qwen-q8-baseline` run
(48.90 / 6.51 — matches to 0.1%), which had been recorded as a legitimate
prereboot baseline and used to credit a reboot with a 20x gain that was actually
just the case being closed. See `bench-results/IF-BRIDGE-REGRESSION-2026-08-05.md`.

`F*Te` tracks `F*Tg`, so the cover override arrives via the `Te` control slot.

### `LS*` = LED Status (a trap — cost an hour)

`LSOF`, `LS0P`, `LS1P` **toggle in lockstep every 1–2 s** while the cover is open.
A single-shot diff catches the strobe at a random phase and "finds" them changing.
Cause (identified by the owner): the front status LED **blinks amber while the
cover is off**. So `LS` is LED Status and the toggle is the blink.

They *are* usable as a secondary signal — `LS0P == 0x5799` steady means closed,
alternating `0000`/`439a` means open — but prefer `MSCO`.

### Two methodology traps that produced wrong answers here

1. **Volatile keys.** Always take **≥2 dumps per state** and require
   stability-within-state (`tools/cmp-smc-dumps.awk`). This is what exposed
   `LS*` as noise.
2. **Verify the baseline state, don't assume it.** The first "cover-closed"
   baseline was actually taken with the cover **open**, which made `MSCO` look
   unchanged and got it wrongly rejected. The tell was misread: fans reading
   740/682/673/674 three seconds after handing control to the SMC looked like a
   settled idle, but the SMC ramp takes ~15 s, so they were still *rising* from
   neofand's 500 RPM floor toward 1200/2250. **Never infer cover state from a
   fan sample taken <20 s after a mode change** — read `MSCO`.

### Still unconfirmed

No dump has been taken with the cover **physically refitted** — the magnet is
assumed to faithfully emulate it (fans, LED and all 13 keys behave identically to
a real close, so this is likely). Confirm opportunistically: with the cover on,
`MSCO` should read `00`.

### Confounded candidates

A closed-vs-open comparison was run with neofand *running* while closed and
*stopped* while open, so these differences are explained by the mode change and
prove nothing about the cover: `F0Md`–`F3Md` (manual flag), `F0Tg`–`F3Tg` (target
RPM), `F0AS`, `F3E0`/`F3E1`, `TG0L`/`TG1L`/`TG3L` (`0004`→`0010`),
`ADC6`, `ADC9`, `CLWK`, `IDTJ`, `PfM0`.

Redo with neofand held in one state throughout.

### Correct experiment (next step)

The cover sensor is a **Hall sensor** — the owner can trip it with a magnet with
the cover physically off. That isolates the sensor from every other variable
(airflow, temps, neofand mode). Procedure:

1. Cover off, neofand stopped, throughout.
2. 2 dumps with no magnet.
3. Apply magnet, confirm the trip via **fan RPMs dropping** (fast signal), then
   2 dumps with it held.
4. `awk -f tools/cmp-smc-dumps.awk *open* *closed*`.

One magnet attempt on 2026-08-05 produced **no** change in 45 s (fans stayed
ramped, `LSOF` kept toggling) — either not applied or not on the sensor.

### Practical fallback

Even without a key, cover-open is inferable: with the SMC in control it pins fans
to ~1200/2250/2250/2250 at idle. But neofand taking manual control removes that
observable, which leads to the open safety question below.

## What the T2 still does on its own

Relevant to the "replace the T2" goal — these do not need reimplementing, but
neofand must not *defeat* them:

- Cover-open fan ramp (confirmed above).
- Its own idle floor of ~680–740 RPM, notably **higher** than neofand's 500 RPM
  floor. neofand currently runs the machine cooler-biased-quieter than stock.
- Amber status-LED fault blink.

## Operational safety

- `systemctl stop neofand` writes `fan*_manual=0` and hands the fans back to the
  SMC. That is the failure-safe state; a crash degrades to stock behaviour.
- **CONFIRMED HAZARD: neofand defeats the T2's cover-open protection.**
  Measured 2026-08-05 with the cover physically off (`MSCO=01`): starting neofand
  drove the fans from **1195 / 2244 / 2243 / 2247 down to 500 / 442 / 455 / 452
  within 20 s**, and they stayed there. `fan*_manual=1` wins outright — the T2
  does *not* override it. So running neofand with the cover off silently removes
  the airflow the PSU, CPU and MPX modules depend on (chassis static pressure,
  not local airflow).

  **Fixed** — `cover_mode=floor` (the default) keeps manual control but clamps
  each fan to its `cover_open_rpm`. Verified with the cover off: `manual=1111`,
  targets 1350/2250/2250/2250, actual ~1328/2255/2252/2247.

  Why a floor rather than releasing `fan*_manual` back to the T2 (which was the
  first implementation): the T2's cover-off ramp was only ever observed as a
  **flat** 1200/2250 regardless of temperature, i.e. a safety floor, not a curve.
  Handing control back would therefore *lose* thermal responsiveness under load.
  Enforcing the same floor ourselves keeps the guarantee and lets the curve ramp
  above it. `cover_mode=release` still exists for exact stock behaviour.

  Failure path is unchanged and still safe: on clean exit the daemon writes
  `fan*_manual=0`, and the unit's `ExecStopPost` does the same after a SIGKILL,
  so a dead daemon always ends up back under T2 control.

  Note `GTHR` stays `07` regardless of fan mode: the GPU throttle is applied
  T2-side and neofand cannot affect it.
- Restore after any experiment:
  ```bash
  sudo systemctl start neofand
  systemctl is-active neofand
  # expect manual=1111
  for i in 1 2 3 4; do cat /sys/bus/acpi/devices/APP0001:00/fan${i}_manual; done
  ```

## Config

`/etc/neofand.conf` (not the repo's example). Already has per-fan `[Fan1]`..`[Fan4]`
sections, so per-fan plumbing exists — but the current build applies one
GPU-driven curve to all four. `/etc/neofand.conf.old` holds the older per-fan
`gpu_low_temp`/`cpu_low_temp`/`speed_curve` schema, which is the shape to grow
back into now that fans are mapped.

Independent per-fan control is **verified working**: driving one fan to 80 % of
max left the other three at 450–500 RPM.

## Implementation status (2026-08-05)

Done and verified on hardware:

1. **Cover safety interlock** — `src/smc.rs` resolves SMC key names to indices at
   startup; the control loop polls `MSCO`. `cover_mode=floor` (default) keeps
   control and clamps each fan to `cover_open_rpm`; `release` hands
   `fan*_manual` back to the T2; `off` ignores the cover (don't).
   Both working modes verified with the cover physically removed, against a
   baseline where the pre-fix build collapsed the fans to 500/442/455/452.
   Legacy `cover_interlock=false` is still honoured as an alias for `off`.
2. **Per-fan sensor sources** — `source=` per fan (`gpu`/`cpu`/`psu`/`gpucpu`).
   `/etc/neofand.conf` now maps psu/gpu/gpu/cpu; template kept in the repo as
   `neofand.conf.macpro71`.
3. **PSU curve from total system power** — SMC `PSTR` as feed-forward, `Tp*`
   sensors as backstop, higher demand wins. Validated: a 35 W step appeared in
   `PSTR` within 1–2 s while `Tp*` had not moved after 20 s. Nine `Tp*` channels
   are discovered by label prefix.
4. **`fan1` ceiling override** — `max_rpm=1500` for the PSU fan, versus the
   bogus 1200 the hardware reports.
5. **`GTHR` warning** — logged on transition, so a throttled benchmark is
   obvious in the journal.
6. **SIGKILL backstop** — `ExecStopPost` in the unit clears `fan*_manual` if the
   daemon is killed outright and never runs its own cleanup.

Still open:

- **Distinguish `fan2` vs `fan3`.** Stall one to learn which MPX bay it serves,
  so each GPU fan can follow its own die instead of both following the hottest.
- **Reconsider the 500 RPM floor.** The T2's own idle floor is ~680–740, so
  neofand runs quieter than stock at idle. Confirm that is intended.
- **Confirm the T2's cover-off ramp really is flat.** The `floor` design assumes
  it is a fixed 1200/2250 that ignores temperature, which is the whole argument
  for preferring `floor` over `release`. That was only ever observed at idle. To
  check: set `cover_mode=release`, run a GPU load with the cover off, and see
  whether the T2 pushes past 2250. If it does ramp with temperature, `release`
  becomes competitive again and the floors should be re-derived from its curve.
- ~~**Verify `GTHR` actually caps clocks.**~~ **Done 2026-08-05: it does, and it
  costs 20x** (987.6 -> 48.8 pp512). Numbers and method in "The cover-open
  response cluster" above. Note `pp_dpm_sclk` understates the throttle.
- **Tune the PSU thresholds against a real load.** 250→900 W and 45→70 °C are
  first estimates from a 195 W idle and a 226 W CPU-only step; nothing has yet
  driven the PSU hard.
- **Other T2 fault states** — pull a fan or overheat a zone and diff with
  `tools/`, same method that found `MSCO`.

## SMC temperature sensor taxonomy

239 of 254 temp slots are labelled; ~150 read non-zero. Prefixes:

| prefix | meaning |
|---|---|
| `TA*` | ambient / intake air |
| `TC*`, `TCS*`, `TCXc` | CPU (proximity, per-cluster, package) |
| `TG*` | GPU / MPX modules (`TG5p` ran hottest, 54.8 °C idle) |
| `TM*`, `Tm*` | memory / DIMM slots |
| `Tp*` | **PSU** |
| `TH*` | storage |
| `TPCD` | PCH die (~50 °C) |
| `TB*`, `TBtd` | bridge / T2 |
| `TS*` | mixed board sensors (`TS1e` ~58 °C) |
| `Te*` | PCIe / expansion |

Read as `temp<N>_label` / `temp<N>_input` (milli-°C) on `APP0001:00`. Note the
index is **not** stable across kernel/driver versions — always resolve by label.

## Fan control attributes

Per fan `N` in 1..4 on `APP0001:00`:

```
fan N _input    RPM (read)
fan N _min      500 on all four
fan N _max      soft ceiling; fan1's is wrong (see above)
fan N _manual   1 = OS control, 0 = SMC control
fan N _output   requested RPM (write; requires _manual=1)
fan N _safe     present but empty
fan N _label    present but EMPTY on this machine — no free mapping info
```

Corresponding SMC keys per fan `X` in 0..3: `FXAc` actual RPM, `FXTg` target,
`FXMd` manual flag, `FXMn`/`FXMx` min/max, `FXTa`..`FXTe` appear to be requested
targets from five independent control sources (the max seems to win into `Tg`),
`FNum` = 4 fan count.

## SMC power zones — the GPU brake explained (2026-08-09)

The `P*` key space is a **power-zone tree**, and it is what actually drives the
`HW-POWER-BRAKE` seen on the A6000s. Each zone has a gauge, a filtered average,
a ceiling and a trip flag: `PZ<n>G` / `PZ<n>F` / `PZ<n>E` / `PZ<n>T`.

| zone | alias | ceiling | idle | 3-card burn | what |
|---|---|---|---|---|---|
| `PZ1` | `PC0H` | 450 W | 85.0 | 115.5 | CPU |
| `PZ3` | `PG0R` | **375 W** | 45.7 | **387.1 — over, `PZ3T`=100** | GPU rail 0 |
| `PZ4` | `PG2R` | 375 W | 19.2 | 352.9 | GPU rail 1 |
| `PZ5` | `PD2R` | 300 W | 25.8 | 179.3 | unidentified |
| `PZ0`/`PZ6` | — | 1228 W | 204.7 | 1052 | total 12 V DC out |
| `PZ7` | — | 1562 W | 234.3 | 1083 | 12 V DC, ~30 W above `PZ0` |

`PSTR` = `PDTR` = total system power: 207.8 W idle, 1066 W under a 3-card burn.

**Sensing is platform-side, not on the card.** Apple meters each rail; the
signal reaches a stock PC card through the PCIe CEM `PWRBRK#` pin (B30), a
platform→card slot signal. That is why an A6000 with no Apple awareness responds
at all, and why *slot position* — not the card — decides which one brakes.

### `GTHR` is announced, not enforced

Measured solo vs trio, 90 s each, identical TF32 SGEMM:

| card | solo | in trio | delta |
|---|---|---|---|
| GPU0 `0D` | 5680 gemms | 5680 | **0.00%** |
| GPU1 `0F` | 5736 | 5672 | −1.12% |
| GPU2 `FC` | 5744 | 5648 | −1.67% |

`GTHR` was asserted **47/84 samples in the trio run and 0/84 in every solo run**,
and aggregate throughput fell 0.93% (209.7 → 207.7 TF). So `GTHR`/`PZ*T` mean
"a zone is over budget", not "the cards are being held back". **Do not infer a
performance loss from `GTHR`** — count GEMMs.

### Two decoding traps

- **`P*` keys are not uniformly `flt`.** `PC0P`, `PMLt` and `Pf*` are `spc3`;
  `PM0f` is `sp5a`. The type code encodes its own layout (`spNM` = signed, `M`
  hex fractional bits), so decode generically. Reading `spc3` as `flt` is
  garbage. Also note `flt` is little-endian while every integer and fixed-point
  type is big-endian.
- **`PZ0/PZ7` is not PSU efficiency.** It looks like 87% idle rising to 97% under
  load, which is already above what any AC→DC supply achieves. Both keys divide
  to **11.975 V** against the SMC current keys (`PSTR/IDTR`, `PZ7G/IDTA`), so
  both are 12 V DC-side, and the gap is a near-constant ~30 W rather than a
  load-proportional loss. `VAIR` gives mains voltage (121 V) but there is no AC
  current key, so true efficiency needs an external wall meter against `PSTR`.

### Headroom for a 4th A6000

The two GPU zones total 750 W of budget; three cards already draw 740 W. A 4th
needs ~247 W and ~10 W is free. It fits only with all four capped to ~185–190 W
— worth it for VRAM (192 GB), roughly a wash for compute. Note an A6000 in an
MPX bay **cannot draw from the MPX connector**; it still feeds from the aux
cables, so an MPX slot buys a slot, not a power feed.

## Telemetry and the web UI (2026-08-09)

`[Logging]` and `[Web]` are optional features (`cargo build --features web`).
386 channels at 10 s with a deadband; **discrete channels — `GTHR`, `MSCO`,
`PZ*T` — use a zero deadband so a trip shorter than the sample interval is still
recorded.**

The logger lives in this daemon rather than a separate collector for the reason
in "SMC access": `key_at_index` is one shared index register, so an external
process cannot read raw keys while neofand runs. That is precisely why the `P*`
zones were unreachable until now.

Control endpoints are authorised by **Tailscale identity** (`tsauth.rs`), which
means no shared secret. This makes the bind address load-bearing: address-derived
identity is only sound if the packet came through the tunnel, so `web.rs`
disables control on a wildcard bind. Boot selection sets `BootNext` only — a
one-shot the firmware clears — never `BootOrder`.
