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

    /// One NVML-visible GPU.
    struct NvDevice {
        index: u32,
        desc: String,
        /// Temperature at which this GPU begins hardware throttling, if the
        /// driver reports it. Used to sanity-check the configured thresholds.
        slowdown: Option<u32>,
    }

    pub struct NvidiaBackend {
        nvml: Nvml,
        devices: Vec<NvDevice>,
        read_memory: bool,
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
        pub fn init(read_memory: bool) -> Option<Self> {
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

                let mut desc = format!("nvml [{index}] {name}");
                if let Some(slowdown) = slowdown {
                    desc.push_str(&format!(" (slowdown {slowdown} C)"));
                }

                devices.push(NvDevice {
                    index,
                    desc,
                    slowdown,
                });
            }

            if devices.is_empty() {
                println!("neofand: NVML loaded but exposed no usable GPU");
                return None;
            }

            Some(Self {
                nvml,
                devices,
                read_memory,
            })
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
    /// Stub used when the `nvidia` feature is disabled, so the call sites in
    /// `main.rs` need no `cfg` handling.
    pub struct NvidiaBackend;

    impl NvidiaBackend {
        pub fn init(_read_memory: bool) -> Option<Self> {
            None
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
    }
}

pub use imp::NvidiaBackend;
