//! Privileged actions: reboot, and choosing what to boot next.
//!
//! Kept deliberately small. Everything here is reachable only through a POST
//! that `web.rs` has already authorised against the Tailscale allowlist, and
//! only when `allow_control=true`.
//!
//! ## Why `--bootnext` and never `--bootorder`
//!
//! `BootNext` is a one-shot override: firmware consumes it on the next boot and
//! clears it. A wrong choice therefore costs exactly one reboot and the machine
//! comes back on its normal path. Rewriting `BootOrder` is persistent, and on
//! this machine getting it wrong means a box that no longer boots the OS you can
//! reach it over Tailscale with -- i.e. it needs a keyboard and a monitor. The
//! risk profiles are not comparable, so the persistent variant is not offered.
//!
//! Note that on this hardware something already rewrites `BootOrder`
//! periodically (a shim/grub package update puts Ubuntu back in front, which is
//! why the rEFInd menu stopped appearing). A one-shot override sidesteps that
//! fight entirely rather than joining it.

use std::process::Command;

pub struct BootEntry {
    pub id: String,
    pub label: String,
    pub active: bool,
    pub current: bool,
    pub next: bool,
}

/// Parse `efibootmgr` output into the entry list plus which is current/next.
pub fn boot_entries() -> Result<Vec<BootEntry>, String> {
    let output = Command::new("efibootmgr")
        .output()
        .map_err(|e| format!("efibootmgr: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    let text = String::from_utf8_lossy(&output.stdout);

    let mut current = String::new();
    let mut next = String::new();
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("BootCurrent:") {
            current = v.trim().to_owned();
        } else if let Some(v) = line.strip_prefix("BootNext:") {
            next = v.trim().to_owned();
        }
    }

    let mut entries = Vec::new();
    for line in text.lines() {
        // `Boot0000* Ubuntu   HD(1,GPT,...)`; the `*` marks an active entry.
        let Some(rest) = line.strip_prefix("Boot") else {
            continue;
        };
        if rest.len() < 4 || !rest[..4].chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        let id = rest[..4].to_owned();
        let remainder = &rest[4..];
        let active = remainder.starts_with('*');
        let label = remainder
            .trim_start_matches('*')
            .trim()
            .split('\t')
            .next()
            .unwrap_or("")
            // The device path follows the label; keep the human part only.
            .split("HD(")
            .next()
            .unwrap_or("")
            .split("PciRoot(")
            .next()
            .unwrap_or("")
            .trim()
            .to_owned();

        entries.push(BootEntry {
            current: id == current,
            next: id == next,
            id,
            label,
            active,
        });
    }
    Ok(entries)
}

/// Set the one-shot boot entry.
///
/// Validates against the real entry list first: `efibootmgr` will happily write
/// `BootNext` for an id that does not exist, which yields a boot that silently
/// falls back and looks like the command did nothing.
pub fn set_boot_next(id: &str) -> Result<String, String> {
    if id.len() != 4 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("{id:?} is not a 4-digit hex boot id"));
    }
    let entries = boot_entries()?;
    let Some(entry) = entries.iter().find(|e| e.id.eq_ignore_ascii_case(id)) else {
        return Err(format!("no boot entry {id}"));
    };

    let output = Command::new("efibootmgr")
        .args(["--bootnext", id])
        .output()
        .map_err(|e| format!("efibootmgr: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    Ok(format!(
        "BootNext set to {} ({}); takes effect on the next reboot and then clears",
        entry.id, entry.label
    ))
}

/// Clear a pending one-shot override.
pub fn clear_boot_next() -> Result<String, String> {
    let output = Command::new("efibootmgr")
        .arg("--delete-bootnext")
        .output()
        .map_err(|e| format!("efibootmgr: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    Ok("BootNext cleared".to_owned())
}

/// Reboot the machine.
///
/// `systemctl reboot` rather than `reboot(2)`: it runs the normal shutdown
/// transaction, so filesystems are unmounted and the daemon's own `ExecStopPost`
/// hands the fans back to the SMC. A direct syscall would skip that and leave
/// the fans wherever this daemon last set them across the reset.
pub fn reboot() -> Result<String, String> {
    let output = Command::new("systemctl")
        .arg("reboot")
        .output()
        .map_err(|e| format!("systemctl: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    Ok("reboot requested".to_owned())
}
