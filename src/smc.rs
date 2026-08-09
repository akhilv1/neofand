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
    /// SMC type code per key (`flt`, `ui8`, `spc3`, `flag`, ...), captured during
    /// the startup scan.
    ///
    /// Needed because the power keys are *not* uniformly `flt`: the `PZ*` zone
    /// gauges are, but `PC0P`/`PMLt`/`Pf*` are `spc3` and `PM0f` is `sp5a`.
    /// Decoding those as `flt` yields garbage, which is why `read_f32` alone is
    /// not enough once we log the whole `P*` space rather than just `PSTR`.
    types: HashMap<String, String>,
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
                types: HashMap::new(),
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
                    // The index register is already parked on this key, so the
                    // type costs one extra read and no extra seek. Reading it
                    // lazily later would mean re-selecting, which is the
                    // expensive half.
                    if let Ok(kind) = std::fs::read_to_string(self.dir.join("key_at_index_type")) {
                        // Types are space-padded to four characters (`"flt "`),
                        // so they must be trimmed before comparison.
                        self.types.insert(name.to_owned(), kind.trim().to_owned());
                    }
                }
            }
        }
    }

    fn select(&self, index: usize) -> Option<()> {
        std::fs::write(self.dir.join("key_at_index"), index.to_string()).ok()
    }

    /// Number of SMC keys discovered at startup, for the status snapshot.
    pub fn key_count(&self) -> usize {
        self.keys.len()
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

    /// Every discovered key name starting with `prefix`, sorted.
    ///
    /// Used to log the whole `P*` power space without hardcoding a list that
    /// would go stale on a firmware update.
    pub fn key_names_with_prefix(&self, prefix: &str) -> Vec<String> {
        let mut found: Vec<String> = self
            .keys
            .keys()
            .filter(|name| name.starts_with(prefix))
            .cloned()
            .collect();
        found.sort();
        found
    }

    /// Read any numeric key, decoding by its declared SMC type.
    ///
    /// Apple's fixed-point codes encode their own layout: `spNM` is signed with
    /// `M` (hex) fractional bits, `fpNM` the unsigned equivalent — so `spc3` is
    /// 12 integer + 3 fractional bits and divides by 8. Decoding that family
    /// generically means new keys work without a table.
    ///
    /// Note the endianness split: `flt` is stored little-endian, every integer
    /// and fixed-point type big-endian. That asymmetry is in the hardware, not a
    /// bug here.
    pub fn read_numeric(&self, name: &str) -> Option<f64> {
        let kind = self.types.get(name)?.clone();
        let bytes = self.raw(name)?;
        decode_numeric(&kind, &bytes)
    }

    /// `temp*_input` paths whose matching `temp*_label` begins with `prefix`.
    ///
    /// The SMC exposes ~240 labelled temperature channels and their numbering is
    /// not stable across driver versions, so callers select by label prefix
    /// (`Tp` for PSU, `TC` for CPU) rather than by index. An empty prefix matches
    /// every labelled channel, which is what the telemetry logger wants.
    ///
    /// These are ordinary sysfs attribute reads, not `key_at_index` traffic, so
    /// unlike the raw key interface they are safe to read concurrently.
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

/// Decode SMC bytes for a declared type code. Separate from `Smc` so it is
/// testable without hardware.
fn decode_numeric(kind: &str, bytes: &[u8]) -> Option<f64> {
    if bytes.is_empty() {
        return None;
    }

    let fractional_bits = |code: &str| -> Option<u32> {
        // e.g. "spc3" -> the trailing hex digit is the fraction width.
        code.chars().nth(3)?.to_digit(16)
    };

    let value = match kind {
        "flt" => {
            let quad: [u8; 4] = bytes.get(..4)?.try_into().ok()?;
            f64::from(f32::from_le_bytes(quad))
        }
        "flag" => f64::from(u8::from(bytes[0] != 0)),
        "ui8" | "ui16" | "ui32" | "ui64" => {
            let mut acc: u64 = 0;
            for byte in bytes.iter().take(8) {
                acc = (acc << 8) | u64::from(*byte);
            }
            acc as f64
        }
        "si8" => f64::from(bytes[0] as i8),
        "si16" => {
            let pair: [u8; 2] = bytes.get(..2)?.try_into().ok()?;
            f64::from(i16::from_be_bytes(pair))
        }
        k if k.starts_with("sp") && k.len() == 4 => {
            let pair: [u8; 2] = bytes.get(..2)?.try_into().ok()?;
            let raw = f64::from(i16::from_be_bytes(pair));
            raw / f64::from(1u32 << fractional_bits(k)?)
        }
        k if k.starts_with("fp") && k.len() == 4 => {
            let pair: [u8; 2] = bytes.get(..2)?.try_into().ok()?;
            let raw = f64::from(u16::from_be_bytes(pair));
            raw / f64::from(1u32 << fractional_bits(k)?)
        }
        // `ch8*`, `{lim`, `{fds` and friends are strings or structs, not
        // measurements. Skipping them keeps non-numeric keys out of the store.
        _ => return None,
    };

    value.is_finite().then_some(value)
}

#[cfg(test)]
mod tests {
    use super::decode_numeric;

    #[test]
    fn flt_is_little_endian() {
        // PSTR observed as d7d74f43 = 207.84 W.
        let bytes = [0xd7, 0xd7, 0x4f, 0x43];
        let value = decode_numeric("flt", &bytes).expect("flt decodes");
        assert!((value - 207.843).abs() < 0.01, "got {value}");
    }

    #[test]
    fn fixed_point_uses_its_own_fraction_width() {
        // PC0P observed as spc3 0x0258 = 600/8 = 75.0.
        let value = decode_numeric("spc3", &[0x02, 0x58]).expect("spc3 decodes");
        assert!((value - 75.0).abs() < f64::EPSILON, "got {value}");
        // PM0f observed as sp5a 0x01d5 = 469/1024.
        let value = decode_numeric("sp5a", &[0x01, 0xd5]).expect("sp5a decodes");
        assert!((value - 0.458).abs() < 0.001, "got {value}");
    }

    #[test]
    fn integers_are_big_endian_and_strings_are_skipped() {
        assert_eq!(decode_numeric("ui16", &[0x01, 0x00]), Some(256.0));
        assert_eq!(decode_numeric("flag", &[0x01]), Some(1.0));
        assert_eq!(decode_numeric("ch8*", &[0x41, 0x42]), None);
    }

}
