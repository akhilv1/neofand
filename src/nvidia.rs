//! NVIDIA temperature backend, via NVML.
//!
//! NVIDIA exposes **no hwmon node** — not in the proprietary driver and not in
//! the open kernel modules. It is a long-standing open feature request, and
//! NVML is the vendor's recommended interface. So the hwmon discovery in
//! `gpu.rs` cannot see an NVIDIA GPU at all, and this module exists to cover it.
//!
//! `libnvidia-ml.so` is loaded **at runtime** (nvml-wrapper uses `libloading`),
//! so this binary has no build-time or link-time NVIDIA dependency and runs
//! unchanged on machines with no NVIDIA driver installed — initialisation simply
//! fails with `DriverNotLoaded` and the backend is skipped.
//!
//! ## Sensor semantics differ from AMD
//!
//! NVML's `TemperatureSensor` enum has exactly one variant, `Gpu`. There is no
//! hotspot/junction channel, so `sensor_preference=junction` cannot be honoured
//! literally on NVIDIA hardware — the GPU core sensor is all the driver offers,
//! and it is what the card's own thermal management acts on. Memory temperature
//! is available separately as a field value, but only on GPUs that support it.

#[cfg(feature = "nvidia")]
mod imp {
    use nvml_wrapper::{
        enum_wrappers::device::{TemperatureSensor, TemperatureThreshold},
        enums::device::SampleValue,
        structs::device::FieldId,
        sys_exports::field_id::NVML_FI_DEV_MEMORY_TEMP,
        Nvml,
    };
    use nvml_wrapper_sys::bindings::{nvmlReturn_enum_NVML_SUCCESS, NvmlLib};

    use crate::config::GpuFanConfig;

    /// Where the driver's own `libnvidia-ml.so` lives. Loading it a second time
    /// is safe: NVML reference-counts initialisation, and `dlopen` of an
    /// already-loaded object returns the same object, so this shares state with
    /// the handles the safe wrapper hands out.
    const NVML_LIB: &str = "libnvidia-ml.so.1";

    /// Full per-card detail for the status snapshot.
    pub struct CardStatus {
        pub index: u32,
        pub name: String,
        pub bus_id: String,
        pub temp_c: Option<u8>,
        pub mem_temp_c: Option<u32>,
        pub power_w: Option<f32>,
        pub fan_actual_pct: Option<u32>,
        pub fans: u32,
        pub slowdown_c: Option<u32>,
        pub ramp_low_c: u8,
        pub ramp_high_c: u8,
        /// False for a card with no fan NVML will expose, or when the fan
        /// setters are unavailable. Such a card is still read for temperature.
        pub controllable: bool,
        /// VRAM in use and total, in bytes. Read for telemetry rather than fan
        /// control -- correlating a memory-resident model against the power
        /// zones is the reason to have it.
        pub mem_used_bytes: Option<u64>,
        pub mem_total_bytes: Option<u64>,
        /// Core and memory-controller busy percentages.
        pub util_gpu_pct: Option<u32>,
        pub util_mem_pct: Option<u32>,
    }

    /// A card that can be driven this cycle, with the ramp resolved for it.
    pub struct FanTarget {
        pub index: u32,
        pub temp: u8,
        pub low: u8,
        pub high: u8,
    }

    /// One NVML-visible GPU.
    struct NvDevice {
        index: u32,
        desc: String,
        /// Temperature at which this GPU begins hardware throttling, if the
        /// driver reports it. Used to sanity-check the configured thresholds.
        slowdown: Option<u32>,
        /// Controllable fans on this card. 0 means the card exposes none (fully
        /// passive, or a driver that will not report them), and it is then
        /// skipped for fan control while still being read for temperature.
        fans: u32,
        /// This card's own ramp, resolved once at startup from the config and
        /// this card's slowdown threshold. Per-card rather than global so a box
        /// with mixed GPUs gets a correct curve for each.
        fan_low: u8,
        fan_high: u8,
    }

    pub struct NvidiaBackend {
        nvml: Nvml,
        devices: Vec<NvDevice>,
        read_memory: bool,
        /// Raw bindings, used only for the fan setters. `nvml-wrapper` 0.10
        /// exposes `num_fans` but no way to write a fan speed, so those three
        /// entry points are called directly. `None` when the symbols are absent
        /// (older driver), which downgrades fan control to a no-op rather than
        /// failing the daemon.
        raw: Option<NvmlLib>,
    }

    fn sample_to_temp(value: SampleValue) -> Option<u32> {
        match value {
            SampleValue::U32(v) => Some(v),
            SampleValue::U64(v) => u32::try_from(v).ok(),
            SampleValue::I64(v) => u32::try_from(v).ok(),
            SampleValue::F64(v) if v.is_finite() && v >= 0.0 => Some(v as u32),
            SampleValue::F64(_) => None,
        }
    }

    impl NvidiaBackend {
        /// Returns `None` when no NVIDIA driver is present, which is not an
        /// error — it is the normal case on an AMD-only machine.
        pub fn init(read_memory: bool, gpu_fans: GpuFanConfig) -> Option<Self> {
            let nvml = match Nvml::init() {
                Ok(nvml) => nvml,
                Err(err) => {
                    println!("neofand: NVML unavailable ({err}), skipping NVIDIA GPUs");
                    return None;
                }
            };

            let count = match nvml.device_count() {
                Ok(count) => count,
                Err(err) => {
                    eprintln!("neofand: NVML device_count failed: {err}");
                    return None;
                }
            };

            let mut devices = Vec::new();
            for index in 0..count {
                let Ok(device) = nvml.device_by_index(index) else {
                    eprintln!("neofand: NVML device {index} not accessible");
                    continue;
                };

                // Reject a GPU whose temperature cannot be read now, for the
                // same reason unreadable sysfs sensors are dropped: a sensor
                // that fails mid-loop must not be able to stall fan control.
                if let Err(err) = device.temperature(TemperatureSensor::Gpu) {
                    eprintln!("neofand: NVML device {index} temperature unreadable: {err}");
                    continue;
                }

                let name = device.name().unwrap_or_else(|_| "unknown".to_owned());
                let slowdown = device
                    .temperature_threshold(TemperatureThreshold::Slowdown)
                    .ok();

                let fans = device.num_fans().unwrap_or(0);
                let (fan_low, fan_high) = gpu_fans.resolve_ramp(slowdown);

                let mut desc = format!("nvml [{index}] {name}");
                if let Some(slowdown) = slowdown {
                    desc.push_str(&format!(" (slowdown {slowdown} C)"));
                }
                if fans > 0 {
                    desc.push_str(&format!(
                        " ({fans} fan{}, ramp {fan_low}-{fan_high} C)",
                        if fans == 1 { "" } else { "s" }
                    ));
                }

                devices.push(NvDevice {
                    index,
                    desc,
                    slowdown,
                    fans,
                    fan_low,
                    fan_high,
                });
            }

            if devices.is_empty() {
                println!("neofand: NVML loaded but exposed no usable GPU");
                return None;
            }

            // Safety: loading the driver's own library by soname. Failure is
            // handled, and a `None` here only disables fan control.
            let raw = match unsafe { NvmlLib::new(NVML_LIB) } {
                Ok(lib) => Some(lib),
                Err(err) => {
                    eprintln!("neofand: NVML fan control unavailable ({err}); temperature only");
                    None
                }
            };

            Some(Self {
                nvml,
                devices,
                read_memory,
                raw,
            })
        }

        /// One entry per fan-capable card: its index, current temperature, and
        /// the ramp resolved for it.
        ///
        /// Distinct from [`read_max`]: chassis fans want the hottest card, but a
        /// card's own blower wants that card's own die. Cards in poorer slots run
        /// several degrees hotter, so collapsing these to a maximum would drive
        /// every fan from the worst card in the box.
        pub fn fan_targets(&self) -> Vec<FanTarget> {
            let mut out = Vec::with_capacity(self.devices.len());
            for info in &self.devices {
                if info.fans == 0 {
                    continue;
                }
                let Ok(device) = self.nvml.device_by_index(info.index) else {
                    continue;
                };
                if let Ok(temp) = device.temperature(TemperatureSensor::Gpu) {
                    out.push(FanTarget {
                        index: info.index,
                        temp: u8::try_from(temp).unwrap_or(u8::MAX),
                        low: info.fan_low,
                        high: info.fan_high,
                    });
                }
            }
            out
        }

        /// True when at least one card has a fan this daemon could drive.
        pub fn has_controllable_fans(&self) -> bool {
            self.raw.is_some() && self.devices.iter().any(|d| d.fans > 0)
        }

        /// `(index, °C)` for every NVML card, including ones with no
        /// controllable fan -- those still contribute to the chassis curve, so
        /// omitting them from the breakdown would hide a real heat source.
        pub fn temps_each(&self) -> Vec<(u32, u8)> {
            let mut out = Vec::new();
            for info in &self.devices {
                let Ok(device) = self.nvml.device_by_index(info.index) else {
                    continue;
                };
                if let Ok(temp) = device.temperature(TemperatureSensor::Gpu) {
                    out.push((info.index, u8::try_from(temp).unwrap_or(u8::MAX)));
                }
            }
            out
        }

        /// Everything known about every card, for the status snapshot.
        ///
        /// Includes cards this daemon does *not* drive (`fans == 0`), flagged by
        /// `controllable`, so the snapshot describes the machine rather than
        /// just the subset under control.
        pub fn full_status(&self) -> Vec<CardStatus> {
            let mut out = Vec::new();
            for info in &self.devices {
                let Ok(device) = self.nvml.device_by_index(info.index) else {
                    continue;
                };
                let mem_temp_c = if self.read_memory {
                    device
                        .field_values_for(&[FieldId(NVML_FI_DEV_MEMORY_TEMP)])
                        .ok()
                        .and_then(|s| {
                            s.into_iter()
                                .flatten()
                                .find_map(|v| v.value.ok().and_then(sample_to_temp))
                        })
                } else {
                    None
                };
                // Both are plain NVML reads with no side effects; failures stay
                // `None` rather than dropping the whole card from the status.
                let memory = device.memory_info().ok();
                let utilization = device.utilization_rates().ok();
                out.push(CardStatus {
                    index: info.index,
                    name: device.name().unwrap_or_else(|_| "unknown".to_owned()),
                    bus_id: device
                        .pci_info()
                        .map(|p| p.bus_id)
                        .unwrap_or_else(|_| "unknown".to_owned()),
                    temp_c: device
                        .temperature(TemperatureSensor::Gpu)
                        .ok()
                        .map(|t| u8::try_from(t).unwrap_or(u8::MAX)),
                    mem_temp_c,
                    power_w: device.power_usage().ok().map(|mw| mw as f32 / 1000.0),
                    fan_actual_pct: device.fan_speed(0).ok(),
                    fans: info.fans,
                    slowdown_c: info.slowdown,
                    ramp_low_c: info.fan_low,
                    ramp_high_c: info.fan_high,
                    controllable: info.fans > 0 && self.raw.is_some(),
                    mem_used_bytes: memory.as_ref().map(|m| m.used),
                    mem_total_bytes: memory.as_ref().map(|m| m.total),
                    util_gpu_pct: utilization.as_ref().map(|u| u.gpu),
                    util_mem_pct: utilization.as_ref().map(|u| u.memory),
                });
            }
            out
        }


        /// Pin every fan on one GPU to `percent`.
        ///
        /// A failure on one fan is reported and the rest are still attempted:
        /// leaving some fans pinned and others on the firmware curve because of
        /// an early return would be worse than a partial application.
        pub fn set_fan_percent(&self, index: u32, percent: u8) {
            self.for_each_fan(index, |lib, handle, fan| unsafe {
                lib.nvmlDeviceSetFanSpeed_v2(handle, fan, u32::from(percent))
            });
        }

        /// Hand every fan on every GPU back to the card's own curve.
        ///
        /// Must be called on shutdown. Manual fan settings persist in the driver
        /// after the process exits, so skipping this leaves the blowers pinned
        /// at whatever was last written — the same failure the unit file's
        /// `ExecStopPost` guards against for the chassis fans.
        pub fn restore_fans(&self) {
            for info in &self.devices {
                self.for_each_fan(info.index, |lib, handle, fan| unsafe {
                    lib.nvmlDeviceSetDefaultFanSpeed_v2(handle, fan)
                });
            }
        }

        fn for_each_fan<F>(&self, index: u32, mut apply: F)
        where
            F: FnMut(&NvmlLib, nvml_wrapper_sys::bindings::nvmlDevice_t, u32) -> u32,
        {
            let Some(lib) = &self.raw else { return };
            let Some(info) = self.devices.iter().find(|d| d.index == index) else {
                return;
            };
            if info.fans == 0 {
                return;
            }
            let Ok(device) = self.nvml.device_by_index(index) else {
                return;
            };

            // Safety: the handle is valid for as long as NVML stays initialised,
            // which it does for the lifetime of `self`.
            let handle = unsafe { device.handle() };
            for fan in 0..info.fans {
                let rc = apply(lib, handle, fan);
                if rc != nvmlReturn_enum_NVML_SUCCESS {
                    eprintln!("neofand: NVML fan write failed on GPU{index} fan{fan} (rc {rc})");
                }
            }
        }

        pub fn descriptions(&self) -> Vec<String> {
            self.devices.iter().map(|d| d.desc.clone()).collect()
        }

        pub fn len(&self) -> usize {
            self.devices.len()
        }

        /// Lowest throttle threshold across all NVIDIA GPUs, for calibration
        /// warnings. The profile defaults assume a ~100 C limit; NVIDIA
        /// workstation cards typically throttle well below that.
        pub fn min_slowdown(&self) -> Option<u32> {
            self.devices.iter().filter_map(|d| d.slowdown).min()
        }

        /// Hottest reading across all NVIDIA GPUs, or `None` if every read
        /// failed this cycle.
        ///
        /// A failed read is skipped rather than propagated: a transient NVML
        /// error must not take the daemon down and hand the fans back to
        /// firmware mid-load.
        pub fn read_max(&self) -> Option<u32> {
            let mut max: Option<u32> = None;
            let mut update = |value: u32| {
                max = Some(max.map_or(value, |current: u32| current.max(value)));
            };

            for device_info in &self.devices {
                let Ok(device) = self.nvml.device_by_index(device_info.index) else {
                    continue;
                };

                if let Ok(temp) = device.temperature(TemperatureSensor::Gpu) {
                    update(temp);
                }

                // Memory temperature is unsupported on many GPUs; a failure
                // here is expected and ignored.
                if self.read_memory {
                    if let Ok(samples) =
                        device.field_values_for(&[FieldId(NVML_FI_DEV_MEMORY_TEMP)])
                    {
                        for sample in samples.into_iter().flatten() {
                            if let Some(temp) = sample.value.ok().and_then(sample_to_temp) {
                                update(temp);
                            }
                        }
                    }
                }
            }

            max
        }
    }
}

#[cfg(not(feature = "nvidia"))]
mod imp {
    use crate::config::GpuFanConfig;

    /// Stub used when the `nvidia` feature is disabled, so the call sites in
    /// `main.rs` need no `cfg` handling.
    pub struct NvidiaBackend;

    pub struct FanTarget {
        pub index: u32,
        pub temp: u8,
        pub low: u8,
        pub high: u8,
    }

    pub struct CardStatus {
        pub index: u32,
        pub name: String,
        pub bus_id: String,
        pub temp_c: Option<u8>,
        pub mem_temp_c: Option<u32>,
        pub power_w: Option<f32>,
        pub fan_actual_pct: Option<u32>,
        pub fans: u32,
        pub slowdown_c: Option<u32>,
        pub ramp_low_c: u8,
        pub ramp_high_c: u8,
        pub controllable: bool,
        pub mem_used_bytes: Option<u64>,
        pub mem_total_bytes: Option<u64>,
        pub util_gpu_pct: Option<u32>,
        pub util_mem_pct: Option<u32>,
    }

    impl NvidiaBackend {
        pub fn init(_read_memory: bool, _gpu_fans: GpuFanConfig) -> Option<Self> {
            None
        }
        pub fn temps_each(&self) -> Vec<(u32, u8)> {
            Vec::new()
        }
        pub fn full_status(&self) -> Vec<CardStatus> {
            Vec::new()
        }
        pub fn descriptions(&self) -> Vec<String> {
            Vec::new()
        }
        pub fn len(&self) -> usize {
            0
        }
        pub fn min_slowdown(&self) -> Option<u32> {
            None
        }
        pub fn read_max(&self) -> Option<u32> {
            None
        }
        pub fn fan_targets(&self) -> Vec<FanTarget> {
            Vec::new()
        }
        pub fn has_controllable_fans(&self) -> bool {
            false
        }
        pub fn set_fan_percent(&self, _index: u32, _percent: u8) {}
        pub fn restore_fans(&self) {}
    }
}

pub use imp::{CardStatus, FanTarget, NvidiaBackend};
