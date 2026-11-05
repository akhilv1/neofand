//! A snapshot of everything the daemon just read, published for outside readers.
//!
//! ## Why a file and not a socket
//!
//! The control loop is deliberately single-threaded, and `smc.rs` documents why:
//! `key_at_index` is one shared index register, so a second reader corrupts both
//! sides. A socket server would need either another thread touching that state or
//! a non-blocking accept woven through the poll loop.
//!
//! A file sidesteps it entirely. The daemon already reads every sensor each poll;
//! this just serialises what it is holding anyway. The write is a temp-file plus
//! `rename`, which is atomic on the same filesystem, so a reader either sees the
//! previous complete snapshot or the next one — never a partial one. Readers
//! cannot perturb the daemon at all, and cost it one small write per poll.
//!
//! The practical win is `system_power_w`: SMC `PSTR` is otherwise reachable only
//! by scanning to key index 887 of 1363, which cannot be done while the daemon is
//! running. Now it is a `cat` away.
//!
//! ## What is published
//!
//! Enough to explain any observed fan speed without re-deriving the config by
//! hand: every sensor individually rather than only each group's reduced maximum,
//! every card including the ones this daemon does not drive, and for each fan the
//! thresholds its `source` selects plus which input actually won.

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use crate::fan_controller::FanExplain;
use crate::nvidia::CardStatus;

pub struct Daemon {
    pub version: &'static str,
    pub pid: u32,
    pub started_unix: u64,
    pub uptime_secs: u64,
    pub config_file: String,
    pub profile: String,
    pub poll_secs: u64,
    pub fall_samples: usize,
    pub cover_mode: String,
    /// Whether the daemon currently holds `fan*_manual`. False means firmware is
    /// driving the chassis fans -- which is the normal state under
    /// `cover_mode=release` with the cover off, and abnormal otherwise.
    pub holding_manual: bool,
    pub gpu_fan_control: bool,
}

pub struct Smc {
    pub present: bool,
    pub keys: usize,
    pub cover_open: bool,
    pub gpu_throttle: bool,
    /// SMC `PSTR`, total system power in watts. 0.0 when unavailable.
    pub system_power_w: f32,
}

/// One sensor group: the reduced value the curve uses, plus what it came from.
pub struct TempGroup {
    pub name: &'static str,
    /// After the trailing-average fall filter -- this is what the curve sees.
    pub effective_c: u8,
    pub sensors: Vec<(String, u8)>,
}

pub struct ChassisFan {
    pub number: usize,
    pub source: String,
    pub target_rpm: u32,
    /// Read back from `fanN_input`. Diverges from `target_rpm` on purpose --
    /// `fan1_max` under-reports on MacPro7,1, so seeing both is how you catch a
    /// fan that cannot reach what it was asked for.
    pub actual_rpm: Option<u32>,
    pub min_rpm: u32,
    pub max_rpm: u32,
    pub explain: FanExplain,
}

pub struct GpuFan {
    pub index: u32,
    /// What the curve asked for this poll. `None` for a card this daemon does
    /// not drive.
    pub target_pct: Option<u8>,
}

pub struct Snapshot {
    pub unix_time: u64,
    pub daemon: Daemon,
    pub smc: Smc,
    pub groups: Vec<TempGroup>,
    pub chassis_fans: Vec<ChassisFan>,
    pub cards: Vec<CardStatus>,
    pub gpu_targets: Vec<GpuFan>,
}

/// Minimal JSON string escape. Device names come from NVML and are tame, but a
/// stray quote would otherwise produce a file no parser can read.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| "null".to_owned(), |v| v.to_string())
}

fn optf(v: Option<f32>) -> String {
    v.map_or_else(|| "null".to_owned(), |v| format!("{v:.1}"))
}

impl Snapshot {
    #[allow(clippy::too_many_lines)]
    fn to_json(&self) -> String {
        let d = &self.daemon;
        let s = &self.smc;
        let mut o = String::with_capacity(4096);

        o.push_str("{\n");
        o.push_str(&format!("  \"unix_time\": {},\n", self.unix_time));

        o.push_str("  \"daemon\": {\n");
        o.push_str(&format!("    \"version\": \"{}\",\n", esc(d.version)));
        o.push_str(&format!("    \"pid\": {},\n", d.pid));
        o.push_str(&format!("    \"started_unix\": {},\n", d.started_unix));
        o.push_str(&format!("    \"uptime_secs\": {},\n", d.uptime_secs));
        o.push_str(&format!("    \"config_file\": \"{}\",\n", esc(&d.config_file)));
        o.push_str(&format!("    \"profile\": \"{}\",\n", esc(&d.profile)));
        o.push_str(&format!("    \"poll_secs\": {},\n", d.poll_secs));
        o.push_str(&format!("    \"fall_samples\": {},\n", d.fall_samples));
        o.push_str(&format!("    \"cover_mode\": \"{}\",\n", esc(&d.cover_mode)));
        o.push_str(&format!("    \"holding_manual\": {},\n", d.holding_manual));
        o.push_str(&format!("    \"gpu_fan_control\": {}\n", d.gpu_fan_control));
        o.push_str("  },\n");

        o.push_str("  \"smc\": {\n");
        o.push_str(&format!("    \"present\": {},\n", s.present));
        o.push_str(&format!("    \"keys\": {},\n", s.keys));
        o.push_str(&format!("    \"cover_open\": {},\n", s.cover_open));
        o.push_str(&format!("    \"gpu_throttle\": {},\n", s.gpu_throttle));
        o.push_str(&format!("    \"system_power_w\": {:.1}\n", s.system_power_w));
        o.push_str("  },\n");

        o.push_str("  \"temps\": {\n");
        for (i, g) in self.groups.iter().enumerate() {
            o.push_str(&format!("    \"{}\": {{\n", esc(g.name)));
            o.push_str(&format!("      \"effective_c\": {},\n", g.effective_c));
            o.push_str("      \"sensors\": [\n");
            for (j, (desc, t)) in g.sensors.iter().enumerate() {
                o.push_str(&format!(
                    "        {{\"sensor\": \"{}\", \"temp_c\": {}}}{}\n",
                    esc(desc),
                    t,
                    if j + 1 == g.sensors.len() { "" } else { "," }
                ));
            }
            o.push_str("      ]\n");
            o.push_str(&format!(
                "    }}{}\n",
                if i + 1 == self.groups.len() { "" } else { "," }
            ));
        }
        o.push_str("  },\n");

        o.push_str("  \"chassis_fans\": [\n");
        for (i, f) in self.chassis_fans.iter().enumerate() {
            o.push_str("    {\n");
            o.push_str(&format!("      \"fan\": {},\n", f.number));
            o.push_str(&format!("      \"source\": \"{}\",\n", esc(&f.source)));
            o.push_str(&format!("      \"curve\": \"{}\",\n", esc(&f.explain.curve)));
            o.push_str(&format!("      \"target_rpm\": {},\n", f.target_rpm));
            o.push_str(&format!("      \"actual_rpm\": {},\n", opt(f.actual_rpm)));
            o.push_str(&format!("      \"min_rpm\": {},\n", f.min_rpm));
            o.push_str(&format!("      \"max_rpm\": {},\n", f.max_rpm));
            o.push_str(&format!(
                "      \"cover_open_rpm\": {},\n",
                f.explain.cover_open_rpm
            ));
            o.push_str(&format!(
                "      \"always_full_speed\": {},\n",
                f.explain.always_full_speed
            ));
            o.push_str(&format!(
                "      \"deciding_input\": \"{}\",\n",
                esc(&f.explain.deciding_input)
            ));
            o.push_str("      \"inputs\": [\n");
            for (j, inp) in f.explain.inputs.iter().enumerate() {
                o.push_str(&format!(
                    "        {{\"name\": \"{}\", \"unit\": \"{}\", \"value\": {:.1}, \
                     \"low\": {:.1}, \"high\": {:.1}, \"demand\": {:.3}}}{}\n",
                    esc(inp.name),
                    esc(inp.unit),
                    inp.value,
                    inp.low,
                    inp.high,
                    inp.demand,
                    if j + 1 == f.explain.inputs.len() { "" } else { "," }
                ));
            }
            o.push_str("      ]\n");
            o.push_str(&format!(
                "    }}{}\n",
                if i + 1 == self.chassis_fans.len() { "" } else { "," }
            ));
        }
        o.push_str("  ],\n");

        o.push_str("  \"cards\": [\n");
        for (i, c) in self.cards.iter().enumerate() {
            let target = self
                .gpu_targets
                .iter()
                .find(|g| g.index == c.index)
                .and_then(|g| g.target_pct);
            o.push_str(&format!(
                "    {{\"index\": {}, \"name\": \"{}\", \"bus_id\": \"{}\", \"temp_c\": {}, \
                 \"mem_temp_c\": {}, \"power_w\": {}, \"fans\": {}, \"controllable\": {}, \
                 \"fan_target_pct\": {}, \"fan_actual_pct\": {}, \"ramp_low_c\": {}, \
                 \"ramp_high_c\": {}, \"slowdown_c\": {}}}{}\n",
                c.index,
                esc(&c.name),
                esc(&c.bus_id),
                opt(c.temp_c),
                opt(c.mem_temp_c),
                optf(c.power_w),
                c.fans,
                c.controllable,
                opt(target),
                opt(c.fan_actual_pct),
                c.ramp_low_c,
                c.ramp_high_c,
                opt(c.slowdown_c),
                if i + 1 == self.cards.len() { "" } else { "," }
            ));
        }
        o.push_str("  ]\n}\n");
        o
    }

    /// Publish atomically: write a sibling temp file, then `rename` over the
    /// target. A reader never observes a half-written snapshot.
    ///
    /// Failures are returned but the caller ignores them on purpose -- a status
    /// file that cannot be written must never take fan control down.
    pub fn publish(&self, path: &Path) -> std::io::Result<()> {
        let mut tmp: PathBuf = path.to_path_buf();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "neofand.status.json".to_owned());
        tmp.set_file_name(format!(".{name}.tmp"));

        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(self.to_json().as_bytes())?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)
    }
}

/// Render the published snapshot for `neofand --status`.
///
/// Deliberately reads the file rather than talking to the daemon: the whole point
/// is that querying cannot disturb it.
pub fn print(path: &Path) -> std::io::Result<()> {
    let raw = std::fs::read_to_string(path)?;
    print!("{raw}");
    Ok(())
}
