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
//! previous complete snapshot or the next one — never a partial line. Readers
//! cannot perturb the daemon at all, and cost the daemon one small write per poll.
//!
//! The practical win is `system_power_w`: SMC `PSTR` is otherwise reachable only
//! by scanning to key index 887 of 1363, which cannot be done while the daemon is
//! running. Now it is a `cat` away.

use std::{
    io::Write,
    path::{Path, PathBuf},
};

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
}

pub struct GpuFan {
    pub index: u32,
    pub name: String,
    pub temp_c: u8,
    /// What the curve asked for this poll.
    pub target_pct: u8,
    /// What the card reports right now.
    pub actual_pct: Option<u32>,
    pub power_w: Option<f32>,
    pub ramp_low_c: u8,
    pub ramp_high_c: u8,
}

pub struct Snapshot {
    pub unix_time: u64,
    pub profile: String,
    pub poll_secs: u64,
    pub cover_open: bool,
    pub smc_gpu_throttle: bool,
    pub gpu_c: u8,
    pub cpu_c: u8,
    pub psu_c: u8,
    /// SMC `PSTR`, total system power in watts. 0.0 when the SMC is unavailable.
    pub system_power_w: f32,
    pub chassis_fans: Vec<ChassisFan>,
    pub gpu_fans: Vec<GpuFan>,
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

fn num_or_null<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| "null".to_owned(), |v| v.to_string())
}

impl Snapshot {
    fn to_json(&self) -> String {
        let mut s = String::with_capacity(1024);
        s.push_str("{\n");
        s.push_str(&format!("  \"unix_time\": {},\n", self.unix_time));
        s.push_str(&format!("  \"profile\": \"{}\",\n", esc(&self.profile)));
        s.push_str(&format!("  \"poll_secs\": {},\n", self.poll_secs));
        s.push_str(&format!("  \"cover_open\": {},\n", self.cover_open));
        s.push_str(&format!(
            "  \"smc_gpu_throttle\": {},\n",
            self.smc_gpu_throttle
        ));
        s.push_str("  \"sensors\": {\n");
        s.push_str(&format!("    \"gpu_c\": {},\n", self.gpu_c));
        s.push_str(&format!("    \"cpu_c\": {},\n", self.cpu_c));
        s.push_str(&format!("    \"psu_c\": {},\n", self.psu_c));
        s.push_str(&format!(
            "    \"system_power_w\": {:.1}\n",
            self.system_power_w
        ));
        s.push_str("  },\n");

        s.push_str("  \"chassis_fans\": [\n");
        for (i, f) in self.chassis_fans.iter().enumerate() {
            s.push_str(&format!(
                "    {{\"fan\": {}, \"source\": \"{}\", \"target_rpm\": {}, \"actual_rpm\": {}, \
                 \"min_rpm\": {}, \"max_rpm\": {}}}{}\n",
                f.number,
                esc(&f.source),
                f.target_rpm,
                num_or_null(f.actual_rpm),
                f.min_rpm,
                f.max_rpm,
                if i + 1 == self.chassis_fans.len() { "" } else { "," }
            ));
        }
        s.push_str("  ],\n");

        s.push_str("  \"gpu_fans\": [\n");
        for (i, g) in self.gpu_fans.iter().enumerate() {
            s.push_str(&format!(
                "    {{\"index\": {}, \"name\": \"{}\", \"temp_c\": {}, \"target_pct\": {}, \
                 \"actual_pct\": {}, \"power_w\": {}, \"ramp_low_c\": {}, \"ramp_high_c\": {}}}{}\n",
                g.index,
                esc(&g.name),
                g.temp_c,
                g.target_pct,
                num_or_null(g.actual_pct),
                g.power_w.map_or_else(|| "null".to_owned(), |w| format!("{w:.1}")),
                g.ramp_low_c,
                g.ramp_high_c,
                if i + 1 == self.gpu_fans.len() { "" } else { "," }
            ));
        }
        s.push_str("  ]\n}\n");
        s
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
