use std::{io::ErrorKind, num::NonZeroUsize, str::FromStr, time::Duration};

use crate::{Error, Result};

#[cfg(debug_assertions)]
const CONFIG_FILE: &str = "./neofand.conf";
#[cfg(not(debug_assertions))]
const CONFIG_FILE: &str = "/etc/neofand.conf";

/// Upper bound on the fall-off window. The runtime value is clamped to this.
pub const MAX_FALL_SAMPLES: usize = 60;

#[derive(Clone, Copy, Debug)]
pub enum SpeedCurve {
    Linear,
    Exponential,
    Logarithmic,
}

impl std::fmt::Display for SpeedCurve {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Linear => f.write_str("linear"),
            Self::Exponential => f.write_str("exponential"),
            Self::Logarithmic => f.write_str("logarithmic"),
        }
    }
}

impl FromStr for SpeedCurve {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_lowercase().as_str() {
            "linear" => Self::Linear,
            "exponential" => Self::Exponential,
            "logarithmic" => Self::Logarithmic,
            _ => return Err(()),
        })
    }
}

/// What to do about the fans while the cover is off.
///
/// With the cover removed the PSU, CPU and MPX modules lose the chassis static
/// pressure they are cooled by, so *something* has to guarantee airflow. Holding
/// manual control at a normal idle curve does not — measured, the fans fall to
/// ~500 RPM and stay there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverMode {
    /// Keep control, but never fall below each fan's `cover_open_rpm`.
    ///
    /// Preferred: the curve can still ramp *above* the floor for a real thermal
    /// load, which handing control back cannot do — the T2's cover-off ramp was
    /// only ever observed as a flat 1200/2250 regardless of temperature.
    Floor,
    /// Release `fan*_manual` and let the T2 do whatever it does.
    ///
    /// Safest in the sense that it is exactly stock behaviour, but it gives up
    /// thermal responsiveness while the cover is off.
    Release,
    /// Ignore cover state entirely. Do not use on this hardware.
    Off,
}

impl std::fmt::Display for CoverMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Floor => f.write_str("floor"),
            Self::Release => f.write_str("release"),
            Self::Off => f.write_str("off"),
        }
    }
}

impl FromStr for CoverMode {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_lowercase().as_str() {
            "floor" | "enforce" | "min" => Self::Floor,
            "release" | "smc" | "handback" => Self::Release,
            "off" | "ignore" | "none" | "false" => Self::Off,
            _ => return Err(()),
        })
    }
}

/// What a given fan is cooling, and therefore which sensors drive it.
///
/// On MacPro7,1 the four chassis fans serve different components, established by
/// stalling each one by hand: `fan1` = PSU, `fan2`/`fan3` = the MPX/GPU bays,
/// `fan4` = the CPU cooler. Pointing every fan at GPU temperature (the default
/// inherited from the fork) leaves the PSU and CPU fans following a signal that
/// has nothing to do with what they cool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FanSource {
    /// GPU temperature only.
    Gpu,
    /// CPU package temperature only.
    Cpu,
    /// PSU: total system power as feed-forward, PSU temperature as the backstop.
    Psu,
    /// Whichever of GPU or CPU asks for more. The safe default when the fan's
    /// role is unknown.
    GpuCpu,
}

impl std::fmt::Display for FanSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gpu => f.write_str("gpu"),
            Self::Cpu => f.write_str("cpu"),
            Self::Psu => f.write_str("psu"),
            Self::GpuCpu => f.write_str("gpu+cpu"),
        }
    }
}

impl FromStr for FanSource {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_lowercase().as_str() {
            "gpu" => Self::Gpu,
            "cpu" => Self::Cpu,
            "psu" | "power" => Self::Psu,
            "gpucpu" | "gpu+cpu" | "both" | "auto" => Self::GpuCpu,
            _ => return Err(()),
        })
    }
}

/// Which hwmon channel to treat as *the* GPU temperature.
#[derive(Clone, Copy, Debug)]
pub enum SensorPreference {
    /// Hotspot. The correct choice on AMD, where it is what trips throttling.
    Junction,
    /// Package/edge temperature.
    Edge,
    /// Memory junction.
    Memory,
    /// Watch every channel and let the hottest win.
    Max,
}

impl std::fmt::Display for SensorPreference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Junction => f.write_str("junction"),
            Self::Edge => f.write_str("edge"),
            Self::Memory => f.write_str("memory"),
            Self::Max => f.write_str("max"),
        }
    }
}

impl FromStr for SensorPreference {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_lowercase().as_str() {
            "junction" | "hotspot" => Self::Junction,
            "edge" => Self::Edge,
            "memory" | "mem" | "vram" => Self::Memory,
            "max" => Self::Max,
            _ => return Err(()),
        })
    }
}

/// Named fan behaviours.
///
/// Thresholds are calibrated against gfx906, which throttles at 100 C junction
/// and idles around 60 C. All three profiles keep the GPU below its throttle
/// point under sustained load; they differ in how much noise they trade for
/// headroom. Re-tune `gpu_high` if your GPU's limit is not ~100 C.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    /// Stay quiet as long as possible, ramp late and steeply. Accepts hotter
    /// steady-state operation while still avoiding the throttle point.
    Quiet,
    /// The validated default. Linear ramp, comfortable margin.
    Balanced,
    /// Ramp early and aggressively for maximum sustained clocks. Loud.
    Turbo,
    /// Take every value from the config file; do not apply a profile.
    Custom,
}

impl std::fmt::Display for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Quiet => f.write_str("quiet"),
            Self::Balanced => f.write_str("balanced"),
            Self::Turbo => f.write_str("turbo"),
            Self::Custom => f.write_str("custom"),
        }
    }
}

impl FromStr for Profile {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_lowercase().as_str() {
            "quiet" | "silent" => Self::Quiet,
            "balanced" | "default" => Self::Balanced,
            "turbo" | "performance" | "max" => Self::Turbo,
            "custom" => Self::Custom,
            _ => return Err(()),
        })
    }
}

impl Profile {
    /// Baseline values a profile contributes. Any key present in the config
    /// overrides the corresponding field.
    fn defaults(self) -> FanConfig {
        match self {
            // Ramp only once we are within ~5 C of throttling, and use a cubic
            // curve so most of the temperature range stays near idle RPM.
            Self::Quiet => FanConfig {
                gpu_low_temp: 80,
                gpu_high_temp: 96,
                cpu_low_temp: 70,
                cpu_high_temp: 92,
                psu_low_temp: 50,
                psu_high_temp: 75,
                power_low_watts: 350,
                power_high_watts: 1100,
                source: FanSource::GpuCpu,
                speed_curve: SpeedCurve::Exponential,
                always_full_speed: false,
                cover_open_rpm: 0,
                max_rpm: 0,
            },
            // Measured: holds gfx906 at ~90 C peak / 100% top DPM level.
            Self::Balanced | Self::Custom => FanConfig {
                gpu_low_temp: 70,
                gpu_high_temp: 90,
                cpu_low_temp: 60,
                cpu_high_temp: 85,
                // Measured idle on this machine is ~195 W total, so the ramp has
                // to start above that or the PSU fan never sits at minimum.
                psu_low_temp: 45,
                psu_high_temp: 70,
                power_low_watts: 250,
                power_high_watts: 900,
                source: FanSource::GpuCpu,
                speed_curve: SpeedCurve::Linear,
                always_full_speed: false,
                cover_open_rpm: 0,
                max_rpm: 0,
            },
            // Start moving air barely above idle and reach full RPM well before
            // the throttle point. Cube-root curve front-loads the ramp.
            Self::Turbo => FanConfig {
                gpu_low_temp: 62,
                gpu_high_temp: 82,
                cpu_low_temp: 50,
                cpu_high_temp: 75,
                psu_low_temp: 40,
                psu_high_temp: 62,
                power_low_watts: 200,
                power_high_watts: 700,
                source: FanSource::GpuCpu,
                speed_curve: SpeedCurve::Logarithmic,
                always_full_speed: false,
                cover_open_rpm: 0,
                max_rpm: 0,
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FanConfig {
    /// GPU thresholds, applied to whichever channel `sensor_preference` selected.
    pub gpu_low_temp: u8,
    pub gpu_high_temp: u8,
    /// CPU package thresholds. Tracked because manual mode stops the SMC from
    /// ramping these fans for the CPU.
    pub cpu_low_temp: u8,
    pub cpu_high_temp: u8,
    /// PSU thresholds, applied to the hottest `Tp*` SMC channel.
    pub psu_low_temp: u8,
    pub psu_high_temp: u8,
    /// Total-system-power thresholds, in watts, read from SMC `PSTR`.
    ///
    /// Power leads temperature by a long way: a 35 W step showed up in `PSTR`
    /// within 1-2 s while the PSU sensors had not moved at all after 20 s. Using
    /// it as feed-forward gets air moving before the PSU heats up, with the
    /// temperature thresholds above as the backstop.
    pub power_low_watts: u16,
    pub power_high_watts: u16,
    /// What this fan cools.
    pub source: FanSource,
    pub speed_curve: SpeedCurve,
    pub always_full_speed: bool,
    /// Minimum RPM to hold while the cover is off, under [`CoverMode::Floor`].
    ///
    /// 0 means "90% of this fan's maximum", which is where the T2 puts fans 2-4
    /// during its own cover-off ramp (2250 of 2500). The curve may still ask for
    /// more; this is only a floor.
    pub cover_open_rpm: u32,
    /// Override the hardware-reported maximum RPM. 0 means trust the hardware.
    ///
    /// Needed because `fan1_max` on MacPro7,1 reports ~1200 while the PSU fan
    /// demonstrably reaches ~1350 RPM when asked for more, and the SMC itself
    /// drives it to 1522 during a cover-open ramp. Out-of-range requests are
    /// clamped by the SMC rather than rejected, so overshooting is safe.
    pub max_rpm: u32,
}

#[derive(Clone, Debug)]
pub struct GpuConfig {
    /// hwmon `name` values to treat as GPUs.
    pub drivers: Vec<String>,
    pub sensor_preference: SensorPreference,
    /// Extra sensor globs, for any source not covered by hwmon or NVML.
    pub extra_temp_paths: Vec<String>,
    /// Read NVIDIA GPUs through NVML. NVIDIA exposes no hwmon node, so this is
    /// the only way to see them; harmless to leave on with no NVIDIA driver
    /// present, since the library is loaded at runtime.
    pub use_nvml: bool,
}

impl Default for GpuConfig {
    fn default() -> Self {
        Self {
            drivers: ["amdgpu", "radeon", "nouveau", "i915", "xe"]
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            sensor_preference: SensorPreference::Junction,
            extra_temp_paths: Vec::new(),
            use_nvml: true,
        }
    }
}

impl SensorPreference {
    /// Whether this preference implies reading NVML's memory-temperature field
    /// in addition to the GPU core sensor.
    ///
    /// NVML has no hotspot channel, so `junction` and `edge` both resolve to the
    /// core sensor on NVIDIA hardware.
    pub fn wants_memory(self) -> bool {
        matches!(self, Self::Memory | Self::Max)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GeneralConfig {
    pub profile: Profile,
    pub poll_interval: Duration,
    pub fall_samples: usize,
    /// How to handle the cover being off. See [`CoverMode`].
    pub cover_mode: CoverMode,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            profile: Profile::Balanced,
            // Junction has been observed climbing ~9 C in 4 s under a dense
            // compute load, so polling must be brisk.
            poll_interval: Duration::from_secs(1),
            fall_samples: 20,
            cover_mode: CoverMode::Floor,
        }
    }
}

pub struct Config {
    pub general: GeneralConfig,
    pub gpu: GpuConfig,
    pub fans: Vec<FanConfig>,
}

/// Read a key, warning and falling back rather than failing.
///
/// A fan daemon that refuses to start because a config key was added in a newer
/// version is worse than one that starts with a sane default, so nothing here is
/// fatal.
fn get_or<V: FromStr>(props: Option<&ini::Properties>, key: &str, default: V) -> V {
    let Some(raw) = props.and_then(|p| p.get(key)) else {
        return default;
    };

    match raw.trim().parse() {
        Ok(value) => value,
        Err(_) => {
            eprintln!("neofand: ignoring invalid value for '{key}': {raw:?}");
            default
        }
    }
}

fn get_list(props: Option<&ini::Properties>, key: &str, default: Vec<String>) -> Vec<String> {
    let Some(raw) = props.and_then(|p| p.get(key)) else {
        return default;
    };

    let items = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();

    if items.is_empty() {
        default
    } else {
        items
    }
}

fn parse(file_raw: &str, fan_count: NonZeroUsize) -> Result<Config> {
    let file = ini::Ini::load_from_str(file_raw)?;

    let general_section = file.section(Some("General"));
    let defaults = GeneralConfig::default();
    let profile: Profile = get_or(general_section, "profile", defaults.profile);

    let poll_secs: u64 = get_or(
        general_section,
        "poll_interval_secs",
        defaults.poll_interval.as_secs(),
    );
    let fall_samples: usize = get_or(general_section, "fall_samples", defaults.fall_samples);

    let general = GeneralConfig {
        profile,
        poll_interval: Duration::from_secs(poll_secs.max(1)),
        fall_samples: fall_samples.clamp(1, MAX_FALL_SAMPLES),
        // `cover_interlock=false` is honoured as a legacy alias for the old
        // boolean key, so an existing config cannot silently gain the interlock.
        cover_mode: if get_or(general_section, "cover_interlock", true) {
            get_or(general_section, "cover_mode", defaults.cover_mode)
        } else {
            CoverMode::Off
        },
    };

    let gpu_section = file.section(Some("Gpu"));
    let gpu_defaults = GpuConfig::default();
    let gpu = GpuConfig {
        drivers: get_list(gpu_section, "drivers", gpu_defaults.drivers),
        sensor_preference: get_or(
            gpu_section,
            "sensor_preference",
            gpu_defaults.sensor_preference,
        ),
        extra_temp_paths: get_list(gpu_section, "extra_temp_paths", Vec::new()),
        use_nvml: get_or(gpu_section, "use_nvml", gpu_defaults.use_nvml),
    };

    // Profile supplies the baseline; per-fan keys override individual fields.
    let base = profile.defaults();
    let mut fans = Vec::with_capacity(fan_count.get());
    for i in 1..=fan_count.get() {
        let section = file.section(Some(format!("Fan{i}")));
        fans.push(FanConfig {
            gpu_low_temp: get_or(section, "gpu_low_temp", base.gpu_low_temp),
            gpu_high_temp: get_or(section, "gpu_high_temp", base.gpu_high_temp),
            cpu_low_temp: get_or(section, "cpu_low_temp", base.cpu_low_temp),
            cpu_high_temp: get_or(section, "cpu_high_temp", base.cpu_high_temp),
            psu_low_temp: get_or(section, "psu_low_temp", base.psu_low_temp),
            psu_high_temp: get_or(section, "psu_high_temp", base.psu_high_temp),
            power_low_watts: get_or(section, "power_low_watts", base.power_low_watts),
            power_high_watts: get_or(section, "power_high_watts", base.power_high_watts),
            source: get_or(section, "source", base.source),
            speed_curve: get_or(section, "speed_curve", base.speed_curve),
            always_full_speed: get_or(section, "always_full_speed", base.always_full_speed),
            cover_open_rpm: get_or(section, "cover_open_rpm", base.cover_open_rpm),
            max_rpm: get_or(section, "max_rpm", base.max_rpm),
        });
    }

    Ok(Config {
        general,
        gpu,
        fans,
    })
}

fn generate(fan_count: NonZeroUsize) -> Result<Config> {
    let general = GeneralConfig::default();
    let gpu = GpuConfig::default();

    let mut file = ini::Ini::new();
    file.with_section(Some("General"))
        .set("profile", general.profile.to_string())
        .set(
            "poll_interval_secs",
            general.poll_interval.as_secs().to_string(),
        )
        .set("fall_samples", general.fall_samples.to_string())
        .set("cover_mode", general.cover_mode.to_string());

    file.with_section(Some("Gpu"))
        .set("drivers", gpu.drivers.join(","))
        .set("sensor_preference", gpu.sensor_preference.to_string())
        .set("extra_temp_paths", gpu.extra_temp_paths.join(","))
        .set("use_nvml", gpu.use_nvml.to_string());

    // Fan sections are written empty on purpose. Thresholds come from the
    // profile, so writing them here would silently pin the fans to whatever
    // profile happened to be active at first run and make `profile=` inert.
    // Add individual keys to a section to override that fan only.
    for i in 1..=fan_count.get() {
        file.with_section(Some(format!("Fan{i}")))
            .set("always_full_speed", "false");
    }

    file.write_to_file(CONFIG_FILE).map_err(Error::ConfigCreate)?;

    let base = general.profile.defaults();
    Ok(Config {
        general,
        gpu,
        fans: vec![base; fan_count.get()],
    })
}

pub fn load_config(fan_count: NonZeroUsize) -> Result<Config> {
    match std::fs::read_to_string(CONFIG_FILE) {
        Ok(file_raw) => parse(&file_raw, fan_count),
        Err(err) if err.kind() == ErrorKind::NotFound => generate(fan_count),
        Err(err) => Err(Error::ConfigRead(err)),
    }
}
