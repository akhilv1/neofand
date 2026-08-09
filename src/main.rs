#![warn(rust_2018_idioms)]
#![warn(clippy::pedantic)]
#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::similar_names,
    clippy::module_name_repetitions
)]

//! neofand — compute-focused fan daemon, forked from T2FanRD (GnomedDev).
//!
//! Built for machines where the GPUs, not the CPU, are what get hot: the fan
//! curve is driven by GPU temperature, with the CPU tracked as a floor.
//!
//! See README.md for configuration and the profile definitions.

use std::{
    io::{ErrorKind, Read, Seek},
    path::PathBuf,
    process::ExitCode,
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};

use arraydeque::ArrayDeque;
use fan_controller::{FanController, Readings};
use nonempty::NonEmpty as NonEmptyVec;
use signal_hook::consts::{SIGINT, SIGTERM};

use config::{load_config, CoverMode, FanSource, GpuFanConfig, MAX_FALL_SAMPLES};
use error::{Error, Result};

mod config;
mod error;
mod fan_controller;
mod gpu;
mod nvidia;
mod smc;
mod status;

#[cfg(not(target_os = "linux"))]
compile_error!("This tool is only developed for Linux systems.");

#[cfg(debug_assertions)]
const PID_FILE: &str = "neofand.pid";
#[cfg(not(debug_assertions))]
const PID_FILE: &str = "/run/neofand.pid";

/// Candidate locations for the SMC fan attributes, tried in order.
///
/// Upstream globs `/sys/devices/pci*/*/*/*/APP0001:00/`, which never matches on
/// MacPro7,1 — the node lives under `LNXSYSTM:00/.../device:1f/` and the `pci`
/// physical-node path carries no fan attributes at all.
///
/// Two traps to avoid when fixing that. A `**` glob over `/sys/devices` pins a
/// core indefinitely, because /sys is full of symlink cycles. And widening the
/// first component to `*` makes the same node reachable 67 different ways, which
/// silently breaks the positional zip of Fan1..FanN config sections against
/// discovered paths. So: anchor on the ACPI bus symlink, which is a single
/// stable path, and canonicalise + dedupe regardless.
const FAN_GLOBS: &[&str] = &[
    "/sys/bus/acpi/devices/APP0001:00/fan*_input",
    "/sys/class/hwmon/hwmon*/device/fan*_input",
    "/sys/devices/platform/applesmc.768/fan*_input",
];

const CPU_TEMP_GLOB: &str = "/sys/devices/platform/coretemp.0/hwmon/hwmon*/temp1_input";

type TempHistory = ArrayDeque<u8, MAX_FALL_SAMPLES, arraydeque::Wrapping>;

fn get_current_euid() -> libc::uid_t {
    // SAFETY: FFI call with no preconditions
    unsafe { libc::geteuid() }
}

fn find_fan_paths() -> Result<NonEmptyVec<PathBuf>> {
    for pattern in FAN_GLOBS {
        let mut fans = glob::glob(pattern)?
            .filter_map(std::result::Result::ok)
            // Resolve through the symlinked bus/class paths so duplicates
            // collapse to one entry per physical fan.
            .filter_map(|path| std::fs::canonicalize(path).ok())
            .filter_map(|mut path| {
                let file_name = path.file_name()?.to_str()?;
                let fan_name = file_name.strip_suffix("_input")?;
                let fan_name_owned = fan_name.to_owned();
                path.set_file_name(fan_name_owned);
                Some(path)
            })
            .collect::<Vec<_>>();

        // Glob order is not guaranteed, and config sections Fan1..FanN are
        // zipped positionally against this list. Sort for a stable mapping,
        // then dedupe in case a pattern reached the same node twice.
        fans.sort();
        fans.dedup();

        if let Some(fans) = NonEmptyVec::from_vec(fans) {
            println!("neofand: {} fan(s) via {pattern}", fans.len());
            return Ok(fans);
        }
    }

    Err(Error::NoFan)
}

fn check_pid_file() -> Result<()> {
    match std::fs::read_to_string(PID_FILE) {
        Ok(pid) => {
            let mut proc_path = std::path::PathBuf::new();
            proc_path.push("/proc");
            proc_path.push(pid);

            if proc_path.exists() {
                return Err(Error::AlreadyRunning);
            }
        }
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => return Err(Error::PidRead(err)),
    }

    let current_pid = std::process::id().to_string();
    std::fs::write(PID_FILE, current_pid).map_err(Error::PidWrite)
}

fn read_temp_file(temp_file: &mut std::fs::File, temp_buf: &mut String) -> Result<u8> {
    temp_file
        .read_to_string(temp_buf)
        .map_err(Error::TempRead)?;

    temp_file.rewind().map_err(Error::TempSeek)?;

    let temp = temp_buf.trim_end().parse::<u32>().map_err(Error::TempParse);
    temp_buf.clear();
    temp.map(|t| (t / 1000) as u8)
}

/// A group of sensors read together, reduced to their maximum.
///
/// Holds both file-backed (hwmon/sysfs) and NVML-backed sources, because NVIDIA
/// exposes no hwmon node and must be read through its own library.
struct TempGroup {
    /// Description alongside each handle, so the status snapshot can report a
    /// per-sensor breakdown rather than only this group's reduced maximum.
    files: Vec<(String, std::fs::File)>,
    nvidia: Option<nvidia::NvidiaBackend>,
    history: TempHistory,
    fall_samples: usize,
}

impl TempGroup {
    /// The NVML backend, when this group has one.
    ///
    /// Exposed so the control loop can drive the NVIDIA cards' own fans, which
    /// needs per-device temperatures rather than this group's reduced maximum.
    fn nvidia(&self) -> Option<&nvidia::NvidiaBackend> {
        self.nvidia.as_ref()
    }

    /// Open a set of already-discovered sensor paths.
    fn from_paths<I>(label: &str, paths: I, fall_samples: usize, temp_buf: &mut String) -> Self
    where
        I: IntoIterator<Item = (PathBuf, String)>,
    {
        let mut files = Vec::new();
        for (path, desc) in paths {
            let Ok(mut file) = std::fs::File::open(&path) else {
                eprintln!("neofand: cannot open {}", path.display());
                continue;
            };

            // Only keep sensors that actually parse, so a bogus node cannot
            // take the daemon down mid-loop. Say so when one is dropped: a
            // silent skip here is indistinguishable from the sensor never being
            // discovered, which sends you looking at the config instead of the
            // hardware.
            if read_temp_file(&mut file, temp_buf).is_ok() {
                println!("neofand: {label} sensor {desc}");
                files.push((desc, file));
            } else {
                eprintln!(
                    "neofand: WARNING ignoring {label} sensor {desc} — {} did not parse as a \
                     temperature",
                    path.display()
                );
            }
        }

        Self {
            files,
            nvidia: None,
            history: TempHistory::new(),
            fall_samples,
        }
    }

    /// Attach an NVML backend to this group.
    fn with_nvidia(mut self, nvidia: Option<nvidia::NvidiaBackend>) -> Self {
        if let Some(backend) = &nvidia {
            for desc in backend.descriptions() {
                println!("neofand: gpu sensor {desc}");
            }
        }
        self.nvidia = nvidia;
        self
    }

    fn from_glob(label: &str, pattern: &str, fall_samples: usize, temp_buf: &mut String) -> Self {
        let paths = glob::glob(pattern)
            .into_iter()
            .flatten()
            .filter_map(std::result::Result::ok)
            .map(|p| {
                let desc = p.display().to_string();
                (p, desc)
            })
            .collect::<Vec<_>>();

        Self::from_paths(label, paths, fall_samples, temp_buf)
    }

    fn is_empty(&self) -> bool {
        self.files.is_empty() && self.nvidia.as_ref().map_or(0, nvidia::NvidiaBackend::len) == 0
    }

    /// Hottest sensor in the group. Empty groups read 0, which yields zero
    /// demand rather than an error.
    fn read_max(&mut self, temp_buf: &mut String) -> Result<u8> {
        let mut max = 0;
        for (_, file) in &mut self.files {
            let temp = read_temp_file(file, temp_buf)?;
            if temp > max {
                max = temp;
            }
        }

        if let Some(backend) = &self.nvidia {
            if let Some(temp) = backend.read_max() {
                let temp = temp.min(u32::from(u8::MAX)) as u8;
                if temp > max {
                    max = temp;
                }
            }
        }

        Ok(max)
    }

    /// Every individual sensor in this group, for the status snapshot.
    ///
    /// Distinct from [`read_max`]: that reduces to one number for the curve,
    /// which hides *which* of several sensors is the hot one. A failed read is
    /// skipped rather than propagated -- status must never break control.
    fn read_each(&mut self, temp_buf: &mut String) -> Vec<(String, u8)> {
        let mut out = Vec::new();
        for (desc, file) in &mut self.files {
            if let Ok(temp) = read_temp_file(file, temp_buf) {
                out.push((desc.clone(), temp));
            }
        }
        if let Some(backend) = &self.nvidia {
            for (index, temp) in backend.temps_each() {
                out.push((format!("nvml [{index}]"), temp));
            }
        }
        out
    }

    /// Rise immediately, fall on the trailing average.
    ///
    /// A symmetric average would lag a thermal spike by the whole window, and
    /// junction has been seen to climb ~9 C in 4s. Averaging only the downward
    /// direction keeps the response prompt while still preventing fan hunting.
    fn effective(&mut self, temp_buf: &mut String) -> Result<u8> {
        let now = self.read_max(temp_buf)?;
        self.history.push_back(now);

        let window = self.fall_samples.min(self.history.len());
        let sum: u32 = self
            .history
            .iter()
            .rev()
            .take(window)
            .map(|t| u32::from(*t))
            .sum();
        let mean = (sum / window as u32) as u8;

        Ok(if now >= mean { now } else { mean })
    }
}

/// Seconds since the epoch, or 0 if the clock is before it.
fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Discover GPU temperature channels, re-scanning until the set stops growing.
///
/// A GPU driver publishes its hwmon node at the end of its probe, which can land
/// after this daemon has already started — measured 9.1 s late for amdgpu on a
/// MacPro7,1. A single scan at startup therefore silently misses the card for the
/// lifetime of the process.
///
/// Cards cannot be swapped on a running machine, so this settles once and then
/// stops rather than polling forever. It returns as soon as two consecutive scans
/// agree, so the common case where everything is already bound costs one extra
/// second rather than the whole window.
fn settle_gpu_sensors(config: &config::GpuConfig) -> Result<Vec<gpu::Sensor>> {
    const AGREEING_SCANS: u32 = 2;

    let mut best = gpu::discover(config)?;
    if config.settle_secs == 0 {
        return Ok(best);
    }

    let deadline = Instant::now() + Duration::from_secs(config.settle_secs);
    let mut stable = 0;

    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_secs(1));
        let scan = gpu::discover(config)?;

        if scan.len() == best.len() {
            stable += 1;
            if stable >= AGREEING_SCANS {
                break;
            }
            continue;
        }

        // Only ever grow the set. A channel that disappears mid-settle is a
        // driver still rearranging itself, not a card being removed, so keeping
        // the larger set avoids latching onto a transient.
        if scan.len() > best.len() {
            for sensor in &scan {
                if !best.iter().any(|existing| existing.path == sensor.path) {
                    println!(
                        "neofand: gpu appeared {:.1}s after start: {}",
                        config.settle_secs as f64
                            - (deadline - Instant::now()).as_secs_f64(),
                        sensor.desc
                    );
                }
            }
            best = scan;
        }
        stable = 0;
    }

    Ok(best)
}

fn main() -> ExitCode {
    // `--status` is a plain reader of the published snapshot -- it never talks to
    // the daemon, which is the whole point: querying cannot disturb fan control.
    // It also needs no privileges, unlike the daemon itself.
    let mut args = std::env::args().skip(1);
    if let Some(flag) = args.next() {
        return match flag.as_str() {
            "--status" => {
                let path = args.next().unwrap_or_else(|| {
                    config::GeneralConfig::default().status_path
                });
                match status::print(std::path::Path::new(&path)) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(err) => {
                        eprintln!("neofand: cannot read {path}: {err}");
                        eprintln!("  is the daemon running, and is status_path set in [General]?");
                        ExitCode::FAILURE
                    }
                }
            }
            "--help" | "-h" => {
                println!("neofand -- per-component fan daemon");
                println!("  neofand                    run the daemon (needs root)");
                println!("  neofand --status [PATH]    print the latest sensor snapshot");
                ExitCode::SUCCESS
            }
            other => {
                eprintln!("neofand: unknown argument {other}; try --help");
                ExitCode::FAILURE
            }
        };
    }

    match real_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Error: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Whether a new sample warrants rewriting the fan targets.
///
/// Temperatures are integers so exact comparison is fine, but `PSTR` jitters by
/// a watt or two at a fixed load, which would otherwise cause a write every
/// single poll.
fn readings_changed(old: &Readings, new: &Readings) -> bool {
    old.gpu != new.gpu
        || old.cpu != new.cpu
        || old.psu != new.psu
        || (old.watts - new.watts).abs() >= 5.0
}

#[allow(clippy::too_many_arguments)]
fn start_temp_loop(
    mut temp_buffer: String,
    mut gpu_group: TempGroup,
    mut cpu_group: TempGroup,
    mut psu_group: TempGroup,
    smc: Option<&smc::Smc>,
    cover_mode: CoverMode,
    fans: &NonEmptyVec<FanController>,
    gpu_fans: GpuFanConfig,
    status_path: &str,
    profile_name: &str,
    config_file: &str,
    fall_samples: usize,
    poll: std::time::Duration,
) -> Result<()> {
    let started = now_unix();
    let cancellation_token = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(SIGINT, cancellation_token.clone()).map_err(Error::Signal)?;
    signal_hook::flag::register(SIGTERM, cancellation_token.clone()).map_err(Error::Signal)?;

    // Tracked here rather than read back from sysfs: `fan*_manual` is eventually
    // consistent, so a readback straight after a write still reports the old
    // value and would make this flip-flop.
    let mut holding_manual = false;
    let mut cover_open_last: Option<bool> = None;
    let mut throttled_last: Option<bool> = None;
    let mut last: Option<Readings> = None;

    while !cancellation_token.load(std::sync::atomic::Ordering::Relaxed) {
        // A missing SMC or missing key reads as "closed", so a machine without
        // cover detection behaves exactly as before rather than refusing to
        // control the fans.
        let cover_open = cover_mode != CoverMode::Off
            && smc
                .and_then(|smc| smc.flag(smc::KEY_COVER_OPEN))
                .unwrap_or(false);

        if cover_open && cover_mode == CoverMode::Release {
            if holding_manual || cover_open_last != Some(true) {
                for fan in fans {
                    fan.set_manual(false)?;
                }
                holding_manual = false;
                eprintln!(
                    "neofand: cover is OPEN — released the fans to the SMC (cover_mode=release)."
                );
            }

            cover_open_last = Some(true);
            // Drop history so the curve does not resume from a stale sample.
            last = None;
            std::thread::sleep(poll);
            continue;
        }

        if cover_open_last != Some(cover_open) {
            if cover_open {
                eprintln!(
                    "neofand: cover is OPEN — holding each fan at its cover-open floor; the \
                     curve still applies above it."
                );
            } else if cover_open_last.is_some() {
                println!("neofand: cover closed — resuming normal floors");
            }
            // Force a write on any transition rather than waiting for the
            // readings to happen to change.
            last = None;
        }
        cover_open_last = Some(cover_open);

        if !holding_manual {
            for fan in fans {
                fan.set_manual(true)?;
            }
            holding_manual = true;
        }

        // Surface the SMC's own GPU throttle state. Checked only with the cover
        // fitted, because cover-off sets it unconditionally and would just spam.
        // Any benchmark captured while this is set is not comparable to one
        // captured without it.
        let throttled = smc
            .and_then(|smc| smc.flag(smc::KEY_GPU_THROTTLE))
            .unwrap_or(false);
        if throttled_last != Some(throttled) {
            if throttled {
                eprintln!(
                    "neofand: WARNING the SMC is throttling the GPUs ({} is set)",
                    smc::KEY_GPU_THROTTLE
                );
            } else if throttled_last.is_some() {
                println!("neofand: SMC GPU throttling cleared");
            }
            throttled_last = Some(throttled);
        }

        let readings = Readings {
            gpu: gpu_group.effective(&mut temp_buffer)?,
            cpu: cpu_group.effective(&mut temp_buffer)?,
            psu: psu_group.effective(&mut temp_buffer)?,
            watts: smc
                .and_then(|smc| smc.read_f32(smc::KEY_SYSTEM_POWER))
                .unwrap_or(0.0),
        };

        // `map_or(true, ..)` rather than `is_none_or`: Ubuntu ships cargo 1.75
        // and the README promises that is enough to build.
        if last
            .as_ref()
            .map_or(true, |old| readings_changed(old, &readings))
        {
            last = Some(readings);
            for fan in fans {
                fan.set_speed(fan.calc_speed(&readings, cover_open))?;
            }
        }

        // The cards' own fans, driven per-card. Deliberately outside the
        // `readings_changed` gate above: that compares the *reduced* group
        // maximum, so one card warming while the hottest holds steady leaves
        // the readings equal and would skip this card's ramp entirely.
        let mut gpu_fan_status = Vec::new();
        if gpu_fans.enabled {
            if let Some(backend) = gpu_group.nvidia() {
                for target in backend.fan_targets() {
                    let percent = fan_controller::gpu_fan_percent(&target, &gpu_fans);
                    let percent = if cover_open {
                        // Same argument as the chassis cover-open floor: with the
                        // cover off there is no chassis static pressure, so the
                        // card's own blower is doing all the work.
                        percent.max(gpu_fans.max_percent)
                    } else {
                        percent
                    };
                    backend.set_fan_percent(target.index, percent);

                    if !status_path.is_empty() {
                        // Only the target belongs here; everything else about the
                        // card comes from `full_status()`, which also covers cards
                        // this daemon does not drive.
                        gpu_fan_status.push(status::GpuFan {
                            index: target.index,
                            target_pct: Some(percent),
                        });
                    }
                }
            }
        }

        // Publish what this poll just read. Failures are swallowed on purpose: a
        // status file that cannot be written must never take fan control down.
        if !status_path.is_empty() {
            let cards = gpu_group
                .nvidia()
                .map(nvidia::NvidiaBackend::full_status)
                .unwrap_or_default();
            // Re-read per-sensor values for the breakdown. Cheap sysfs reads, and
            // it keeps `effective()` above as the single path the curve depends
            // on rather than teeing off it.
            let gpu_sensors = gpu_group.read_each(&mut temp_buffer);
            let cpu_sensors = cpu_group.read_each(&mut temp_buffer);
            let psu_sensors = psu_group.read_each(&mut temp_buffer);

            let snapshot = status::Snapshot {
                unix_time: now_unix(),
                daemon: status::Daemon {
                    version: env!("CARGO_PKG_VERSION"),
                    pid: std::process::id(),
                    started_unix: started,
                    uptime_secs: now_unix().saturating_sub(started),
                    config_file: config_file.to_owned(),
                    profile: profile_name.to_owned(),
                    poll_secs: poll.as_secs(),
                    fall_samples,
                    cover_mode: cover_mode.to_string(),
                    holding_manual,
                    gpu_fan_control: gpu_fans.enabled,
                },
                smc: status::Smc {
                    present: smc.is_some(),
                    keys: smc.map_or(0, smc::Smc::key_count),
                    cover_open,
                    gpu_throttle: throttled,
                    system_power_w: readings.watts,
                },
                groups: vec![
                    status::TempGroup {
                        name: "gpu",
                        effective_c: readings.gpu,
                        sensors: gpu_sensors,
                    },
                    status::TempGroup {
                        name: "cpu",
                        effective_c: readings.cpu,
                        sensors: cpu_sensors,
                    },
                    status::TempGroup {
                        name: "psu",
                        effective_c: readings.psu,
                        sensors: psu_sensors,
                    },
                ],
                chassis_fans: fans
                    .into_iter()
                    .enumerate()
                    .map(|(i, f)| status::ChassisFan {
                        number: i + 1,
                        source: f.source(),
                        target_rpm: f.calc_speed(&readings, cover_open),
                        actual_rpm: f.actual_rpm(),
                        min_rpm: f.min_speed(),
                        max_rpm: f.max_speed(),
                        explain: f.explain(&readings),
                    })
                    .collect(),
                cards,
                gpu_targets: gpu_fan_status,
            };
            let _ = snapshot.publish(std::path::Path::new(status_path));
        }

        std::thread::sleep(poll);
    }

    // Hand the cards' fans back before returning. The chassis fans have the
    // unit's `ExecStopPost` as a SIGKILL backstop; NVML settings have no such
    // net, so a clean exit must undo them explicitly.
    if gpu_fans.enabled {
        if let Some(backend) = gpu_group.nvidia() {
            backend.restore_fans();
            println!("neofand: returned GPU fans to the driver curve");
        }
    }

    Ok(())
}

fn real_main() -> Result<()> {
    if get_current_euid() != 0 {
        return Err(Error::NotRoot);
    }

    check_pid_file()?;

    let mut temp_buffer = String::new();

    let fan_paths = find_fan_paths()?;
    let config = load_config(fan_paths.len_nonzero())?;

    println!(
        "neofand: profile={} poll={}s fall_samples={} sensor={}",
        config.general.profile,
        config.general.poll_interval.as_secs(),
        config.general.fall_samples,
        config.gpu.sensor_preference,
    );

    // Cover detection and total-system-power both come from raw SMC keys, so
    // this is optional: without it the daemon still runs, just without the
    // interlock and with the PSU curve falling back to temperature alone.
    let smc = smc::Smc::discover();
    match &smc {
        Some(smc) if smc.has(smc::KEY_COVER_OPEN) => {}
        Some(_) => eprintln!(
            "neofand: WARNING SMC found but no '{}' key — cover interlock disabled. Do not run \
             with the cover off.",
            smc::KEY_COVER_OPEN
        ),
        None => eprintln!(
            "neofand: WARNING no SMC key interface — cover interlock disabled. Do not run with \
             the cover off."
        ),
    }
    if config.general.cover_mode != CoverMode::Off
        && smc.as_ref().is_some_and(|s| s.has(smc::KEY_COVER_OPEN))
    {
        println!(
            "neofand: cover interlock armed via {} (cover_mode={})",
            smc::KEY_COVER_OPEN,
            config.general.cover_mode
        );
    }

    // GPU discovery has to finish before the fans are built, because a fan's GPU
    // thresholds may be derived from what the cards themselves report.
    let gpu_sensors = settle_gpu_sensors(&config.gpu)?;
    let gpu_limit = gpu::min_limit(&gpu_sensors);

    // Auto-derive each fan's GPU ramp from the hardware, where the config asked
    // for it by leaving both thresholds at 0. Done per fan rather than globally
    // so a fan with explicit numbers keeps them.
    let derived = config::GpuFanConfig::derive_chassis_ramp(gpu_limit);
    let mut config_fans = config.fans;
    for fan in &mut config_fans {
        if fan.gpu_low_temp != 0 || fan.gpu_high_temp != 0 {
            continue;
        }
        match derived {
            Some((low, high)) => {
                fan.gpu_low_temp = low;
                fan.gpu_high_temp = high;
            }
            None => {
                // Nothing published a limit, so there is nothing to derive from.
                // Fall back to the documented pair rather than leaving 0/0, which
                // would make the ramp demand 0 forever.
                fan.gpu_low_temp = config::FALLBACK_LOW;
                fan.gpu_high_temp = config::FALLBACK_HIGH;
                eprintln!(
                    "neofand: WARNING gpu thresholds set to auto but no GPU published a limit \
                     — falling back to {}-{} C",
                    config::FALLBACK_LOW,
                    config::FALLBACK_HIGH
                );
            }
        }
    }
    if config_fans.iter().any(|f| f.source == FanSource::Gpu) {
        if let (Some(limit), Some((low, high))) = (gpu_limit, derived) {
            println!(
                "neofand: gpu chassis ramp {low}-{high} C auto-derived from the coolest \
                 throttle point present ({limit} C)"
            );
        }
    }

    // Captured before `config_fans` is consumed below.
    let highest_gpu_high = config_fans
        .iter()
        .map(|f| f.gpu_high_temp)
        .max()
        .unwrap_or(0);

    let fans = fan_paths
        .into_iter()
        .zip(config_fans)
        .map(|(path, fan_config)| FanController::new(path, fan_config))
        .collect::<Result<Vec<_>>>()?;
    let fans = NonEmptyVec::from_vec(fans).ok_or(Error::NoFan)?;

    let gpu_sensors = gpu_sensors
        .into_iter()
        .map(|s| (s.path, s.desc))
        .collect::<Vec<_>>();

    let nvidia = if config.gpu.use_nvml {
        nvidia::NvidiaBackend::init(
            config.gpu.sensor_preference.wants_memory(),
            config.gpu_fans,
        )
    } else {
        None
    };

    // The profile thresholds assume a ~100 C throttle point (gfx906). NVIDIA
    // workstation cards typically throttle well below that, so a config that is
    // safe on AMD can be useless on NVIDIA — the fans would never reach full
    // speed before the GPU started clocking down. Warn rather than silently
    // rewrite the user's thresholds.
    if let Some(slowdown) = nvidia.as_ref().and_then(nvidia::NvidiaBackend::min_slowdown) {
        if u32::from(highest_gpu_high) >= slowdown {
            eprintln!(
                "neofand: WARNING gpu_high_temp={highest_gpu_high} C is at or above the \
                 NVIDIA slowdown threshold of {slowdown} C — fans will not reach maximum \
                 until the GPU is already throttling. Lower gpu_high_temp to ~{} C.",
                slowdown.saturating_sub(8)
            );
        }
    }

    if config.gpu_fans.enabled {
        match nvidia.as_ref() {
            Some(backend) if backend.has_controllable_fans() => println!(
                // Per-card ramps are reported next to each card in the sensor
                // lines above, since with auto-derivation they can differ.
                "neofand: GPU fans {}-{}%, {}, ramp {} (per-card, each follows its own die)",
                config.gpu_fans.min_percent,
                config.gpu_fans.max_percent,
                config.gpu_fans.curve,
                if config.gpu_fans.low_temp == 0 || config.gpu_fans.high_temp == 0 {
                    "auto-derived from each card's slowdown threshold"
                } else {
                    "from config"
                },
            ),
            // Enabled but unusable is worth saying out loud: the cards are then
            // still on their firmware curve, which is the thing the setting was
            // turned on to replace.
            _ => eprintln!(
                "neofand: WARNING [GpuFans] enabled but no NVIDIA card exposes a controllable \
                 fan — the cards stay on their own firmware curve"
            ),
        }
    }

    let gpu_group = TempGroup::from_paths(
        "gpu",
        gpu_sensors,
        config.general.fall_samples,
        &mut temp_buffer,
    )
    .with_nvidia(nvidia);
    let cpu_group = TempGroup::from_glob(
        "cpu",
        CPU_TEMP_GLOB,
        config.general.fall_samples,
        &mut temp_buffer,
    );

    // PSU sensors come from the SMC's `Tp*` channels; there is no hwmon
    // equivalent. Selected by label prefix because the temp*_input numbering
    // shifts between driver versions.
    let psu_paths = smc
        .as_ref()
        .map(|smc| smc.temp_paths_with_prefix("Tp"))
        .unwrap_or_default();
    let psu_group = TempGroup::from_paths(
        "psu",
        psu_paths,
        config.general.fall_samples,
        &mut temp_buffer,
    );

    // GPU temperature is the whole point of this fork; refuse to run blind to
    // it. A missing CPU group is survivable but worth shouting about, since
    // manual mode means the SMC is no longer ramping for the CPU either.
    if gpu_group.is_empty() {
        return Err(Error::NoSensors("gpu"));
    }
    if cpu_group.is_empty() {
        eprintln!("neofand: WARNING no CPU sensors found — fans will follow GPU only");
    }
    if psu_group.is_empty() {
        eprintln!(
            "neofand: WARNING no PSU (Tp*) sensors found — any psu-source fan will follow total \
             system power only"
        );
    }

    // Manual mode is claimed inside the loop, not here: with the cover off the
    // interlock must never take it in the first place.
    let res = start_temp_loop(
        temp_buffer,
        gpu_group,
        cpu_group,
        psu_group,
        smc.as_ref(),
        config.general.cover_mode,
        &fans,
        config.gpu_fans,
        &config.general.status_path,
        &config.general.profile.to_string(),
        config::config_file_path(),
        config.general.fall_samples,
        config.general.poll_interval,
    );

    println!("\nneofand: shutting down, returning fans to SMC control...");
    for fan in fans {
        fan.set_manual(false)?;
    }

    let pid_res = std::fs::remove_file(PID_FILE).map_err(Error::PidDelete);
    match (res, pid_res) {
        (Err(err), _) | (_, Err(err)) => Err(err),
        (Ok(()), Ok(())) => Ok(()),
    }
}
