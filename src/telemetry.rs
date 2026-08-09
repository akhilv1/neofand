//! What gets logged, and how it is gathered.
//!
//! This module owns the *reading* side of telemetry; `logdb` owns the storing
//! side and `web` the serving side. The split matters because of the constraint
//! in `smc.rs`: `key_at_index` is one shared index register, so raw SMC reads
//! must happen on the daemon's control thread and nowhere else. Everything here
//! is therefore called from that thread, produces owned values, and hands them
//! off through a channel. No other thread ever touches the SMC.
//!
//! ## What is captured
//!
//! * **SMC power keys** — the whole `P*` space. The zone gauges (`PZ*G`), their
//!   ceilings (`PZ*E`), filtered averages (`PZ*F`) and trip flags (`PZ*T`), plus
//!   the per-rail meters (`PG0R`, `PG2R`, `PD2R`) and system totals (`PSTR`).
//!   This is the data that showed a GPU zone running 387 W against a 375 W
//!   budget, which no other interface on this machine exposes.
//! * **SMC temperatures** — all ~239 labelled channels, not just the nine the
//!   PSU curve consumes.
//! * **Chassis fans** — commanded target and actual RPM, which diverge on
//!   purpose here because `fan1_max` under-reports.
//! * **GPUs** — power, core and memory temperature, memory used, fan duty.
//!
//! ## Units
//!
//! Values are stored as `f64` in their natural unit (watts, degrees C, RPM,
//! percent, bytes). No scaling is applied on the way in, so a query returns what
//! the hardware reported.

use std::{collections::HashMap, path::PathBuf};

use crate::smc::Smc;

/// How a channel behaves, which decides its deadband when stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Temp,
    Power,
    /// Rail voltage. Needs a far finer deadband than watts: these sit at 12 V and
    /// move by tens of millivolts, so a 1 W-scale threshold would record a
    /// perfectly flat line and hide exactly the sag worth seeing.
    Volts,
    Amps,
    Rpm,
    Percent,
    Bytes,
    /// Flags and trip states. Always written on change, never deadbanded — a
    /// `PZ3T` trip that lasts less than one sample interval is precisely the
    /// event this whole store exists to catch.
    Discrete,
}

pub struct Reading {
    pub subsystem: &'static str,
    pub name: String,
    pub unit: &'static str,
    pub value: f64,
    pub kind: Kind,
}

impl Reading {
    pub fn new(
        subsystem: &'static str,
        name: impl Into<String>,
        unit: &'static str,
        value: f64,
        kind: Kind,
    ) -> Self {
        Self {
            subsystem,
            name: name.into(),
            unit,
            value,
            kind,
        }
    }
}

pub struct Sample {
    pub unix_time: u64,
    pub readings: Vec<Reading>,
}

/// Resolved once at startup so each sample is pure I/O with no discovery cost.
///
/// Both lists are resolved by name/label rather than index, because SMC key
/// indices and hwmon `tempN` numbering are documented as unstable across driver
/// versions — caching indices here would silently log the wrong channel after an
/// update.
pub struct Collector {
    power_keys: Vec<String>,
    flag_keys: Vec<String>,
    temp_paths: Vec<(PathBuf, String)>,
}

/// SMC keys that are booleans rather than measurements.
const FLAG_KEYS: &[&str] = &["MSCO", "GTHR"];

/// Is this a physically possible reading for its unit?
///
/// The SMC exposes many channels for hardware this machine does not have, and
/// they do not read as absent -- they read as sentinels. Measured here: 111
/// channels report `-127 C` (the classic "no sensor" marker), `-38 C`, or in one
/// case `-26164 A`. Those are not cold sensors; they are unpopulated ones, and a
/// key whose declared type decodes cleanly is not necessarily a measurement.
///
/// Storing them costs a third of every sample forever and puts junk in the
/// channel picker. The windows below are deliberately generous -- wide enough
/// that no real reading is at risk, narrow enough that sentinels cannot pass.
fn plausible(kind: Kind, value: f64) -> bool {
    if !value.is_finite() {
        return false;
    }
    match kind {
        // A chassis that reached 150 C has bigger problems than telemetry, and
        // nothing indoors reads below -40.
        Kind::Temp => (-40.0..=150.0).contains(&value),
        Kind::Volts => (-100.0..=100.0).contains(&value),
        Kind::Amps => (-1000.0..=1000.0).contains(&value),
        Kind::Power => (-100.0..=5000.0).contains(&value),
        Kind::Rpm => (0.0..=20000.0).contains(&value),
        Kind::Percent => (0.0..=100.0).contains(&value),
        Kind::Bytes | Kind::Discrete => true,
    }
}

impl Collector {
    pub fn new(smc: &Smc) -> Self {
        // Every `P*`/`V*`/`I*` key that decodes to a number. `read_numeric`
        // returns `None` for string and struct types, so filtering on a probe
        // read here keeps non-measurements out of the store permanently rather
        // than producing a null column forever.
        //
        // Volts and amps matter beyond completeness: dividing a power key by its
        // current key is what identifies which rail it measures. `PSTR/IDTR` and
        // `PZ7G/IDTA` both come to 11.975 V, which is how we know `PZ7` is a
        // 12 V DC measurement and not the PSU's AC input — and therefore that
        // `PZ0/PZ7` is not a conversion efficiency.
        let power_keys = ["P", "V", "I"]
            .iter()
            .flat_map(|prefix| smc.key_names_with_prefix(prefix))
            .filter(|key| smc.read_numeric(key).is_some())
            .collect();

        let flag_keys = FLAG_KEYS
            .iter()
            .filter(|key| smc.has(key))
            .map(|key| (*key).to_owned())
            .collect();

        // Empty prefix matches every labelled channel.
        let temp_paths = smc.temp_paths_with_prefix("");

        Self {
            power_keys,
            flag_keys,
            temp_paths,
        }
    }

    pub fn power_key_count(&self) -> usize {
        self.power_keys.len()
    }

    pub fn temp_count(&self) -> usize {
        self.temp_paths.len()
    }

    /// Read every SMC channel into `out`.
    ///
    /// Must be called from the control thread only.
    pub fn collect(&self, smc: &Smc, out: &mut Vec<Reading>) {
        for key in &self.power_keys {
            if let Some(value) = smc.read_numeric(key) {
                // `PZ*T` are trip states that happen to be stored as floats.
                // Classifying them as discrete is what keeps a brief trip from
                // being deadbanded away.
                let is_trip = key.len() == 4 && key.starts_with("PZ") && key.ends_with('T');
                let (unit, kind) = if is_trip {
                    ("", Kind::Discrete)
                } else {
                    match key.as_bytes().first() {
                        Some(b'V') => ("V", Kind::Volts),
                        Some(b'I') => ("A", Kind::Amps),
                        _ => ("W", Kind::Power),
                    }
                };
                if plausible(kind, value) {
                    out.push(Reading::new("smc_power", key.clone(), unit, value, kind));
                }
            }
        }

        for key in &self.flag_keys {
            if let Some(state) = smc.flag(key) {
                out.push(Reading::new(
                    "smc_flag",
                    key.clone(),
                    "",
                    f64::from(u8::from(state)),
                    Kind::Discrete,
                ));
            }
        }

        for (path, label) in &self.temp_paths {
            // millidegrees C, as hwmon always reports.
            let Ok(raw) = std::fs::read_to_string(path) else {
                continue;
            };
            let Ok(milli) = raw.trim().parse::<f64>() else {
                continue;
            };
            // Unpopulated channels read exactly 0 and there are ~90 of them;
            // logging them wastes a third of every sample forever.
            if milli == 0.0 {
                continue;
            }
            let celsius = milli / 1000.0;
            if !plausible(Kind::Temp, celsius) {
                continue;
            }
            out.push(Reading::new(
                "smc_temp",
                label.clone(),
                "C",
                celsius,
                Kind::Temp,
            ));
        }
    }
}


/// Host CPU and memory load.
///
/// Neither is reachable through the SMC -- `/proc` is the only source -- but
/// they belong in the same store: correlating a power-zone trip against what the
/// box was actually doing is the whole reason to keep history.
///
/// CPU utilisation is a *rate*, so it needs the previous jiffy counts. That
/// state lives here rather than being re-derived, which also means the first
/// sample after startup reports nothing rather than a bogus figure computed
/// against boot.
#[derive(Default)]
pub struct HostLoad {
    previous: Option<(u64, u64)>,
}

impl HostLoad {
    pub fn sample(&mut self, out: &mut Vec<Reading>) {
        self.cpu(out);
        self.memory(out);
    }

    /// Busy percentage across all cores, from the aggregate line of /proc/stat.
    fn cpu(&mut self, out: &mut Vec<Reading>) {
        let Ok(stat) = std::fs::read_to_string("/proc/stat") else {
            return;
        };
        let Some(line) = stat.lines().next().filter(|l| l.starts_with("cpu ")) else {
            return;
        };
        let fields: Vec<u64> = line
            .split_whitespace()
            .skip(1)
            .filter_map(|f| f.parse().ok())
            .collect();
        // user nice system idle iowait irq softirq steal ...
        if fields.len() < 5 {
            return;
        }
        let total: u64 = fields.iter().sum();
        // Idle proper plus iowait: a core waiting on disk is not doing work.
        let idle = fields[3] + fields[4];

        if let Some((prev_total, prev_idle)) = self.previous {
            let dt = total.saturating_sub(prev_total);
            let di = idle.saturating_sub(prev_idle);
            if dt > 0 {
                let busy = 100.0 * (dt.saturating_sub(di)) as f64 / dt as f64;
                out.push(Reading::new(
                    "host",
                    "cpu_pct",
                    "%",
                    busy.clamp(0.0, 100.0),
                    Kind::Percent,
                ));
            }
        }
        self.previous = Some((total, idle));
    }

    fn memory(&mut self, out: &mut Vec<Reading>) {
        let Ok(info) = std::fs::read_to_string("/proc/meminfo") else {
            return;
        };
        let mut total_kb = 0u64;
        let mut available_kb = 0u64;
        for line in info.lines() {
            let Some((key, rest)) = line.split_once(':') else {
                continue;
            };
            let Some(kb) = rest.split_whitespace().next().and_then(|v| v.parse::<u64>().ok()) else {
                continue;
            };
            match key {
                "MemTotal" => total_kb = kb,
                // MemAvailable, not MemFree: page cache is reclaimable, so
                // MemFree reports a machine with a warm cache as nearly full.
                "MemAvailable" => available_kb = kb,
                _ => {}
            }
        }
        if total_kb == 0 {
            return;
        }
        let used_kb = total_kb.saturating_sub(available_kb);
        out.push(Reading::new(
            "host",
            "mem_used_pct",
            "%",
            100.0 * used_kb as f64 / total_kb as f64,
            Kind::Percent,
        ));
        out.push(Reading::new(
            "host",
            "mem_used_bytes",
            "B",
            used_kb as f64 * 1024.0,
            Kind::Bytes,
        ));
        out.push(Reading::new(
            "host",
            "mem_total_bytes",
            "B",
            total_kb as f64 * 1024.0,
            Kind::Bytes,
        ));
    }
}

/// Temperature sources that live in hwmon rather than the SMC.
///
/// The SMC has ~239 channels but not these: NVMe composite temperature comes
/// from the drive's own controller, CPU package temperature from `coretemp`, and
/// the NIC PHY/MAC sensors from the network driver. The SMC's `TH*`/`TC*`
/// channels are the *board's* opinion of nearby temperature, which is not the
/// same measurement.
pub struct Hwmon {
    /// (`temp*_input` path, metric name).
    channels: Vec<(PathBuf, String)>,
}

/// hwmon drivers worth recording. An allowlist rather than everything, because
/// `/sys/class/hwmon` also carries battery and virtual nodes with no bearing on
/// this machine's thermals.
const HWMON_DRIVERS: &[&str] = &[
    "nvme",
    "coretemp",
    "acpitz",
    "pch_lewisburg",
    "amdgpu",
    "enp3s0",
    "enp4s0",
];

fn sanitize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_owned()
}

impl Hwmon {
    pub fn discover() -> Self {
        let mut channels = Vec::new();
        let mut seen_driver_count: HashMap<String, usize> = HashMap::new();

        let Ok(entries) = std::fs::read_dir("/sys/class/hwmon") else {
            return Self { channels };
        };
        let mut dirs: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        // Directory order is not stable; sort so the disambiguating index for two
        // identically named drivers (two NVMe controllers) stays put across
        // restarts and a series does not silently swap drives.
        dirs.sort();

        for dir in dirs {
            let Ok(name) = std::fs::read_to_string(dir.join("name")) else {
                continue;
            };
            let name = name.trim().to_owned();
            if !HWMON_DRIVERS.contains(&name.as_str()) {
                continue;
            }
            let ordinal = seen_driver_count.entry(name.clone()).or_insert(0);
            let index = *ordinal;
            *ordinal += 1;

            let Ok(files) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut inputs: Vec<PathBuf> = files
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    p.file_name()
                        .and_then(|f| f.to_str())
                        .is_some_and(|f| f.starts_with("temp") && f.ends_with("_input"))
                })
                .collect();
            inputs.sort();

            for input in inputs {
                let Some(stem) = input
                    .file_name()
                    .and_then(|f| f.to_str())
                    .and_then(|f| f.strip_suffix("_input"))
                else {
                    continue;
                };
                let label = std::fs::read_to_string(input.with_file_name(format!("{stem}_label")))
                    .map(|l| l.trim().to_owned())
                    .unwrap_or_default();

                // Skip the 24 per-core channels: they track the package within a
                // few degrees and would triple this subsystem for no new signal.
                // The package is the number that matters for cooling.
                if name == "coretemp" && label.starts_with("Core ") {
                    continue;
                }

                let leaf = if label.is_empty() {
                    stem.to_owned()
                } else {
                    sanitize(&label)
                };
                let metric = if *seen_driver_count.get(&name).unwrap_or(&1) > 1 || index > 0 {
                    format!("{}{index}_{leaf}", sanitize(&name))
                } else {
                    format!("{}_{leaf}", sanitize(&name))
                };
                channels.push((input, metric));
            }
        }
        Self { channels }
    }

    pub fn len(&self) -> usize {
        self.channels.len()
    }

    pub fn collect(&self, out: &mut Vec<Reading>) {
        for (path, metric) in &self.channels {
            let Ok(raw) = std::fs::read_to_string(path) else {
                continue;
            };
            let Ok(milli) = raw.trim().parse::<f64>() else {
                continue;
            };
            let celsius = milli / 1000.0;
            if !plausible(Kind::Temp, celsius) {
                continue;
            }
            out.push(Reading::new("hwmon", metric.clone(), "C", celsius, Kind::Temp));
        }
    }
}
