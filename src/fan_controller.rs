use std::{
    io::{IsTerminal, Write},
    path::PathBuf,
};

use crate::{
    config::{FanConfig, FanSource, SpeedCurve},
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

    min_speed: u32,
    max_speed: u32,
    /// Floor enforced while the cover is off. See `FanConfig::cover_open_rpm`.
    cover_open_speed: u32,
}

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
            .open(join_suffix(path, "_output"))
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
            min_speed,
            max_speed,
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
}
