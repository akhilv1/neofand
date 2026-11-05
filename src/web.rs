//! Dashboard and JSON query API.
//!
//! A hand-rolled HTTP/1.1 server on `std::net::TcpListener`. That is a
//! deliberate choice over pulling in an async runtime: the whole surface is
//! three read-only endpoints and a static page, and `axum` + `tokio` would be
//! more dependency than the rest of the daemon combined.
//!
//! ## Isolation
//!
//! This thread never touches the SMC and never writes to the database. It opens
//! its own **read-only** SQLite connection; WAL mode means those reads proceed
//! concurrently with the writer thread without either blocking the other, and
//! neither can reach the control loop. A wedged HTTP client therefore cannot
//! affect fan control, which is the property that made a socket unacceptable for
//! `status.rs` and acceptable here.
//!
//! ## Endpoints
//!
//! * `GET /` — the dashboard, a single self-contained page.
//! * `GET /api/metrics` — every channel, with its subsystem, unit and row count.
//! * `GET /api/series?subsystem=&name=&from=&to=&step=` — one series,
//!   bucket-averaged to `step` seconds. `name` accepts a comma-separated list.
//! * `GET /api/latest` — the most recent value of every channel.
//! * `GET /api/services` — systemd units, HTTP probes, GPU VRAM claims.
//! * `GET /api/events` — throttle trips and flag transitions.
//! * `GET /api/energy?from=&to=&bucket=` — watt-hours per bucket.
//! * `GET /api/boot` — EFI boot entries, and who the caller is.
//! * `POST /api/control/{reboot,bootnext,bootnext-clear}` — privileged.
//!
//! ## Authorisation
//!
//! Read endpoints are open to anything that can reach the socket; the bind
//! address is the access control. Control endpoints additionally require the
//! caller's Tailscale identity to be in `control_users` (see `tsauth.rs`), and
//! are refused outright on a wildcard bind, because address-derived identity is
//! only sound when the packet had to arrive through the tunnel.
//!
//! Control requests also require `X-Neofand-Control: 1`. Browser requests must
//! use the native listener authority and matching HTTP Origin; alternate proxy
//! hostnames are not control origins. CLI example (on an allowed tailnet peer):
//! `curl -H 'X-Neofand-Control: 1' -X POST http://TAILSCALE_IP:9977/api/control/reboot`.
//! Omitting Origin is allowed for CLI clients but never bypasses identity checks.

use std::{
    io::{self, BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{atomic::{AtomicUsize, Ordering}, Arc},
    time::{Duration, Instant},
};

use rusqlite::{Connection, OpenFlags};

use crate::config::WebConfig;
use crate::{control, services, tsauth};

/// The dashboard, compiled into the binary.
///
/// It lives in `web/` rather than `src/` because it is a separate concern with a
/// different language and different reviewers — but it stays in *this repo*
/// deliberately. `include_str!` resolves at compile time, so a submodule would
/// make a plain `git clone` produce a tree that does not build and would break
/// `cargo install --git` outright. It is also coupled to this file's JSON shape
/// (the `PZ*G`/`PZ*E` pairing, the `whoami` contract, the events schema), so the
/// two always ship together. Split it out only if it grows a build step or a
/// second frontend appears; the reusable surface is `/api/*`, not this file.
const DASHBOARD: &str = include_str!("../web/dashboard.html");

/// Refuse absurd ranges rather than trying to serialise a year at 1 s buckets.
const MAX_POINTS: i64 = 5000;
const MAX_TOTAL_POINTS: i64 = 64_000;
const MAX_CONNECTIONS: usize = 16;
const MAX_LINE: usize = 8192;
const MAX_HEADERS: usize = 16384;
const MAX_BODY: usize = 8192;
const MAX_SERIES: usize = 64;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const QUERY_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_RANGE: i64 = 366 * 86400;

struct ConnectionPermit(Arc<AtomicUsize>);
impl ConnectionPermit {
    fn acquire(active: &Arc<AtomicUsize>) -> Option<Self> {
        active.fetch_update(Ordering::AcqRel, Ordering::Relaxed, |n|
            (n < MAX_CONNECTIONS).then_some(n + 1)).ok()?;
        Some(Self(Arc::clone(active)))
    }
}
impl Drop for ConnectionPermit {
    fn drop(&mut self) { self.0.fetch_sub(1, Ordering::AcqRel); }
}

// A socket inactivity timeout alone does not stop a client sending one byte
// just before each timeout. Recompute the remaining total budget on every read.
struct DeadlineReader<'a> { stream: &'a TcpStream, deadline: Instant }
impl Read for DeadlineReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let remaining = self.deadline.checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "request deadline"))?;
        self.stream.set_read_timeout(Some(remaining))?;
        let mut stream = self.stream;
        stream.read(buf)
    }
}

fn limited_line(reader: &mut impl BufRead, limit: usize) -> io::Result<String> {
    let mut line = String::new();
    reader.take(limit as u64 + 1).read_line(&mut line)?;
    if line.len() > limit || !line.ends_with('\n') {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "HTTP line too long or incomplete"));
    }
    Ok(line)
}

#[derive(Default)]
struct Request {
    method: String, target: String, content_length: usize,
    host: Option<String>, origin: Option<String>, control_header: bool,
}

fn parse_request(reader: &mut impl BufRead) -> io::Result<Request> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "invalid HTTP request");
    let line = limited_line(reader, MAX_LINE)?;
    let parts: Vec<_> = line.split_whitespace().collect();
    if parts.len() != 3 || !matches!(parts[2], "HTTP/1.0" | "HTTP/1.1") || !parts[1].starts_with('/') {
        return Err(invalid());
    }
    let mut request = Request { method: parts[0].into(), target: parts[1].into(), ..Request::default() };
    let mut bytes = 0; let mut length_seen = false; let mut control_seen = false;
    loop {
        let line = limited_line(reader, MAX_LINE.min(MAX_HEADERS - bytes))?;
        bytes += line.len();
        if line == "\r\n" || line == "\n" { break; }
        if bytes >= MAX_HEADERS || line.starts_with([' ', '\t']) { return Err(invalid()); }
        let (name, value) = line.trim_end_matches(['\r', '\n']).split_once(':').ok_or_else(invalid)?;
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            if length_seen { return Err(invalid()); }
            length_seen = true;
            request.content_length = value.parse().map_err(|_| invalid())?;
            if request.content_length > MAX_BODY { return Err(invalid()); }
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(invalid());
        } else if name.eq_ignore_ascii_case("host") {
            if request.host.replace(value.into()).is_some() { return Err(invalid()); }
        } else if name.eq_ignore_ascii_case("origin") {
            if request.origin.replace(value.into()).is_some() { return Err(invalid()); }
        } else if name.eq_ignore_ascii_case("x-neofand-control") {
            if control_seen { return Err(invalid()); }
            control_seen = true; request.control_header = value == "1";
        }
    }
    Ok(request)
}

// Browser forms cannot set this header. Cross-origin fetch needs a preflight,
// which this server rejects. Host checks also close DNS-rebinding origins.
fn safe_control_request(request: &Request, local: &str, configured: &str) -> bool {
    let Some(host) = request.host.as_deref() else { return false; };
    if !request.control_header || !(host.eq_ignore_ascii_case(local) || host.eq_ignore_ascii_case(configured)) {
        return false;
    }
    request.origin.as_deref().map_or(true, |origin|
        origin.strip_prefix("http://").is_some_and(|authority| authority.eq_ignore_ascii_case(host)))
}

fn query_range(from: i64, to: i64, now: i64) -> Result<(i64, i64), String> {
    let from = from.max((now - MAX_RANGE).max(0));
    let to = to.min(now);
    if from > to { return Err("invalid time range".into()); }
    Ok((from, to))
}


/// Everything a connection thread needs, shared immutably.
pub struct Server {
    pub db_path: String,
    pub bind: String,
    pub cost_per_kwh: f64,
    pub allow_control: bool,
    pub control_users: Vec<String>,
    pub services: Vec<String>,
    pub probes: Vec<(String, String)>,
}

/// A wildcard bind cannot support address-derived identity: a LAN host could
/// forge a `100.x` source address and be attributed to a tailnet peer.
fn is_wildcard_bind(bind: std::net::SocketAddr) -> bool {
    bind.ip().is_unspecified()
}

pub fn spawn(config: &WebConfig, db_path: &str) -> Result<(), String> {
    let listener =
        TcpListener::bind(&config.bind).map_err(|e| format!("cannot bind {}: {e}", config.bind))?;

    // Refuse rather than silently serving control on an address where the
    // identity check is unsound. Downgrading to read-only is the safe failure.
    let allow_control = if config.allow_control && is_wildcard_bind(listener.local_addr().map_err(|e| e.to_string())?) {
        eprintln!(
            "neofand: WARNING control endpoints DISABLED — bind={} is a wildcard address, so a \
             Tailscale peer address cannot be trusted. Bind to the tailscale0 address instead.",
            config.bind
        );
        false
    } else {
        config.allow_control
    };

    if allow_control && config.control_users.is_empty() {
        eprintln!(
            "neofand: WARNING allow_control=true but control_users is empty — every control \
             request will be refused."
        );
    }

    let server = Arc::new(Server {
        db_path: db_path.to_owned(),
        bind: config.bind.clone(),
        cost_per_kwh: config.cost_per_kwh,
        allow_control,
        control_users: config.control_users.clone(),
        services: config.services.clone(),
        probes: config.probes.clone(),
    });

    std::thread::Builder::new()
        .name("neofand-web".to_owned())
        .spawn(move || {
            let active = Arc::new(AtomicUsize::new(0));
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let Some(permit) = ConnectionPermit::acquire(&active) else { continue; };
                if stream.set_write_timeout(Some(REQUEST_TIMEOUT)).is_err() { continue; }
                let server = Arc::clone(&server);
                let _ = std::thread::Builder::new()
                    .name("neofand-web-conn".to_owned())
                    .spawn(move || { let _permit = permit; handle(&stream, &server); });
            }
        })
        .map_err(|e| format!("cannot start web thread: {e}"))?;

    Ok(())
}

fn handle(stream: &TcpStream, server: &Server) {
    let mut reader = BufReader::new(DeadlineReader { stream, deadline: Instant::now() + REQUEST_TIMEOUT });
    let request = match parse_request(&mut reader) {
        Ok(request) => request,
        Err(_) => { respond_status(stream, "400 Bad Request", "invalid or timed-out HTTP request"); return; }
    };
    let method = request.method.as_str();
    let target = request.target.as_str();
    let (path, query) = target.split_once('?').unwrap_or((target, ""));

    if method == "GET" {
        match path {
            "/" | "/index.html" => respond(
                stream,
                "200 OK",
                "text/html; charset=utf-8",
                DASHBOARD.as_bytes(),
            ),
            "/api/metrics" => reply(stream, api_metrics(&server.db_path)),
            "/api/latest" => reply(stream, api_latest(&server.db_path)),
            "/api/series" => reply(stream, api_series(&server.db_path, query)),
            "/api/events" => reply(stream, api_events(&server.db_path, query)),
            "/api/energy" => reply(stream, api_energy(&server.db_path, query, server.cost_per_kwh)),
            "/api/services" => reply(stream, Ok(api_services(server))),
            "/api/boot" => reply(stream, api_boot(stream, server)),
            "/api/whoami" => reply(stream, Ok(api_whoami(stream, server))),
            _ => respond(stream, "404 Not Found", "text/plain", b"not found"),
        }
        return;
    }

    if method != "POST" {
        respond(
            stream,
            "405 Method Not Allowed",
            "text/plain",
            b"GET or POST",
        );
        return;
    }

    if !path.starts_with("/api/control/") {
        respond(stream, "404 Not Found", "text/plain", b"not found");
        return;
    }

    if !server.allow_control {
        respond(
            stream,
            "403 Forbidden",
            "application/json",
            br#"{"error":"control endpoints are disabled (allow_control=false)"}"#,
        );
        return;
    }

    let local = stream.local_addr().map(|a| a.to_string()).unwrap_or_default();
    if !safe_control_request(&request, &local, &server.bind) {
        respond_status(stream, "403 Forbidden", "control requires X-Neofand-Control: 1 and the native listener origin");
        return;
    }
    let mut buf = vec![0u8; request.content_length];
    if reader.read_exact(&mut buf).is_err() {
        respond_status(stream, "400 Bad Request", "incomplete control request"); return;
    }
    let body = String::from_utf8_lossy(&buf);

    // Identify the caller before doing anything. Any failure is a refusal:
    // an unidentifiable caller is not a permitted one.
    let Ok(peer) = stream.peer_addr() else {
        respond_status(stream, "403 Forbidden", "cannot determine peer address");
        return;
    };
    let identity = match tsauth::authorized(peer, &server.control_users) {
        Ok(identity) => identity,
        Err(e) => {
            eprintln!("neofand: control request from {peer} refused — {e}");
            respond_status(stream, "403 Forbidden", &e);
            return;
        }
    };

    // Log every accepted privileged action with who asked. This is the audit
    // trail; a reboot with no record of who caused it is not acceptable.
    eprintln!(
        "neofand: control {} by {} ({})",
        path, identity.login, identity.node
    );

    let outcome = match path {
        "/api/control/reboot" => control::reboot(),
        "/api/control/bootnext" => match param(&body, "id").or_else(|| param(query, "id")) {
            Some(id) => control::set_boot_next(&id),
            None => Err("missing id".to_owned()),
        },
        "/api/control/bootnext-clear" => control::clear_boot_next(),
        _ => {
            respond(stream, "404 Not Found", "text/plain", b"not found");
            return;
        }
    };

    match outcome {
        Ok(message) => {
            let body = format!("{{\"ok\":true,\"message\":\"{}\"}}", esc(&message));
            respond_json(stream, &body);
        }
        Err(e) => {
            eprintln!("neofand: control {path} failed — {e}");
            respond_status(stream, "500 Internal Server Error", &e);
        }
    }
}

/// Send a handler result as JSON, or its error as a 500.
fn reply(stream: &TcpStream, result: Result<String, String>) {
    match result {
        Ok(body) => respond_json(stream, &body),
        Err(e) => respond_error(stream, &e),
    }
}

fn respond_status(stream: &TcpStream, status: &str, message: &str) {
    let body = format!("{{\"ok\":false,\"error\":\"{}\"}}", esc(message));
    respond(stream, status, "application/json", body.as_bytes());
}

fn api_whoami(stream: &TcpStream, server: &Server) -> String {
    let peer = stream.peer_addr().ok();
    let identity = peer.and_then(tsauth::whois);
    let login = identity.as_ref().map_or("", |i| i.login.as_str());
    let permitted = server.allow_control
        && server
            .control_users
            .iter()
            .any(|a| a.eq_ignore_ascii_case(login));
    format!(
        "{{\"peer\":\"{}\",\"login\":{},\"node\":{},\"control_enabled\":{},\"permitted\":{}}}",
        peer.map_or_else(|| "unknown".to_owned(), |p| p.to_string()),
        identity
            .as_ref()
            .map_or_else(|| "null".to_owned(), |i| format!("\"{}\"", esc(&i.login))),
        identity
            .as_ref()
            .map_or_else(|| "null".to_owned(), |i| format!("\"{}\"", esc(&i.node))),
        server.allow_control,
        permitted
    )
}

fn api_boot(stream: &TcpStream, server: &Server) -> Result<String, String> {
    let entries = control::boot_entries()?;
    let mut out = String::from("{\"entries\":[");
    for (i, e) in entries.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"id\":\"{}\",\"label\":\"{}\",\"active\":{},\"current\":{},\"next\":{}}}",
            esc(&e.id),
            esc(&e.label),
            e.active,
            e.current,
            e.next
        ));
    }
    out.push_str("],\"whoami\":");
    out.push_str(&api_whoami(stream, server));
    out.push('}');
    Ok(out)
}

fn api_services(server: &Server) -> String {
    let mut out = String::from("{\"units\":[");
    for (i, name) in server.services.iter().enumerate() {
        let h = services::unit_health(name);
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"name\":\"{}\",\"load\":\"{}\",\"active\":\"{}\",\"sub\":\"{}\",             \"restarts\":\"{}\",\"since\":{}}}",
            esc(&h.name),
            esc(&h.load_state),
            esc(&h.active_state),
            esc(&h.sub_state),
            esc(&h.restarts),
            h.since_unix
        ));
    }
    out.push_str("],\"probes\":[");
    for (i, (name, url)) in server.probes.iter().enumerate() {
        let r = services::probe(name, url);
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"name\":\"{}\",\"url\":\"{}\",\"ok\":{},\"status\":{},             \"latency_ms\":{},\"detail\":\"{}\"}}",
            esc(&r.name),
            esc(&r.url),
            r.ok,
            r.status,
            r.latency_ms,
            esc(&r.detail)
        ));
    }
    out.push_str("],\"gpu_claims\":[");
    for (i, c) in services::gpu_claims().iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"pid\":\"{}\",\"process\":\"{}\",\"gpu\":\"{}\",\"used_mib\":\"{}\"}}",
            esc(&c.pid),
            esc(&c.process),
            esc(&c.gpu_index),
            esc(&c.used_mib)
        ));
    }
    out.push_str("]}");
    out
}

/// Throttle trips and flag transitions.
///
/// Discrete channels are stored with a zero deadband, so every row here is
/// either a genuine change or a heartbeat -- which is exactly why a 10 s logging
/// cadence does not lose a short trip.
fn api_events(db_path: &str, query: &str) -> Result<String, String> {
    let hours: i64 = param(query, "hours")
        .and_then(|v| v.parse().ok())
        .unwrap_or(24)
        .clamp(1, 24 * 365);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let from = now - hours * 3600;

    let connection = open(db_path)?;
    let mut statement = connection
        .prepare(
            "SELECT m.subsystem, m.name, s.ts, s.value
             FROM sample s JOIN metric m ON m.id = s.metric_id
             WHERE s.ts >= ?1
               AND (m.subsystem = 'smc_flag'
                    OR (m.subsystem = 'smc_power' AND m.name LIKE 'PZ_T'))
             ORDER BY s.ts DESC LIMIT 1000",
        )
        .map_err(|e| e.to_string())?;

    let rows = statement
        .query_map([from], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, f64>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut out = String::from("{\"events\":[");
    let mut first = true;
    let mut previous: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for row in rows {
        let (subsystem, name, ts, value) = row.map_err(|e| e.to_string())?;
        // Rows arrive newest-first, so "previous" here is the newer neighbour;
        // a differing value still marks a boundary either way.
        let changed = previous.insert(name.clone(), value).map_or(true, |p| {
            (p - value).abs() > f64::EPSILON
        });
        if !changed {
            continue;
        }
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str(&format!(
            "{{\"subsystem\":\"{}\",\"name\":\"{}\",\"ts\":{},\"value\":{},\"state\":\"{}\"}}",
            esc(&subsystem),
            esc(&name),
            ts,
            fmt_f64(value),
            if value > 0.0 { "asserted" } else { "cleared" }
        ));
    }
    out.push_str("]}");
    Ok(out)
}

fn open(db_path: &str) -> Result<Connection, String> {
    let connection = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("cannot open database: {e}"))?;
    connection.busy_timeout(Duration::from_millis(250)).map_err(|e| e.to_string())?;
    install_query_deadline(&connection, QUERY_TIMEOUT);
    Ok(connection)
}

fn install_query_deadline(connection: &Connection, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    connection.progress_handler(10_000, Some(move || Instant::now() >= deadline));
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn api_metrics(db_path: &str) -> Result<String, String> {
    let connection = open(db_path)?;
    let mut statement = connection
        .prepare(
            "SELECT m.subsystem, m.name, m.unit, COUNT(s.ts), MAX(s.ts)
             FROM metric m LEFT JOIN sample s ON s.metric_id = m.id
             GROUP BY m.id ORDER BY m.subsystem, m.name",
        )
        .map_err(|e| e.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<i64>>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut out = String::from("[");
    let mut first = true;
    for row in rows {
        let (subsystem, name, unit, count, last) = row.map_err(|e| e.to_string())?;
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str(&format!(
            "{{\"subsystem\":\"{}\",\"name\":\"{}\",\"unit\":\"{}\",\"points\":{},\"last_ts\":{}}}",
            esc(&subsystem),
            esc(&name),
            esc(&unit),
            count,
            last.map_or_else(|| "null".to_owned(), |v| v.to_string())
        ));
    }
    out.push(']');
    Ok(out)
}

fn api_latest(db_path: &str) -> Result<String, String> {
    let connection = open(db_path)?;
    // The correlated MAX(ts) is cheap because (metric_id, ts) is the primary key
    // of a WITHOUT ROWID table, so each series' tail is one index seek.
    let mut statement = connection
        .prepare(
            "SELECT m.subsystem, m.name, m.unit, s.ts, s.value
             FROM metric m JOIN sample s ON s.metric_id = m.id
             WHERE s.ts = (SELECT MAX(ts) FROM sample WHERE metric_id = m.id)
             ORDER BY m.subsystem, m.name",
        )
        .map_err(|e| e.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, f64>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut out = String::from("[");
    let mut first = true;
    for row in rows {
        let (subsystem, name, unit, ts, value) = row.map_err(|e| e.to_string())?;
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str(&format!(
            "{{\"subsystem\":\"{}\",\"name\":\"{}\",\"unit\":\"{}\",\"ts\":{},\"value\":{}}}",
            esc(&subsystem),
            esc(&name),
            esc(&unit),
            ts,
            fmt_f64(value)
        ));
    }
    out.push(']');
    Ok(out)
}

/// JSON cannot represent NaN or Infinity, and a single one poisons the whole
/// response for a strict parser.
fn fmt_f64(v: f64) -> String {
    if v.is_finite() {
        format!("{v:.4}")
    } else {
        "null".to_owned()
    }
}

fn param<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then_some(v)
    })
}

fn series_names(names: &str) -> Result<Vec<&str>, String> {
    let mut unique = Vec::new();
    for name in names.split(',').filter(|n| !n.is_empty()) {
        if name.len() > 128 { return Err("metric name too long".into()); }
        if !unique.contains(&name) { unique.push(name); }
        if unique.len() > MAX_SERIES { return Err("too many series".into()); }
    }
    if unique.is_empty() { return Err("no series requested".into()); }
    Ok(unique)
}

fn api_series(db_path: &str, query: &str) -> Result<String, String> {
    let subsystem = param(query, "subsystem").unwrap_or("");
    let names = param(query, "name").unwrap_or("");
    if subsystem.is_empty() || names.is_empty() {
        return Err("subsystem and name are required".to_owned());
    }

    let unique = series_names(names)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let from: i64 = param(query, "from")
        .and_then(|v| v.parse().ok())
        .unwrap_or(now - 3600);
    let to: i64 = param(query, "to").and_then(|v| v.parse().ok()).unwrap_or(now);
    let (from, to) = query_range(from, to, now)?;
    let mut step: i64 = param(query, "step").and_then(|v| v.parse().ok()).unwrap_or(10);
    step = step.max(1);
    // Bound the combined response, including edge buckets, not just each
    // individual series. Repeated names do not multiply database work.
    let per_series = (MAX_TOTAL_POINTS / unique.len() as i64).min(MAX_POINTS);
    step = step.max((to - from) / (per_series - 1) + 1);

    let connection = open(db_path)?;
    let mut out = String::from("{\"step\":");
    out.push_str(&step.to_string());
    out.push_str(",\"series\":[");

    let mut first_series = true;
    for name in unique {
        let mut statement = connection
            .prepare_cached(
                "SELECT (s.ts / ?4) * ?4 AS bucket, AVG(s.value), MIN(s.value), MAX(s.value)
                 FROM sample s JOIN metric m ON m.id = s.metric_id
                 WHERE m.subsystem = ?1 AND m.name = ?2 AND s.ts >= ?3 AND s.ts <= ?5
                 GROUP BY bucket ORDER BY bucket",
            )
            .map_err(|e| e.to_string())?;

        let rows = statement
            .query_map(
                rusqlite::params![subsystem, name, from, step, to],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, f64>(1)?,
                        row.get::<_, f64>(2)?,
                        row.get::<_, f64>(3)?,
                    ))
                },
            )
            .map_err(|e| e.to_string())?;

        if !first_series {
            out.push(',');
        }
        first_series = false;
        out.push_str(&format!(
            "{{\"name\":\"{}\",\"points\":[",
            esc(name)
        ));

        let mut first_point = true;
        for row in rows {
            let (ts, avg, low, high) = row.map_err(|e| e.to_string())?;
            if !first_point {
                out.push(',');
            }
            first_point = false;
            out.push_str(&format!(
                "[{},{},{},{}]",
                ts,
                fmt_f64(avg),
                fmt_f64(low),
                fmt_f64(high)
            ));
        }
        out.push_str("]}");
    }

    out.push_str("]}");
    Ok(out)
}


/// Energy consumed per time bucket, in watt-hours.
///
/// Power is sampled, not integrated, so energy has to be reconstructed. Each
/// sample is treated as holding until the next one -- a step integral -- which is
/// exactly right for a deadbanded store: a reading is only written when it
/// *changes*, so "unchanged since the last row" is the literal meaning of a gap.
///
/// `DT_CAP` guards the one case that breaks: if the daemon was stopped for six
/// hours, the sample either side of the outage would otherwise contribute six
/// hours at that wattage and invent kilowatt-hours that were never drawn.
/// Clamping each interval to twice the heartbeat means an outage under-reports
/// (honest) rather than over-reports (fiction).
fn api_energy(db_path: &str, query: &str, cost_per_kwh: f64) -> Result<String, String> {
    const DT_CAP: i64 = 600;

    let metric = param(query, "metric").unwrap_or("PSTR");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let from: i64 = param(query, "from").and_then(|v| v.parse().ok()).unwrap_or(now - 86400);
    let to: i64 = param(query, "to").and_then(|v| v.parse().ok()).unwrap_or(now);
    let (from, to) = query_range(from, to, now)?;
    let bucket: i64 = param(query, "bucket")
        .and_then(|v| v.parse().ok())
        .unwrap_or(3600)
        .clamp(60, 30 * 86400)
        .max((to - from + MAX_POINTS - 1) / MAX_POINTS);

    let connection = open(db_path)?;
    // LEAD() gives each sample the timestamp of the next one, which is the width
    // of its step. Needs SQLite >= 3.25; the bundled build is well past that.
    let mut statement = connection
        .prepare(
            "WITH s AS (
               SELECT sa.ts AS ts, sa.value AS value,
                      LEAD(sa.ts) OVER (ORDER BY sa.ts) AS next_ts
               FROM sample sa JOIN metric m ON m.id = sa.metric_id
               WHERE m.subsystem = 'smc_power' AND m.name = ?1
                 AND sa.ts >= ?2 AND sa.ts <= ?3
             )
             SELECT (ts / ?4) * ?4 AS b,
                    SUM(value * MIN(next_ts - ts, ?5)) / 3600.0 AS wh,
                    AVG(value) AS avg_w,
                    MAX(value) AS peak_w
             FROM s WHERE next_ts IS NOT NULL
             GROUP BY b ORDER BY b",
        )
        .map_err(|e| e.to_string())?;

    let rows = statement
        .query_map(rusqlite::params![metric, from, to, bucket, DT_CAP], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, f64>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut out = format!(
        "{{\"bucket\":{bucket},\"metric\":\"{}\",\"cost_per_kwh\":{},\"buckets\":[",
        esc(metric),
        fmt_f64(cost_per_kwh)
    );
    let mut first = true;
    let mut total = 0.0;
    for row in rows {
        let (ts, wh, avg_w, peak_w) = row.map_err(|e| e.to_string())?;
        total += wh;
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str(&format!(
            "{{\"ts\":{ts},\"wh\":{},\"avg_w\":{},\"peak_w\":{}}}",
            fmt_f64(wh),
            fmt_f64(avg_w),
            fmt_f64(peak_w)
        ));
    }
    out.push_str(&format!("],\"total_wh\":{}}}", fmt_f64(total)));
    Ok(out)
}

fn respond_json(stream: &TcpStream, body: &str) {
    respond(stream, "200 OK", "application/json", body.as_bytes());
}

fn respond_error(stream: &TcpStream, message: &str) {
    let body = format!("{{\"error\":\"{}\"}}", esc(message));
    respond(stream, "500 Internal Server Error", "application/json", body.as_bytes());
}

fn respond(stream: &TcpStream, status: &str, content_type: &str, body: &[u8]) {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let mut stream = stream;
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

#[cfg(test)]
mod security_tests {
    use super::*;
    use std::io::Cursor;

    fn request(headers: &str) -> Request {
        parse_request(&mut Cursor::new(format!("POST /api/control/reboot HTTP/1.1\r\n{headers}\r\n"))).unwrap()
    }

    #[test]
    fn wildcard_detection_uses_the_resolved_listener_address() {
        assert!(is_wildcard_bind("0.0.0.0:9977".parse().unwrap()));
        assert!(is_wildcard_bind("[::]:9977".parse().unwrap()));
        assert!(!is_wildcard_bind("100.64.0.1:9977".parse().unwrap()));
    }

    #[test]
    fn browser_controls_require_custom_header_and_native_origin() {
        let authority = "100.64.0.1:9977";
        let valid = "Host: 100.64.0.1:9977\r\nX-Neofand-Control: 1\r\n";
        assert!(safe_control_request(&request(valid), authority, authority)); // CLI
        assert!(safe_control_request(&request(&format!("{valid}Origin: http://{authority}\r\n")), authority, authority));
        for origin in ["null", "https://attacker.example", "http://100.64.0.1:9977.attacker.example", "http://100.64.0.1:9977/"] {
            assert!(!safe_control_request(&request(&format!("{valid}Origin: {origin}\r\n")), authority, authority));
        }
        assert!(!safe_control_request(&request("Host: 100.64.0.1:9977\r\n"), authority, authority));
        assert!(!safe_control_request(&request("Host: attacker.example\r\nX-Neofand-Control: 1\r\nOrigin: http://attacker.example\r\n"), authority, authority));
        let v6 = request("Host: [fd7a:115c:a1e0::1]:9977\r\nX-Neofand-Control: 1\r\nOrigin: http://[fd7a:115c:a1e0::1]:9977\r\n");
        assert!(safe_control_request(&v6, "[fd7a:115c:a1e0::1]:9977", "unused"));
    }

    #[test]
    fn parser_rejects_ambiguous_framing_and_oversized_input() {
        for headers in [
            "Content-Length: 1\r\nContent-Length: 1\r\n",
            "Content-Length: 8193\r\n", "Content-Length: -1\r\n",
            "Transfer-Encoding: chunked\r\n",
            "Host: a\r\nHost: b\r\n", "Origin: a\r\nOrigin: b\r\n",
            "X-Neofand-Control: 1\r\nX-Neofand-Control: 1\r\n",
            " Content-Length: 1\r\n",
        ] {
            assert!(parse_request(&mut Cursor::new(format!("POST / HTTP/1.1\r\n{headers}\r\n"))).is_err(), "{headers}");
        }
        assert!(parse_request(&mut Cursor::new(format!("GET /{} HTTP/1.1\r\n\r\n", "x".repeat(MAX_LINE)))).is_err());
        let header = format!("X-Long: {}\r\n", "x".repeat(8000));
        assert!(parse_request(&mut Cursor::new(format!("GET / HTTP/1.1\r\n{}\r\n", header.repeat(3)))).is_err());
        assert!(parse_request(&mut Cursor::new("GET / HTTP/1.1\r\nHost: example")).is_err());
    }

    #[test]
    fn admission_is_bounded_and_permits_are_released() {
        let active = Arc::new(AtomicUsize::new(0));
        let mut permits: Vec<_> = (0..MAX_CONNECTIONS).map(|_| ConnectionPermit::acquire(&active).unwrap()).collect();
        assert!(ConnectionPermit::acquire(&active).is_none());
        permits.pop();
        assert!(ConnectionPermit::acquire(&active).is_some());
        drop(permits);
        assert_eq!(active.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn deadline_rejects_a_stalled_client() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let _client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        let mut reader = DeadlineReader { stream: &server, deadline: Instant::now() + Duration::from_millis(20) };
        let started = Instant::now();
        assert!(reader.read(&mut [0]).is_err());
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn ranges_reject_overflow_inputs_and_bound_history() {
        let now = 2_000_000_000;
        assert_eq!(query_range(i64::MIN, i64::MAX, now).unwrap(), (now - MAX_RANGE, now));
        assert!(query_range(i64::MAX, i64::MIN, now).is_err());
        assert!(query_range(now + 1, now + 2, now).is_err());
    }

    #[test]
    fn series_limit_deduplicates_and_rejects_excessive_names() {
        assert_eq!(series_names("a,a,b,b").unwrap(), vec!["a", "b"]);
        assert!(series_names(",,").is_err());
        assert!(series_names(&"x".repeat(129)).is_err());
        let names = (0..=MAX_SERIES).map(|n| format!("metric{n}")).collect::<Vec<_>>().join(",");
        assert!(series_names(&names).is_err());
    }

    #[test]
    fn sqlite_progress_hook_interrupts_excessive_work() {
        let connection = Connection::open_in_memory().unwrap();
        install_query_deadline(&connection, Duration::from_millis(2));
        let result: rusqlite::Result<i64> = connection.query_row(
            "WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000000) SELECT sum(x) FROM n", [], |row| row.get(0));
        assert!(matches!(result, Err(rusqlite::Error::SqliteFailure(error, _)) if error.code == rusqlite::ErrorCode::OperationInterrupted));
    }
}
