//! Vendor-agnostic GPU temperature discovery.
//!
//! Rather than hardcoding a driver's sysfs layout, this walks `/sys/class/hwmon`
//! and keeps any node whose `name` is a known GPU driver. That covers amdgpu,
//! nouveau, radeon and Intel's i915/xe with the same code path, because they all
//! register standard hwmon nodes.
//!
//! Channel selection is by `temp*_label`, not by index, since the numbering is
//! not consistent between drivers: amdgpu reports edge/junction/mem as
//! temp1/2/3, while nouveau and i915 typically expose a single unlabelled
//! temp1. Preferring a *label* and falling back to the lowest-numbered channel
//! keeps both cases correct.
//!
//! NVIDIA's proprietary driver registers no hwmon node at all. Point
//! `extra_temp_paths` at something that does (nouveau, or a file fed by
//! `nvidia-smi`) to include those GPUs.

use std::path::PathBuf;

use crate::{
    config::{GpuConfig, SensorPreference},
    error::Result,
};

/// A discovered temperature channel.
pub struct Sensor {
    pub path: PathBuf,
    pub desc: String,
    /// The card's own throttle/shutdown point for this channel, in whole degrees.
    ///
    /// Read from the hardware rather than assumed, so a fan ramp can be derived
    /// per card instead of from a profile constant keyed to whatever GPU
    /// happened to be installed when the profile was written.
    pub limit_c: Option<u32>,
}

/// Labels that identify each kind of channel, lowercased substring match.
const JUNCTION_HINTS: &[&str] = &["junction", "hotspot"];
const EDGE_HINTS: &[&str] = &["edge", "gpu", "gt"];
const MEMORY_HINTS: &[&str] = &["mem", "vram", "hbm"];

fn label_of(input_path: &PathBuf) -> Option<String> {
    let name = input_path.file_name()?.to_str()?;
    let stem = name.strip_suffix("_input")?;
    let label_path = input_path.with_file_name(format!("{stem}_label"));
    let raw = std::fs::read_to_string(label_path).ok()?;
    Some(raw.trim().to_lowercase())
}

/// The throttle point hwmon advertises for one channel, in whole degrees.
///
/// `crit` is the standard hwmon name and what amdgpu uses; `emergency` and `max`
/// are tried after it so a driver that only publishes one of those still yields
/// a limit rather than falling back to a hardcoded guess.
fn limit_of(input_path: &PathBuf) -> Option<u32> {
    let name = input_path.file_name()?.to_str()?;
    let stem = name.strip_suffix("_input")?;

    for suffix in ["crit", "emergency", "max"] {
        let candidate = input_path.with_file_name(format!("{stem}_{suffix}"));
        let Ok(raw) = std::fs::read_to_string(candidate) else {
            continue;
        };
        let Ok(millidegrees) = raw.trim().parse::<i64>() else {
            continue;
        };
        // Drivers publish an unset threshold as a large negative sentinel
        // (temp1_crit_hyst reads -273150 on this hardware), so reject anything
        // that is not a plausible silicon limit rather than deriving from it.
        if millidegrees > 30_000 && millidegrees < 200_000 {
            return u32::try_from(millidegrees / 1000).ok();
        }
    }

    None
}

fn matches(label: Option<&String>, hints: &[&str]) -> bool {
    let Some(label) = label else { return false };
    hints.iter().any(|hint| label.contains(hint))
}

/// Every `temp*_input` under one hwmon directory, sorted by channel number so
/// "lowest-numbered" is well defined.
fn channels(hwmon: &PathBuf) -> Vec<PathBuf> {
    let pattern = format!("{}/temp*_input", hwmon.display());
    let mut found = glob::glob(&pattern)
        .into_iter()
        .flatten()
        .filter_map(std::result::Result::ok)
        .collect::<Vec<_>>();
    found.sort();
    found
}

/// Pick the channels to watch on a single GPU.
fn select(hwmon: &PathBuf, driver: &str, preference: SensorPreference) -> Vec<Sensor> {
    let channels = channels(hwmon);
    if channels.is_empty() {
        return Vec::new();
    }

    let labelled = channels
        .into_iter()
        .map(|path| {
            let label = label_of(&path);
            (path, label)
        })
        .collect::<Vec<_>>();

    // Identify the GPU by its PCI address. Without this, two dies of the same
    // card produce identical descriptions and the log cannot tell them apart.
    let device = std::fs::canonicalize(hwmon.join("device"))
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| {
            hwmon
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "?".to_owned())
        });

    let describe = |path: &PathBuf, label: Option<&String>| {
        let channel = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("temp?_input");
        match label {
            Some(label) => format!("{driver} {device} {channel} ({label})"),
            None => format!("{driver} {device} {channel}"),
        }
    };

    // `Max` watches every channel and lets the hottest win, which is the safe
    // option on an unfamiliar driver.
    if matches!(preference, SensorPreference::Max) {
        return labelled
            .iter()
            .map(|(path, label)| Sensor {
                limit_c: limit_of(path),
                path: path.clone(),
                desc: describe(path, label.as_ref()),
            })
            .collect();
    }

    // Preferred label first. Junction falls back through edge before giving up,
    // because on a driver that only reports one unlabelled channel that channel
    // is the best available proxy.
    let fallbacks: &[&[&str]] = match preference {
        SensorPreference::Junction => &[JUNCTION_HINTS, EDGE_HINTS],
        SensorPreference::Edge => &[EDGE_HINTS],
        SensorPreference::Memory => &[MEMORY_HINTS],
        SensorPreference::Max => unreachable!("handled above"),
    };

    for hint_set in fallbacks {
        if let Some((path, label)) = labelled
            .iter()
            .find(|(_, label)| matches(label.as_ref(), hint_set))
        {
            return vec![Sensor {
                limit_c: limit_of(path),
                path: path.clone(),
                desc: describe(path, label.as_ref()),
            }];
        }
    }

    // Nothing labelled usefully — take the lowest-numbered channel. This is the
    // nouveau / i915 case, where a bare temp1_input is the GPU temperature.
    let (path, label) = &labelled[0];
    vec![Sensor {
        limit_c: limit_of(path),
        path: path.clone(),
        desc: format!("{} [unlabelled fallback]", describe(path, label.as_ref())),
    }]
}

/// All GPU temperature channels to monitor, across every vendor present.
pub fn discover(config: &GpuConfig) -> Result<Vec<Sensor>> {
    let mut sensors = Vec::new();

    let mut hwmons = glob::glob("/sys/class/hwmon/hwmon*")?
        .filter_map(std::result::Result::ok)
        .collect::<Vec<_>>();
    hwmons.sort();

    for hwmon in hwmons {
        let Ok(raw_name) = std::fs::read_to_string(hwmon.join("name")) else {
            continue;
        };
        let name = raw_name.trim().to_lowercase();

        // An empty `name` means the driver used the deprecated hwmon
        // registration path (applesmc does this), so it is never a GPU we want.
        if name.is_empty() {
            continue;
        }

        if !config.drivers.iter().any(|d| d.eq_ignore_ascii_case(&name)) {
            continue;
        }

        sensors.extend(select(&hwmon, &name, config.sensor_preference));
    }

    // Explicit escape hatch, appended after the auto-discovered set.
    for pattern in &config.extra_temp_paths {
        for path in glob::glob(pattern)?.filter_map(std::result::Result::ok) {
            let desc = format!("extra:{}", path.display());
            let limit_c = limit_of(&path);
            sensors.push(Sensor {
                path,
                desc,
                limit_c,
            });
        }
    }

    Ok(sensors)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a fake hwmon channel in a temp dir and read its limit back.
    ///
    /// `case` must be unique per test: these run in parallel in one process, and
    /// a shared directory means one test deletes another's fixture mid-read.
    fn limit_from(case: &str, files: &[(&str, &str)]) -> Option<u32> {
        let dir = std::env::temp_dir()
            .join(format!("neofand-gpu-test-{}-{case}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        for (name, contents) in files {
            std::fs::write(dir.join(name), contents).expect("write");
        }
        let limit = limit_of(&dir.join("temp1_input"));
        std::fs::remove_dir_all(&dir).ok();
        limit
    }

    #[test]
    fn reads_crit_in_whole_degrees() {
        // What the RX 460 actually publishes.
        assert_eq!(
            limit_from("crit", &[("temp1_input", "34000\n"), ("temp1_crit", "94000\n")]),
            Some(94)
        );
    }

    #[test]
    fn rejects_the_unset_sentinel() {
        // temp1_crit_hyst reads -273150 on this hardware. Deriving a fan ramp
        // from absolute zero would peg the fans forever, so an implausible
        // value must be refused rather than clamped.
        assert_eq!(
            limit_from("sentinel", &[("temp1_input", "34000\n"), ("temp1_crit", "-273150\n")]),
            None
        );
    }

    #[test]
    fn falls_through_crit_to_emergency() {
        // A driver that only publishes one of the three still yields a limit.
        assert_eq!(
            limit_from(
                "emergency",
                &[
                ("temp1_input", "34000\n"),
                ("temp1_crit", "-273150\n"),
                ("temp1_emergency", "100000\n"),
                ]
            ),
            Some(100)
        );
    }

    #[test]
    fn no_limit_files_is_not_an_error() {
        assert_eq!(limit_from("bare", &[("temp1_input", "34000\n")]), None);
    }
}

/// The coolest throttle point across a discovered set, in whole degrees.
///
/// The chassis fans are driven from one reduced temperature rather than per card,
/// so the ramp has to suit the card that throttles earliest — anything hotter
/// would let that card start clocking down before the fans reached maximum.
pub fn min_limit(sensors: &[Sensor]) -> Option<u32> {
    sensors.iter().filter_map(|s| s.limit_c).min()
}
