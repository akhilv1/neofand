//! The telemetry store: a SQLite database written by a dedicated thread.
//!
//! ## Why a separate thread
//!
//! The control loop must never block. It holds `fan*_manual`, and a stall in it
//! is a stall in fan control — on a machine that can pull 1 kW that is a real
//! consequence, not a latency nit. SQLite commits touch the disk, so the loop
//! hands finished samples to this thread over a bounded channel and returns
//! immediately. If the channel is full the sample is dropped and counted; losing
//! a row is always preferable to delaying a fan decision.
//!
//! This thread never touches the SMC. It only receives values the control thread
//! already read, which is what keeps the single-reader rule in `smc.rs` intact.
//!
//! ## Schema
//!
//! ```sql
//! metric(id, subsystem, name, unit)      -- one row per channel, ~360 total
//! sample(metric_id, ts, value)           -- the time series
//! ```
//!
//! Narrow rather than a wide column-per-sensor table: the channel set differs
//! per machine and changes when a card is swapped, and a wide table would need a
//! migration every time. `WITHOUT ROWID` with `(metric_id, ts)` as the primary
//! key stores each series contiguously, which is the access pattern every
//! dashboard query uses.
//!
//! ## Deadband
//!
//! Most channels are flat most of the time. Writing all ~360 every interval
//! stores mostly-unchanged rows, so a reading is only written when it moved by
//! more than its kind's threshold, or when `heartbeat_secs` has elapsed so flat
//! series still have anchor points. Discrete channels bypass the threshold and
//! are written on any change.

use std::{
    collections::HashMap,
    sync::mpsc::{sync_channel, SyncSender, TrySendError},
    time::Duration,
};

use rusqlite::Connection;

use crate::config::LoggingConfig;
use crate::telemetry::{Kind, Sample};

/// Bounded so a slow disk cannot grow this without limit. Ten samples is well
/// over a minute of headroom at the default interval.
const QUEUE_DEPTH: usize = 10;

/// How often to delete rows past the retention horizon.
const PRUNE_INTERVAL: Duration = Duration::from_secs(3600);

pub struct Logger {
    tx: SyncSender<Sample>,
    dropped: std::cell::Cell<u64>,
}

impl Logger {
    /// Open the database and start the writer thread.
    ///
    /// Returns the error rather than panicking: telemetry is an optional extra
    /// and must never prevent the daemon from starting.
    pub fn spawn(config: &LoggingConfig) -> Result<Self, String> {
        if let Some(parent) = std::path::Path::new(&config.db_path).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }

        let mut connection = Connection::open(&config.db_path)
            .map_err(|e| format!("cannot open {}: {e}", config.db_path))?;
        init_schema(&connection).map_err(|e| format!("schema: {e}"))?;

        let (tx, rx) = sync_channel::<Sample>(QUEUE_DEPTH);
        let settings = Settings::from(config);

        std::thread::Builder::new()
            .name("neofand-logdb".to_owned())
            .spawn(move || {
                let mut state = WriterState::default();
                while let Ok(sample) = rx.recv() {
                    if let Err(e) = write_sample(&mut connection, &settings, &mut state, &sample) {
                        // Keep going: a transient write failure should cost rows,
                        // not the whole logger.
                        eprintln!("neofand: telemetry write failed: {e}");
                    }
                }
            })
            .map_err(|e| format!("cannot start writer thread: {e}"))?;

        Ok(Self {
            tx,
            dropped: std::cell::Cell::new(0),
        })
    }

    /// Hand a sample to the writer. Never blocks.
    pub fn submit(&self, sample: Sample) {
        match self.tx.try_send(sample) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                let n = self.dropped.get() + 1;
                self.dropped.set(n);
                // Only complain occasionally; a disk that is slow now is likely
                // to be slow for the next several samples too.
                if n == 1 || n % 100 == 0 {
                    eprintln!("neofand: telemetry queue full, dropped {n} sample(s)");
                }
            }
            Err(TrySendError::Disconnected(_)) => {}
        }
    }
}

struct Settings {
    retention_days: u32,
    deadband_temp_c: f64,
    deadband_power_w: f64,
    deadband_rpm: f64,
    heartbeat_secs: u64,
}

impl From<&LoggingConfig> for Settings {
    fn from(c: &LoggingConfig) -> Self {
        Self {
            retention_days: c.retention_days,
            deadband_temp_c: c.deadband_temp_c,
            deadband_power_w: c.deadband_power_w,
            deadband_rpm: c.deadband_rpm,
            heartbeat_secs: c.heartbeat_secs,
        }
    }
}

impl Settings {
    /// Threshold for a kind. Zero means "write on any change", which is how
    /// discrete channels avoid being smoothed away.
    fn deadband(&self, kind: Kind) -> f64 {
        match kind {
            Kind::Temp => self.deadband_temp_c,
            Kind::Power => self.deadband_power_w,
            // Fixed rather than configurable: these exist to characterise rail
            // sag, and a user-tuned threshold would most likely be set from
            // watt-scale intuition and erase the signal.
            Kind::Volts => 0.02,
            Kind::Amps => 0.1,
            Kind::Rpm => self.deadband_rpm,
            Kind::Percent => 1.0,
            Kind::Bytes => 1_048_576.0,
            Kind::Discrete => 0.0,
        }
    }
}

#[derive(Default)]
struct WriterState {
    /// `(subsystem, name)` to metric id.
    ids: HashMap<(String, String), i64>,
    /// Last value written per metric, and when.
    last: HashMap<i64, (f64, u64)>,
    last_prune: Option<std::time::Instant>,
}

fn init_schema(connection: &Connection) -> rusqlite::Result<()> {
    // WAL lets the web thread read while this thread writes. NORMAL sync is the
    // right trade for telemetry: a power cut can cost the last commits, and the
    // alternative is an fsync every interval forever.
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS metric (
             id        INTEGER PRIMARY KEY,
             subsystem TEXT NOT NULL,
             name      TEXT NOT NULL,
             unit      TEXT NOT NULL DEFAULT '',
             UNIQUE(subsystem, name)
         );
         CREATE TABLE IF NOT EXISTS sample (
             metric_id INTEGER NOT NULL REFERENCES metric(id),
             ts        INTEGER NOT NULL,
             value     REAL    NOT NULL,
             PRIMARY KEY (metric_id, ts)
         ) WITHOUT ROWID;
         CREATE INDEX IF NOT EXISTS sample_ts ON sample(ts);",
    )
}

fn metric_id(
    connection: &Connection,
    state: &mut WriterState,
    subsystem: &str,
    name: &str,
    unit: &str,
) -> rusqlite::Result<i64> {
    let key = (subsystem.to_owned(), name.to_owned());
    if let Some(id) = state.ids.get(&key) {
        return Ok(*id);
    }

    connection.execute(
        "INSERT OR IGNORE INTO metric (subsystem, name, unit) VALUES (?1, ?2, ?3)",
        rusqlite::params![subsystem, name, unit],
    )?;
    let id: i64 = connection.query_row(
        "SELECT id FROM metric WHERE subsystem = ?1 AND name = ?2",
        rusqlite::params![subsystem, name],
        |row| row.get(0),
    )?;
    state.ids.insert(key, id);
    Ok(id)
}

fn write_sample(
    connection: &mut Connection,
    settings: &Settings,
    state: &mut WriterState,
    sample: &Sample,
) -> rusqlite::Result<()> {
    let tx = connection.transaction()?;
    {
        let mut insert = tx.prepare_cached(
            "INSERT OR REPLACE INTO sample (metric_id, ts, value) VALUES (?1, ?2, ?3)",
        )?;

        for reading in &sample.readings {
            let id = metric_id(&tx, state, reading.subsystem, &reading.name, reading.unit)?;

            let deadband = settings.deadband(reading.kind);
            let should_write = match state.last.get(&id) {
                None => true,
                Some((previous, when)) => {
                    let stale = sample.unix_time.saturating_sub(*when) >= settings.heartbeat_secs;
                    let moved = if deadband > 0.0 {
                        (reading.value - previous).abs() >= deadband
                    } else {
                        // Any change at all, for discrete channels.
                        (reading.value - previous).abs() > f64::EPSILON
                    };
                    stale || moved
                }
            };

            if should_write {
                insert.execute(rusqlite::params![id, sample.unix_time as i64, reading.value])?;
                state.last.insert(id, (reading.value, sample.unix_time));
            }
        }
    }
    tx.commit()?;

    prune_if_due(connection, settings, state)
}

fn prune_if_due(
    connection: &Connection,
    settings: &Settings,
    state: &mut WriterState,
) -> rusqlite::Result<()> {
    let now = std::time::Instant::now();
    let due = state
        .last_prune
        .map_or(true, |last| now.duration_since(last) >= PRUNE_INTERVAL);
    if !due {
        return Ok(());
    }
    state.last_prune = Some(now);

    if settings.retention_days == 0 {
        return Ok(());
    }
    let horizon = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        .saturating_sub(u64::from(settings.retention_days) * 86_400);

    connection.execute(
        "DELETE FROM sample WHERE ts < ?1",
        rusqlite::params![horizon as i64],
    )?;
    Ok(())
}
