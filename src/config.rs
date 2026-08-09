use std::{io::ErrorKind, num::NonZeroUsize, str::FromStr, time::Duration};

use crate::{Error, Result};

#[cfg(debug_assertions)]
const CONFIG_FILE: &str = "./neofand.conf";
#[cfg(not(debug_assertions))]
const CONFIG_FILE: &str = "/etc/neofand.conf";

/// The config file this build reads, for the status snapshot.
#[must_use]
pub fn config_file_path() -> &'static str {
    CONFIG_FILE
}

/// Upper bound on the fall-off window. The runtime value is clamped to this.
pub const MAX_FALL_SAMPLES: usize = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// How long to keep re-scanning for GPUs after startup, in seconds.
    ///
    /// A GPU driver registers its hwmon node at the end of its probe, which can
    /// land well after this daemon starts: measured 9.1 s late for amdgpu on a
    /// MacPro7,1 (VBIOS fetch from VFCT, then display-core init, then hwmon).
    /// Scanning once at startup therefore misses the card permanently and looks
    /// exactly like a misconfiguration. Re-scanning for a window costs one
    /// directory walk per second and then stops.
    ///
    /// Cards cannot be changed while the machine runs, so this is a startup
    /// settle window rather than continuous polling. 0 disables it.
    pub settle_secs: u64,
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
            settle_secs: 30,
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

// Clone, not Copy: `status_path` is a String. Nothing copies this by value in a
// hot path -- it is read once at startup.
#[derive(Clone, Debug)]
pub struct GeneralConfig {
    pub profile: Profile,
    /// Where to publish the sensor snapshot each poll. Empty disables it.
    ///
    /// A file rather than a socket so that reading it cannot perturb the control
    /// loop -- see `status.rs`. It is the only way to reach SMC `PSTR` while the
    /// daemon is running, since scanning `key_at_index` for it would corrupt the
    /// daemon's own reads.
    pub status_path: String,
    pub poll_interval: Duration,
    pub fall_samples: usize,
    /// How to handle the cover being off. See [`CoverMode`].
    pub cover_mode: CoverMode,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            profile: Profile::Balanced,
            // Alongside /run/neofand.pid; needs no directory of its own.
            status_path: "/run/neofand.status.json".to_owned(),
            // Junction has been observed climbing ~9 C in 4 s under a dense
            // compute load, so polling must be brisk.
            poll_interval: Duration::from_secs(1),
            fall_samples: 20,
            cover_mode: CoverMode::Floor,
        }
    }
}

/// Persist every sensor the daemon reads to a local SQLite database.
///
/// Off by default. The daemon's job is fan control; a store that grows without
/// being asked for is not something a fan daemon should do uninvited.
///
/// ## Why this lives in the daemon rather than a separate collector
///
/// `smc.rs` documents the constraint: `key_at_index` is a single shared index
/// register, so only one process can read raw SMC keys. The daemon already holds
/// that role. An external logger would have to either corrupt the daemon's reads
/// or stop it — which is why the `P*` power zones were unreachable while the
/// daemon ran, until now.
pub struct LoggingConfig {
    pub enabled: bool,
    pub db_path: String,
    /// How often to write a sample. Independent of `poll_interval`: fan control
    /// wants 1 s reactions, but a year of 1 Hz rows across ~360 metrics is tens
    /// of GB for resolution nothing reads back.
    pub interval: Duration,
    pub retention_days: u32,
    /// Minimum change before a reading is written again. Suppresses the large
    /// idle majority of channels that never move.
    ///
    /// Discrete flags (`GTHR`, `MSCO`, `PZ*T`) bypass this entirely and are
    /// always written on change — a throttle trip between two samples is exactly
    /// the event worth having, and a deadband would swallow it.
    pub deadband_temp_c: f64,
    pub deadband_power_w: f64,
    pub deadband_rpm: f64,
    /// Write an unchanged reading anyway after this long, so a flat channel
    /// still has anchor points and charts do not interpolate across hours.
    pub heartbeat_secs: u64,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            db_path: "/var/lib/neofand/telemetry.db".to_owned(),
            interval: Duration::from_secs(10),
            retention_days: 365,
            deadband_temp_c: 0.5,
            deadband_power_w: 1.0,
            deadband_rpm: 20.0,
            heartbeat_secs: 300,
        }
    }
}

/// Dashboard, JSON query API, and the control plane.
pub struct WebConfig {
    pub enabled: bool,
    /// Defaults to loopback. Exposes the machine's full thermal and power
    /// history, so widening it is an explicit choice rather than a default.
    ///
    /// **Bind to a specific Tailscale address, never `0.0.0.0`, when control is
    /// enabled.** Authorization identifies the caller by its tailnet address; if
    /// the listener also accepts LAN packets, a local host can forge a `100.x`
    /// source address and be mistaken for a tailnet peer. Binding to the
    /// tailscale0 address means the packet must have come through the tunnel.
    pub bind: String,
    /// Permit the POST control endpoints (reboot, boot device) at all.
    pub allow_control: bool,
    /// Tailscale login names allowed to use them, e.g. `akhil@example.com`.
    ///
    /// Membership of the tailnet is deliberately *not* sufficient: a tailnet
    /// commonly has several people on it, and rebooting this machine should not
    /// be available to all of them.
    pub control_users: Vec<String>,
    /// systemd units to report health for on the services tab.
    pub services: Vec<String>,
    /// `name=url` HTTP probes, for things that do not run under systemd.
    pub probes: Vec<(String, String)>,
    /// Electricity price per kWh, in whatever currency you think in. 0 disables
    /// the cost readout and leaves the energy chart in watt-hours only.
    pub cost_per_kwh: f64,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind: "127.0.0.1:9977".to_owned(),
            allow_control: false,
            control_users: Vec::new(),
            services: Vec::new(),
            probes: Vec::new(),
            cost_per_kwh: 0.0,
        }
    }
}

pub struct Config {
    pub general: GeneralConfig,
    pub gpu: GpuConfig,
    pub gpu_fans: GpuFanConfig,
    pub fans: Vec<FanConfig>,
    pub logging: LoggingConfig,
    pub web: WebConfig,
}

/// Control of the GPUs' **own** fans, as opposed to the chassis fans.
///
/// Separate from `[FanN]` because the units and the topology both differ: these
/// are per-card duty-cycle percentages, not chassis RPM, and each card follows
/// its own die rather than the hottest one in the box. On MacPro7,1 that
/// distinction is measurable — under a 300 W burn GPU2 sat 4-6 C cooler than
/// GPU0/GPU1 purely from slot airflow, so a shared curve would over-drive one
/// card or under-drive the others.
///
/// Off by default: this takes fan control away from the card firmware, and a
/// daemon that grabs it uninvited on every machine that happens to have an
/// NVIDIA GPU is not a safe default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuFanConfig {
    pub enabled: bool,
    /// Ramp start, °C. `0` means derive it from the card's own slowdown
    /// threshold — see [`AUTO_HIGH_MARGIN`].
    pub low_temp: u8,
    /// Full speed at this temperature, °C. `0` means derive it, as above.
    pub high_temp: u8,
    /// Duty floor. Clamped up to [`MIN_USEFUL_PERCENT`], below which a blower
    /// moves too little air to cool a loaded card.
    pub min_percent: u8,
    pub max_percent: u8,
    pub curve: SpeedCurve,
}

/// Degrees below a card's slowdown threshold at which its fan reaches maximum.
///
/// The fan has to be at full speed *before* the card starts clocking down,
/// otherwise it is reacting to throttling rather than preventing it. Measured on
/// this hardware: the A6000s report a 95 C slowdown and were picking up
/// sw-thermal at 88 C on the stock curve, so the ramp has to finish comfortably
/// under that.
pub const AUTO_HIGH_MARGIN: u8 = 15;

/// Width of the derived ramp, in degrees below `high_temp`.
pub const AUTO_RAMP_WIDTH: u8 = 25;

/// Duty cycle below which a GPU blower is not usefully cooling anything.
pub const MIN_USEFUL_PERCENT: u8 = 30;

/// Fallback ramp for a card whose driver will not report a slowdown threshold.
/// Deliberately conservative: it ramps early rather than trusting an unknown.
pub const FALLBACK_LOW: u8 = 50;
pub const FALLBACK_HIGH: u8 = 75;

impl GpuFanConfig {
    /// Resolve the ramp for one card, given the slowdown threshold NVML reports
    /// for it (`None` when the driver will not say).
    ///
    /// Auto-derivation is per-card on purpose: thresholds vary by model, so a
    /// box with mixed GPUs needs a different curve for each rather than one
    /// tuned to whichever card happened to be measured.
    /// Derive a chassis-fan GPU ramp from the hardware's own throttle point.
    ///
    /// Same margins as [`GpuFanConfig::resolve_ramp`] so a card's own blower and
    /// the chassis fans that feed it agree on when things are getting hot. Kept
    /// separate because the chassis fans take a single reduced temperature for
    /// every GPU at once, so the caller passes the *coolest* limit present.
    ///
    /// Returns `None` when nothing published a usable limit, leaving the caller's
    /// configured thresholds in place rather than inventing a ramp.
    pub fn derive_chassis_ramp(limit: Option<u32>) -> Option<(u8, u8)> {
        let limit = u8::try_from(limit?).ok()?;

        // The full ramp has to fit below the limit without either end
        // saturating. Checking `high > low` afterwards is not enough: a 20 C
        // limit yields high=5, low=0, which passes that test while describing a
        // ramp that is finished before any real GPU has warmed up. Anything this
        // cool is a bogus or misread threshold, not silicon.
        if limit <= AUTO_HIGH_MARGIN + AUTO_RAMP_WIDTH {
            return None;
        }

        let high = limit - AUTO_HIGH_MARGIN;
        let low = high - AUTO_RAMP_WIDTH;
        Some((low, high))
    }

    pub fn resolve_ramp(&self, slowdown: Option<u32>) -> (u8, u8) {
        // An explicit pair in the config always wins; auto only fills in what
        // was left at 0.
        if self.low_temp > 0 && self.high_temp > 0 {
            return (self.low_temp, self.high_temp);
        }

        let high = match slowdown {
            Some(slowdown) => u8::try_from(slowdown)
                .unwrap_or(u8::MAX)
                .saturating_sub(AUTO_HIGH_MARGIN),
            None => FALLBACK_HIGH,
        };
        let low = high.saturating_sub(AUTO_RAMP_WIDTH);

        // A partially-specified pair keeps whichever half was given.
        let low = if self.low_temp > 0 { self.low_temp } else { low };
        let high = if self.high_temp > 0 {
            self.high_temp
        } else {
            high
        };

        // Guard against a config or a threshold that inverts the pair, which
        // would make `demand` return 0 forever and silently disable the ramp.
        if high <= low {
            (FALLBACK_LOW, FALLBACK_HIGH)
        } else {
            (low, high)
        }
    }
}

impl Default for GpuFanConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            // 0 = derive per card from its own slowdown threshold.
            low_temp: 0,
            high_temp: 0,
            min_percent: MIN_USEFUL_PERCENT,
            max_percent: 100,
            curve: SpeedCurve::Linear,
        }
    }
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
        status_path: get_or(general_section, "status_path", defaults.status_path.clone()),
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
        settle_secs: get_or(gpu_section, "settle_secs", gpu_defaults.settle_secs),
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

    let gpu_fan_section = file.section(Some("GpuFans"));
    let gpu_fan_defaults = GpuFanConfig::default();
    let gpu_fans = GpuFanConfig {
        enabled: get_or(gpu_fan_section, "enabled", gpu_fan_defaults.enabled),
        low_temp: get_or(gpu_fan_section, "low_temp", gpu_fan_defaults.low_temp),
        high_temp: get_or(gpu_fan_section, "high_temp", gpu_fan_defaults.high_temp),
        // Clamped rather than rejected, for the same reason as every other key
        // here: a bad value should degrade to something safe, not stop the
        // daemon. The floor is 30 because a slower blower cannot cool the card.
        min_percent: get_or(gpu_fan_section, "min_percent", gpu_fan_defaults.min_percent)
            .clamp(MIN_USEFUL_PERCENT, 100),
        max_percent: get_or(gpu_fan_section, "max_percent", gpu_fan_defaults.max_percent)
            .clamp(MIN_USEFUL_PERCENT, 100),
        curve: get_or(gpu_fan_section, "curve", gpu_fan_defaults.curve),
    };

    let logging_section = file.section(Some("Logging"));
    let logging_defaults = LoggingConfig::default();
    let logging_secs: u64 = get_or(
        logging_section,
        "interval_secs",
        logging_defaults.interval.as_secs(),
    );
    let logging = LoggingConfig {
        enabled: get_or(logging_section, "enabled", logging_defaults.enabled),
        db_path: get_or(logging_section, "db_path", logging_defaults.db_path.clone()),
        interval: Duration::from_secs(logging_secs.max(1)),
        retention_days: get_or(
            logging_section,
            "retention_days",
            logging_defaults.retention_days,
        ),
        deadband_temp_c: get_or(
            logging_section,
            "deadband_temp_c",
            logging_defaults.deadband_temp_c,
        ),
        deadband_power_w: get_or(
            logging_section,
            "deadband_power_w",
            logging_defaults.deadband_power_w,
        ),
        deadband_rpm: get_or(logging_section, "deadband_rpm", logging_defaults.deadband_rpm),
        heartbeat_secs: get_or(
            logging_section,
            "heartbeat_secs",
            logging_defaults.heartbeat_secs,
        ),
    };

    let web_section = file.section(Some("Web"));
    let web_defaults = WebConfig::default();
    let web = WebConfig {
        enabled: get_or(web_section, "enabled", web_defaults.enabled),
        bind: get_or(web_section, "bind", web_defaults.bind.clone()),
        allow_control: get_or(web_section, "allow_control", web_defaults.allow_control),
        control_users: get_list(web_section, "control_users", Vec::new()),
        services: get_list(web_section, "services", Vec::new()),
        // `name=url,name=url`. Entries without an `=` are dropped rather than
        // guessed at, so a typo shows up as a missing probe instead of a probe
        // pointing somewhere unintended.
        probes: get_list(web_section, "probes", Vec::new())
            .into_iter()
            .filter_map(|entry| {
                let (name, url) = entry.split_once('=')?;
                Some((name.trim().to_owned(), url.trim().to_owned()))
            })
            .collect(),
        cost_per_kwh: get_or(web_section, "cost_per_kwh", web_defaults.cost_per_kwh),
    };

    Ok(Config {
        general,
        gpu,
        gpu_fans,
        fans,
        logging,
        web,
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
        .set("cover_mode", general.cover_mode.to_string())
        .set("status_path", general.status_path.clone());

    file.with_section(Some("Gpu"))
        .set("drivers", gpu.drivers.join(","))
        .set("sensor_preference", gpu.sensor_preference.to_string())
        .set("extra_temp_paths", gpu.extra_temp_paths.join(","))
        .set("use_nvml", gpu.use_nvml.to_string())
        .set("settle_secs", gpu.settle_secs.to_string());

    // Written out disabled, with the thresholds visible, so the keys are
    // discoverable without reaching for the README.
    let gpu_fans = GpuFanConfig::default();
    file.with_section(Some("GpuFans"))
        .set("enabled", gpu_fans.enabled.to_string())
        .set("low_temp", gpu_fans.low_temp.to_string())
        .set("high_temp", gpu_fans.high_temp.to_string())
        .set("min_percent", gpu_fans.min_percent.to_string())
        .set("max_percent", gpu_fans.max_percent.to_string())
        .set("curve", gpu_fans.curve.to_string());

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
        gpu_fans,
        fans: vec![base; fan_count.get()],
        logging: LoggingConfig::default(),
        web: WebConfig::default(),
    })
}

pub fn load_config(fan_count: NonZeroUsize) -> Result<Config> {
    match std::fs::read_to_string(CONFIG_FILE) {
        Ok(file_raw) => parse(&file_raw, fan_count),
        Err(err) if err.kind() == ErrorKind::NotFound => generate(fan_count),
        Err(err) => Err(Error::ConfigRead(err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(low: u8, high: u8) -> GpuFanConfig {
        GpuFanConfig {
            low_temp: low,
            high_temp: high,
            ..GpuFanConfig::default()
        }
    }

    #[test]
    fn derives_ramp_from_slowdown() {
        // The A6000s report 95 C, which is what these constants were chosen
        // against: a 55-80 C ramp finishes well before the card clocks down.
        assert_eq!(cfg(0, 0).resolve_ramp(Some(95)), (55, 80));
    }

    #[test]
    fn chassis_ramp_tracks_the_coolest_card() {
        // The RX 460 publishes temp1_crit=94 C and the A6000s report a 95 C
        // slowdown, so a box holding both must ramp for the 94 C card.
        assert_eq!(GpuFanConfig::derive_chassis_ramp(Some(94)), Some((54, 79)));
        assert_eq!(GpuFanConfig::derive_chassis_ramp(Some(95)), Some((55, 80)));
    }

    #[test]
    fn chassis_ramp_declines_rather_than_inventing_one() {
        // No limit published means the caller keeps its configured thresholds.
        assert_eq!(GpuFanConfig::derive_chassis_ramp(None), None);
        // A limit so low the ramp would invert is refused, not clamped into a
        // pair that makes the fan demand 0 forever.
        assert_eq!(GpuFanConfig::derive_chassis_ramp(Some(20)), None);
    }

    #[test]
    fn chassis_and_card_ramps_agree_on_the_same_limit() {
        // The card's own blower and the chassis fans feeding it should not
        // disagree about when the card is getting hot.
        assert_eq!(
            GpuFanConfig::derive_chassis_ramp(Some(95)),
            Some(cfg(0, 0).resolve_ramp(Some(95)))
        );
    }

    #[test]
    fn derives_per_card_rather_than_globally() {
        // The point of deriving at all: two different cards get two different
        // ramps from the same config.
        let config = cfg(0, 0);
        assert_ne!(config.resolve_ramp(Some(95)), config.resolve_ramp(Some(83)));
        assert_eq!(config.resolve_ramp(Some(83)), (43, 68));
    }

    #[test]
    fn explicit_config_wins_over_derivation() {
        assert_eq!(cfg(60, 85).resolve_ramp(Some(95)), (60, 85));
    }

    #[test]
    fn half_specified_pair_derives_only_the_other_half() {
        assert_eq!(cfg(70, 0).resolve_ramp(Some(95)), (70, 80));
        assert_eq!(cfg(0, 90).resolve_ramp(Some(95)), (55, 90));
    }

    #[test]
    fn unknown_slowdown_uses_fallback() {
        assert_eq!(cfg(0, 0).resolve_ramp(None), (FALLBACK_LOW, FALLBACK_HIGH));
    }

    #[test]
    fn inverted_pair_falls_back_instead_of_disabling_the_ramp() {
        // `demand` returns 0.0 forever when high <= low, which would leave the
        // fan pinned at min_percent and look like fan control simply not working.
        assert_eq!(cfg(90, 0).resolve_ramp(Some(95)), (FALLBACK_LOW, FALLBACK_HIGH));
    }

    #[test]
    fn absurdly_low_slowdown_does_not_underflow() {
        // saturating_sub, not `-`: a card claiming a 10 C slowdown must not wrap.
        let (low, high) = cfg(0, 0).resolve_ramp(Some(10));
        assert!(low < high, "ramp must stay ordered, got {low}-{high}");
    }
}
