//! Raw Apple SMC key access.
//!
//! The patched `applesmc` exposes every SMC key by index through a small set of
//! sysfs files. That is the only way to reach values the driver does not surface
//! as hwmon channels — notably `MSCO` (cover open) and `PSTR` (total system
//! power), both of which this daemon needs.
//!
//! Two hardware quirks drive the design here:
//!
//! * `key_at_index` is a **single shared index register**. Selecting a key and
//!   reading its data are two separate writes/reads, so concurrent readers
//!   interleave and corrupt each other. Everything here runs from the daemon's
//!   single control thread; do not add a second reader. An external
//!   `dump-smc-keys.sh` run while the daemon is live will disturb both.
//! * Key indices are **not stable** across driver versions, so names are
//!   resolved to indices once at startup and cached.

use std::{collections::HashMap, path::PathBuf};

/// Candidate SMC sysfs directories, tried in order.
///
/// On MacPro7,1 the attributes hang off the ACPI node, not a platform device.
/// The bare hwmon node that `applesmc` also registers is a decoy: it has an
/// empty `name` and no sensor attributes at all.
const SMC_DIRS: &[&str] = &[
    "/sys/bus/acpi/devices/APP0001:00",
    "/sys/devices/platform/applesmc.768",
];

/// Cover-open flag. `true` (1) = cover off, `false` (0) = cover fitted.
pub const KEY_COVER_OPEN: &str = "MSCO";
/// Total system power draw, watts.
pub const KEY_SYSTEM_POWER: &str = "PSTR";
/// Non-zero while the SMC is throttling the GPUs.
pub const KEY_GPU_THROTTLE: &str = "GTHR";

pub struct Smc {
    dir: PathBuf,
    keys: HashMap<String, usize>,
}

impl Smc {
    /// Locate the SMC and cache its key-name to index mapping.
    ///
    /// Returns `None` rather than an error: a machine without a readable SMC
    /// should still get a working fan daemon, just without cover detection or
    /// power-driven curves.
    pub fn discover() -> Option<Self> {
        for candidate in SMC_DIRS {
            let dir = PathBuf::from(candidate);
            if !dir.join("key_count").is_file() || !dir.join("key_at_index").is_file() {
                continue;
            }

            let mut this = Self {
                dir,
                keys: HashMap::new(),
            };

            this.index_keys();
            if !this.keys.is_empty() {
                println!(
                    "neofand: smc {} keys via {}",
                    this.keys.len(),
                    this.dir.display()
                );
                return Some(this);
            }
        }

        None
    }

    /// Walk every index once, recording name to index.
    ///
    /// ~1400 iterations of two sysfs operations. Individual failures are skipped
    /// rather than aborting the scan, so one unreadable key cannot cost us the
    /// rest of the map.
    fn index_keys(&mut self) {
        let Some(count) = std::fs::read_to_string(self.dir.join("key_count"))
            .ok()
            .and_then(|raw| raw.trim().parse::<usize>().ok())
        else {
            return;
        };

        for index in 0..count {
            if self.select(index).is_none() {
                continue;
            }

            if let Ok(name) = std::fs::read_to_string(self.dir.join("key_at_index_name")) {
                let name = name.trim();
                if !name.is_empty() {
                    self.keys.insert(name.to_owned(), index);
                }
            }
        }
    }

    fn select(&self, index: usize) -> Option<()> {
        std::fs::write(self.dir.join("key_at_index"), index.to_string()).ok()
    }

    pub fn has(&self, name: &str) -> bool {
        self.keys.contains_key(name)
    }

    fn raw(&self, name: &str) -> Option<Vec<u8>> {
        let index = *self.keys.get(name)?;
        self.select(index)?;
        std::fs::read(self.dir.join("key_at_index_data")).ok()
    }

    /// Read a 1-byte `flag` key.
    pub fn flag(&self, name: &str) -> Option<bool> {
        self.raw(name)?.first().map(|byte| *byte != 0)
    }

    /// Read a 4-byte little-endian `flt` key.
    ///
    /// The bytes are laid out little-endian as stored, so `0000fa43` is
    /// `0x43fa0000` = 500.0.
    pub fn read_f32(&self, name: &str) -> Option<f32> {
        let bytes = self.raw(name)?;
        let quad: [u8; 4] = bytes.get(..4)?.try_into().ok()?;
        let value = f32::from_le_bytes(quad);
        value.is_finite().then_some(value)
    }

    /// `temp*_input` paths whose matching `temp*_label` begins with `prefix`.
    ///
    /// The SMC exposes ~240 labelled temperature channels and their numbering is
    /// not stable across driver versions, so callers select by label prefix
    /// (`Tp` for PSU, `TC` for CPU) rather than by index.
    pub fn temp_paths_with_prefix(&self, prefix: &str) -> Vec<(PathBuf, String)> {
        let pattern = format!("{}/temp*_label", self.dir.display());
        let mut found = glob::glob(&pattern)
            .into_iter()
            .flatten()
            .filter_map(std::result::Result::ok)
            .filter_map(|label_path| {
                let label = std::fs::read_to_string(&label_path).ok()?;
                let label = label.trim().to_owned();
                if !label.starts_with(prefix) {
                    return None;
                }

                // temp12_label -> temp12_input
                let file_name = label_path.file_name()?.to_str()?;
                let stem = file_name.strip_suffix("_label")?;
                let input = label_path.with_file_name(format!("{stem}_input"));
                input.is_file().then_some((input, label))
            })
            .collect::<Vec<_>>();

        found.sort();
        found
    }
}
