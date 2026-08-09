use std::{
    io::{IsTerminal, Write},
    path::PathBuf,
};

use crate::{
    config::{FanConfig, FanSource, GpuFanConfig, SpeedCurve},
    nvidia::FanTarget,
    error::{Error, Result},
};

/// One sampling round, shared by every fan.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Readings {
    /// Hottest GPU channel, °C.
    pub gpu: u8,
    /// CPU package, °C.
    pub cpu: u8,
    /// Hottest PSU (`Tp*`) channel, °C. 0 when unavailable.
    pub psu: u8,
    /// Total system power from SMC `PSTR`, watts. 0.0 when unavailable.
    pub watts: f32,
}

#[derive(Debug)]
pub struct FanController {
    manual_file: std::fs::File,
    output_file: std::fs::File,
    config: FanConfig,
    /// Base sysfs path (`.../fanN`), kept so the status snapshot can read
    /// `fanN_input` back. Target and actual are both worth publishing: on
    /// MacPro7,1 `fan1_max` under-reports, so a fan that cannot reach what it
    /// was asked for is only visible by comparing the two.
    base_path: PathBuf,

    min_speed: u32,
    max_speed: u32,
    /// Floor enforced while the cover is off. See `FanConfig::cover_open_rpm`.
    cover_open_speed: u32,

    /// Last speed actually commanded, for hysteresis.
    ///
    /// `Cell` because the control loop holds the fans immutably and this is the
    /// only mutable state they need. Sound here because the loop is
    /// single-threaded by design (see `smc.rs`) and `FanController` is never
    /// shared across threads.
    last_commanded: std::cell::Cell<Option<u32>>,
}

/// Minimum change before the commanded speed moves at all.
///
/// Kills chatter, but on its own it cannot damp a large oscillation -- see
/// `SLEW_*` below.
const HYSTERESIS_RPM: u32 = 80;

/// Maximum change per poll, rising and falling.
///
/// This is what actually fixes oscillation, and it is asymmetric on purpose.
///
/// Measured on MacPro7,1: GPU temperature wobbles ~1 C between polls under a
/// bursty inference load, and with the logarithmic curve (`cbrt`, which is
/// near-vertical at the bottom of the ramp) that 1 C moved the target ~465 RPM,
/// reversing direction on 91% of samples. A deadband alone does not help --
/// each swing is far larger than any sane deadband. Rate-limiting does: a
/// signal that flips every poll can never traverse the full swing, so the fan
/// settles near the mean instead of chasing the noise.
///
/// Rising is allowed to move faster than falling because the failure modes are
/// not symmetric: being slow to speed up risks the hardware, being slow to slow
/// down only costs noise.
const SLEW_UP_RPM: u32 = 150;
const SLEW_DOWN_RPM: u32 = 60;

/// Fraction of the way from `low` to `high`, clamped to 0.0..=1.0.
fn demand(temp: u8, low: u8, high: u8) -> f32 {
    if high <= low || temp <= low {
        return 0.0;
    }
    if temp >= high {
        return 1.0;
    }

    f32::from(temp - low) / f32::from(high - low)
}

/// As [`demand`], for a watt reading against watt thresholds.
fn demand_watts(watts: f32, low: u16, high: u16) -> f32 {
    let low = f32::from(low);
    let high = f32::from(high);

    if high <= low || watts <= low {
        return 0.0;
    }
    if watts >= high {
        return 1.0;
    }

    (watts - low) / (high - low)
}

/// One signal feeding a fan's curve, with the thresholds it is measured against.
pub struct Input {
    pub name: &'static str,
    pub unit: &'static str,
    pub value: f32,
    pub low: f32,
    pub high: f32,
    pub demand: f32,
}

impl Input {
    fn temp(name: &'static str, value: u8, low: u8, high: u8) -> Self {
        Self {
            name,
            unit: "C",
            value: f32::from(value),
            low: f32::from(low),
            high: f32::from(high),
            demand: demand(value, low, high),
        }
    }
}

/// Why a fan is where it is: its curve, its inputs, and which one won.
pub struct FanExplain {
    pub curve: String,
    pub always_full_speed: bool,
    pub cover_open_rpm: u32,
    /// The input with the highest demand -- the one that actually set the speed.
    pub deciding_input: String,
    pub inputs: Vec<Input>,
}

/// Duty cycle for a GPU's own fan, from that GPU's own temperature.
///
/// The ramp comes from the `target`, not from `config` directly: thresholds are
/// resolved per-card at startup, so a card whose ramp was auto-derived from its
/// own slowdown point keeps that ramp. See `GpuFanConfig::resolve_ramp`.
///
/// Shares [`demand`] and the curve shaping with the chassis fans so the two
/// behave identically; only the output unit differs (percent, not RPM).
pub fn gpu_fan_percent(target: &FanTarget, config: &GpuFanConfig) -> u8 {
    let raw = demand(target.temp, target.low, target.high);
    let shaped = match config.curve {
        SpeedCurve::Linear => raw,
        SpeedCurve::Exponential => raw * raw * raw,
        SpeedCurve::Logarithmic => raw.cbrt(),
    };

    let (min, max) = (config.min_percent, config.max_percent.max(config.min_percent));
    min + (shaped * f32::from(max - min)) as u8
}

impl FanController {
    pub fn new(path: PathBuf, config: FanConfig) -> Result<Self> {
        fn join_suffix(mut path: PathBuf, suffix: &str) -> PathBuf {
            let file_name = path.file_name().unwrap().to_str().unwrap();
            path.set_file_name(format!("{file_name}{suffix}"));
            path
        }

        let min_speed = std::fs::read_to_string(join_suffix(path.clone(), "_min"))
            .map_err(Error::MinSpeedRead)?
            .trim()
            .parse()
            .map_err(Error::MinSpeedParse)?;

        // Read per-fan rather than assuming a shared ceiling: on MacPro7,1 fan1
        // (PSU) reports ~1200 while fan2-4 report 2500.
        let hardware_max: u32 = std::fs::read_to_string(join_suffix(path.clone(), "_max"))
            .map_err(Error::MaxSpeedRead)?
            .trim_end()
            .parse()
            .map_err(Error::MaxSpeedParse)?;

        // A configured ceiling wins over the hardware's. `fan1_max` under-reports
        // (see `FanConfig::max_rpm`) and also drifts a few RPM between reads,
        // which would otherwise make the computed top speed wobble.
        let max_speed = if config.max_rpm > 0 {
            config.max_rpm.max(min_speed)
        } else {
            hardware_max
        };

        let mut open_options = std::fs::OpenOptions::new();
        open_options.write(true).truncate(true);

        let manual_file = open_options
            .open(join_suffix(path.clone(), "_manual"))
            .map_err(Error::FanOpen)?;

        let output_file = open_options
            .open(join_suffix(path.clone(), "_output"))
            .map_err(Error::FanOpen)?;

        // Default to 90% of maximum, which is where the T2 puts fans 2-4 during
        // its own cover-off ramp (2250 of 2500).
        let cover_open_speed = if config.cover_open_rpm > 0 {
            config.cover_open_rpm.clamp(min_speed, max_speed)
        } else {
            (max_speed * 9 / 10).clamp(min_speed, max_speed)
        };

        let this = Self {
            manual_file,
            output_file,
            config,
            base_path: path,
            min_speed,
            max_speed,
            last_commanded: std::cell::Cell::new(None),
            cover_open_speed,
        };

        let thresholds = match config.source {
            FanSource::Gpu => format!("gpu {}-{} C", config.gpu_low_temp, config.gpu_high_temp),
            FanSource::Cpu => format!("cpu {}-{} C", config.cpu_low_temp, config.cpu_high_temp),
            FanSource::Psu => format!(
                "psu {}-{} C, power {}-{} W",
                config.psu_low_temp,
                config.psu_high_temp,
                config.power_low_watts,
                config.power_high_watts
            ),
            FanSource::GpuCpu => format!(
                "gpu {}-{} C, cpu {}-{} C",
                config.gpu_low_temp,
                config.gpu_high_temp,
                config.cpu_low_temp,
                config.cpu_high_temp
            ),
        };

        println!(
            "neofand: fan {}-{} RPM{}, source={}, {}, {}{}, cover-open floor {} RPM",
            this.min_speed,
            this.max_speed,
            if config.max_rpm > 0 {
                format!(" (max overridden from {hardware_max})")
            } else {
                String::new()
            },
            config.source,
            thresholds,
            config.speed_curve,
            if config.always_full_speed {
                ", ALWAYS FULL"
            } else {
                ""
            },
            this.cover_open_speed
        );
        Ok(this)
    }

    /// Current RPM from `fanN_input`, for the status snapshot.
    ///
    /// `None` rather than an error on a failed read: a status field that cannot
    /// be populated must not be able to interrupt fan control.
    pub fn actual_rpm(&self) -> Option<u32> {
        let mut path = self.base_path.clone();
        let name = path.file_name()?.to_str()?.to_owned();
        path.set_file_name(format!("{name}_input"));
        std::fs::read_to_string(path).ok()?.trim().parse().ok()
    }

    pub fn source(&self) -> String {
        self.config.source.to_string()
    }

    /// How this fan's target was arrived at this poll: which sensor(s) its
    /// `source` selects, the thresholds in play, the live value of each, and the
    /// resulting 0.0-1.0 demand.
    ///
    /// Published so a fan sitting at an unexpected speed can be explained
    /// without re-deriving the curve by hand -- the mapping from source to
    /// thresholds is the part that is easy to get wrong when reading a config.
    pub fn explain(&self, readings: &Readings) -> FanExplain {
        let c = &self.config;
        let inputs = match c.source {
            FanSource::Gpu => vec![Input::temp("gpu", readings.gpu, c.gpu_low_temp, c.gpu_high_temp)],
            FanSource::Cpu => vec![Input::temp("cpu", readings.cpu, c.cpu_low_temp, c.cpu_high_temp)],
            FanSource::GpuCpu => vec![
                Input::temp("gpu", readings.gpu, c.gpu_low_temp, c.gpu_high_temp),
                Input::temp("cpu", readings.cpu, c.cpu_low_temp, c.cpu_high_temp),
            ],
            FanSource::Psu => vec![
                Input::temp("psu_temp", readings.psu, c.psu_low_temp, c.psu_high_temp),
                Input {
                    name: "system_power",
                    unit: "W",
                    value: readings.watts,
                    low: f32::from(c.power_low_watts),
                    high: f32::from(c.power_high_watts),
                    demand: demand_watts(readings.watts, c.power_low_watts, c.power_high_watts),
                },
            ],
        };

        // Whichever input asks for most is the one that set the speed. That is
        // the whole point of the multi-signal sources, and it is invisible from
        // the resulting RPM alone.
        let winner = inputs
            .iter()
            .max_by(|a, b| a.demand.total_cmp(&b.demand))
            .map_or("none", |i| i.name);

        FanExplain {
            curve: c.speed_curve.to_string(),
            always_full_speed: c.always_full_speed,
            cover_open_rpm: self.cover_open_speed,
            deciding_input: winner.to_owned(),
            inputs,
        }
    }

    pub fn min_speed(&self) -> u32 {
        self.min_speed
    }

    pub fn max_speed(&self) -> u32 {
        self.max_speed
    }

    pub fn set_manual(&self, enabled: bool) -> Result<()> {
        (&self.manual_file)
            .write_all(if enabled { b"1" } else { b"0" })
            .map_err(Error::FanWrite)
    }

    pub fn set_speed(&self, mut speed: u32) -> Result<()> {
        if speed < self.min_speed {
            speed = self.min_speed;
        } else if speed > self.max_speed {
            speed = self.max_speed;
        }

        {
            let mut stdout = std::io::stdout().lock();
            if stdout.is_terminal() {
                print!("\x1b[1K\rSetting fan speed to {speed}");
                let _ = stdout.flush();
            }
        }

        write!(&self.output_file, "{speed}").map_err(Error::FanWrite)?;
        Ok(())
    }

    /// Demand for this fan, from whichever sensors its `source` selects.
    ///
    /// Each source keeps its own threshold pair rather than sharing one, because
    /// the sane operating points differ wildly: 90 C junction on gfx906 is near
    /// its 100 C limit, while 90 C on the CPU package means something is quite
    /// wrong, and a PSU is unhappy well below either.
    ///
    /// Where a source has two signals the higher demand wins, so the PSU fan
    /// responds to a power step immediately without waiting for the PSU sensors
    /// to catch up, yet still ramps if temperature rises for any other reason.
    /// `cover_open` raises the result to at least this fan's cover-open floor.
    /// The curve is still evaluated and still wins if it asks for more, which is
    /// the whole advantage over handing control back to the SMC.
    pub fn calc_speed(&self, readings: &Readings, cover_open: bool) -> u32 {
        if self.config.always_full_speed {
            return self.max_speed;
        }

        let floor = if cover_open {
            self.cover_open_speed
        } else {
            self.min_speed
        };

        let config = &self.config;
        let gpu = || demand(readings.gpu, config.gpu_low_temp, config.gpu_high_temp);
        let cpu = || demand(readings.cpu, config.cpu_low_temp, config.cpu_high_temp);

        let raw = match config.source {
            FanSource::Gpu => gpu(),
            FanSource::Cpu => cpu(),
            FanSource::GpuCpu => gpu().max(cpu()),
            FanSource::Psu => demand(readings.psu, config.psu_low_temp, config.psu_high_temp).max(
                demand_watts(
                    readings.watts,
                    config.power_low_watts,
                    config.power_high_watts,
                ),
            ),
        };

        // Curves are applied to the normalised demand, so they stay within
        // 0.0..=1.0 for any threshold pair. Upstream applied `log` directly to a
        // temperature delta, which is non-monotonic below 1 and can go negative.
        let shaped = match self.config.speed_curve {
            SpeedCurve::Linear => raw,
            SpeedCurve::Exponential => raw * raw * raw,
            SpeedCurve::Logarithmic => raw.cbrt(),
        };

        let curved = self.min_speed + (shaped * (self.max_speed - self.min_speed) as f32) as u32;
        curved.max(floor)
    }

    /// The last speed this controller commanded, without advancing anything.
    ///
    /// Read-only on purpose: `commanded_speed` is a state machine that steps
    /// once per control tick, so telemetry and the status snapshot must observe
    /// it rather than call it. Calling it from the 10 s telemetry path as well
    /// as the 1 s control path would step the slew limiter twice per cycle and
    /// silently double the ramp rate.
    pub fn last_commanded_rpm(&self) -> Option<u32> {
        self.last_commanded.get()
    }

    /// The speed to actually write, after hysteresis and slew limiting.
    ///
    /// `calc_speed` stays pure so the raw curve target can be published
    /// alongside what was really commanded -- seeing both is how you tell a fan
    /// that is being damped from one that cannot reach its target.
    ///
    /// Safety floors bypass damping entirely: with the cover off there is no
    /// chassis static pressure, and ramping gently towards the floor is not a
    /// tradeoff worth making.
    pub fn commanded_speed(&self, readings: &Readings, cover_open: bool) -> u32 {
        let target = self.calc_speed(readings, cover_open);
        let floor = if cover_open {
            self.cover_open_speed
        } else {
            self.min_speed
        };

        let Some(last) = self.last_commanded.get() else {
            // First poll has nothing to smooth against; take the target as-is
            // rather than ramping up from zero.
            self.last_commanded.set(Some(target));
            return target;
        };

        let next = damp(last, target, floor, self.max_speed);
        self.last_commanded.set(Some(next));
        next
    }
}

/// One step of the damping state machine.
///
/// Free function so it can be tested without a real fan behind it.
fn damp(last: u32, target: u32, floor: u32, ceiling: u32) -> u32 {
    let next = if target > last {
        if target - last < HYSTERESIS_RPM {
            last
        } else {
            last + (target - last).min(SLEW_UP_RPM)
        }
    } else if target < last {
        if last - target < HYSTERESIS_RPM {
            last
        } else {
            last - (last - target).min(SLEW_DOWN_RPM)
        }
    } else {
        last
    };

    // A floor is a safety limit, not a preference: never damp below it.
    next.clamp(floor, ceiling)
}

#[cfg(test)]
mod damp_tests {
    use super::{damp, HYSTERESIS_RPM, SLEW_DOWN_RPM, SLEW_UP_RPM};

    #[test]
    fn small_moves_are_ignored() {
        // The chatter this exists to kill.
        assert_eq!(damp(900, 900 + HYSTERESIS_RPM - 1, 500, 2500), 900);
        assert_eq!(damp(900, 900 - HYSTERESIS_RPM + 1, 500, 2500), 900);
    }

    #[test]
    fn large_moves_are_rate_limited_and_asymmetric() {
        // Rising is allowed to move faster than falling: being slow to speed up
        // risks the hardware, being slow to slow down only costs noise.
        assert_eq!(damp(500, 2500, 500, 2500), 500 + SLEW_UP_RPM);
        assert_eq!(damp(2500, 500, 500, 2500), 2500 - SLEW_DOWN_RPM);
        assert!(SLEW_UP_RPM > SLEW_DOWN_RPM);
    }

    #[test]
    fn a_floor_is_never_damped_through() {
        // Cover opens: the floor jumps to 1350 and must apply at once, not
        // ramp towards it 60 RPM at a time.
        assert_eq!(damp(500, 1350, 1350, 2500), 1350);
    }

    #[test]
    fn oscillation_settles_instead_of_chasing() {
        // The measured failure: target flips 500 <-> 965 every poll. Undamped
        // that is a 465 RPM swing each tick; damped it must converge.
        let mut rpm = 500;
        for i in 0..40 {
            rpm = damp(rpm, if i % 2 == 0 { 965 } else { 500 }, 500, 2500);
        }
        let swing_after = damp(rpm, 965, 500, 2500).abs_diff(damp(rpm, 500, 500, 2500));
        assert!(swing_after <= SLEW_UP_RPM + SLEW_DOWN_RPM,
                "per-tick swing {swing_after} should be bounded by the slew limits");
        assert!(rpm > 500 && rpm < 965, "should settle between the extremes, got {rpm}");
    }
}
