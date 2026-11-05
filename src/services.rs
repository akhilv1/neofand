//! Service health for the dashboard: systemd units, HTTP probes, GPU claims.
//!
//! Three mechanisms because the things worth watching here are started three
//! different ways. `neofand` is a unit; a vLLM or llama-server run is often a
//! process in a shell with a port; and "what is actually sitting on GPU2" is
//! answered by neither, because a process can be alive and holding 40 GB of VRAM
//! while its HTTP endpoint is still loading weights.
//!
//! Everything here runs on a web connection thread. Nothing touches the SMC or
//! the control loop, and every probe is bounded by a timeout so a hung endpoint
//! cannot pin a thread indefinitely.

use std::{
    io::{Read, Write},
    net::TcpStream,
    process::Command,
    time::{Duration, Instant},
};

/// Probes are a health check, not a download. Anything slower than this is
/// reported as a failure, which is the useful answer for a dashboard.
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

pub struct UnitHealth {
    pub name: String,
    pub load_state: String,
    pub active_state: String,
    pub sub_state: String,
    pub restarts: String,
    pub since_unix: u64,
}

/// Query systemd for one unit.
///
/// `systemctl show` rather than `is-active`: one spawn gets every field, and it
/// exits 0 even for a unit that does not exist (reporting `LoadState=not-found`),
/// so a missing unit is data rather than an error to special-case.
pub fn unit_health(name: &str) -> UnitHealth {
    let output = Command::new("systemctl")
        .args([
            "show",
            name,
            "--property=LoadState",
            "--property=ActiveState",
            "--property=SubState",
            "--property=NRestarts",
            "--property=ExecMainStartTimestampMonotonic",
            "--property=ActiveEnterTimestamp",
        ])
        .output();

    let mut health = UnitHealth {
        name: name.to_owned(),
        load_state: "unknown".to_owned(),
        active_state: "unknown".to_owned(),
        sub_state: String::new(),
        restarts: "0".to_owned(),
        since_unix: 0,
    };

    let Ok(output) = output else {
        return health;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "LoadState" => health.load_state = value.to_owned(),
            "ActiveState" => health.active_state = value.to_owned(),
            "SubState" => health.sub_state = value.to_owned(),
            "NRestarts" => health.restarts = value.to_owned(),
            "ActiveEnterTimestamp" => health.since_unix = parse_systemd_time(value),
            _ => {}
        }
    }
    health
}

/// systemd prints `Thu 2026-08-09 00:01:54 PDT`. Rather than parse that by hand
/// across locales, ask `date` to do it; an unparseable value becomes 0, which
/// the UI renders as "unknown" rather than a wrong uptime.
fn parse_systemd_time(value: &str) -> u64 {
    if value.trim().is_empty() {
        return 0;
    }
    Command::new("date")
        .args(["-d", value, "+%s"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
        .unwrap_or(0)
}

pub struct ProbeResult {
    pub name: String,
    pub url: String,
    pub ok: bool,
    pub status: u16,
    pub latency_ms: u128,
    pub detail: String,
}

/// Minimal HTTP GET. Only `http://host:port/path` is supported -- these are
/// local inference endpoints, and pulling in a TLS stack for them would be a
/// large dependency for no benefit.
pub fn probe(name: &str, url: &str) -> ProbeResult {
    let started = Instant::now();
    let mut result = ProbeResult {
        name: name.to_owned(),
        url: url.to_owned(),
        ok: false,
        status: 0,
        latency_ms: 0,
        detail: String::new(),
    };

    let Some(rest) = url.strip_prefix("http://") else {
        result.detail = "only http:// URLs are supported".to_owned();
        return result;
    };
    let (authority, path) = rest.split_once('/').map_or((rest, String::from("/")), |(a, p)| {
        (a, format!("/{p}"))
    });

    let Ok(mut addrs) = std::net::ToSocketAddrs::to_socket_addrs(&authority) else {
        result.detail = "cannot resolve".to_owned();
        return result;
    };
    let Some(addr) = addrs.next() else {
        result.detail = "cannot resolve".to_owned();
        return result;
    };

    let stream = TcpStream::connect_timeout(&addr, PROBE_TIMEOUT);
    let Ok(mut stream) = stream else {
        result.detail = "connection refused".to_owned();
        result.latency_ms = started.elapsed().as_millis();
        return result;
    };
    let _ = stream.set_read_timeout(Some(PROBE_TIMEOUT));
    let _ = stream.set_write_timeout(Some(PROBE_TIMEOUT));

    let request =
        format!("GET {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        result.detail = "write failed".to_owned();
        result.latency_ms = started.elapsed().as_millis();
        return result;
    }

    let mut body = Vec::new();
    // Bounded read: a model-list response can be large and none of it is needed
    // beyond the status line.
    let mut buf = [0u8; 4096];
    loop {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                body.extend_from_slice(&buf[..n]);
                if body.len() > 65536 {
                    break;
                }
            }
        }
    }

    result.latency_ms = started.elapsed().as_millis();
    let text = String::from_utf8_lossy(&body);
    if let Some(status) = text
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
    {
        result.status = status;
        result.ok = (200..400).contains(&status);
        if !result.ok {
            result.detail = format!("HTTP {status}");
        }
    } else {
        result.detail = "no HTTP response".to_owned();
    }
    result
}

pub struct GpuClaim {
    pub pid: String,
    pub process: String,
    pub gpu_index: String,
    pub used_mib: String,
}

/// Which processes hold VRAM, per card.
///
/// Uses `nvidia-smi` rather than NVML: the daemon's NVML handle lives on the
/// control thread and is not shared, and a second handle purely to answer a
/// dashboard query is more machinery than a process spawn on an endpoint nobody
/// polls tightly.
pub fn gpu_claims() -> Vec<GpuClaim> {
    let Ok(output) = Command::new("nvidia-smi")
        .args([
            "--query-compute-apps=pid,process_name,gpu_uuid,used_memory",
            "--format=csv,noheader,nounits",
        ])
        .output()
    else {
        return Vec::new();
    };

    // Map UUID to index so the UI can show "GPU2" rather than a GPU-xxxx string.
    let mut uuid_to_index = std::collections::HashMap::new();
    if let Ok(list) = Command::new("nvidia-smi")
        .args(["--query-gpu=index,uuid", "--format=csv,noheader,nounits"])
        .output()
    {
        for line in String::from_utf8_lossy(&list.stdout).lines() {
            if let Some((index, uuid)) = line.split_once(',') {
                uuid_to_index.insert(uuid.trim().to_owned(), index.trim().to_owned());
            }
        }
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(',').map(str::trim).collect();
            if fields.len() < 4 {
                return None;
            }
            Some(GpuClaim {
                pid: fields[0].to_owned(),
                process: fields[1].to_owned(),
                gpu_index: uuid_to_index
                    .get(fields[2])
                    .cloned()
                    .unwrap_or_else(|| fields[2].to_owned()),
                used_mib: fields[3].to_owned(),
            })
        })
        .collect()
}
