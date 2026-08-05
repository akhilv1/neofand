#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("T2 Fan Daemon must be run as root")]
    NotRoot,
    #[error("Fan not found")]
    NoFan,
    #[error("No {0} temperature sensors found")]
    NoSensors(&'static str),

    #[error("Temperature sensor cannot be read")]
    TempRead(#[source] std::io::Error),
    #[error("Temperature sensor cannot be seeked")]
    TempSeek(#[source] std::io::Error),
    #[error("Temporature sensor cannot be parsed")]
    TempParse(#[source] std::num::ParseIntError),

    #[error("Cannot read minimum fan speed")]
    MinSpeedRead(#[source] std::io::Error),
    #[error("Cannot parse minimum fan speed")]
    MinSpeedParse(#[source] std::num::ParseIntError),
    #[error("Cannot read maximum fan speed")]
    MaxSpeedRead(#[source] std::io::Error),
    #[error("Cannot parse maximum fan speed")]
    MaxSpeedParse(#[source] std::num::ParseIntError),

    #[error("Cannot read pid file")]
    PidRead(#[source] std::io::Error),
    #[error("Cannot write pid file")]
    PidWrite(#[source] std::io::Error),
    #[error("Cannot delete pid file")]
    PidDelete(#[source] std::io::Error),
    #[error("T2 Fan Daemon is already running")]
    AlreadyRunning,

    #[error("Cannot create default config file")]
    ConfigCreate(#[source] std::io::Error),
    #[error("Cannot read config file")]
    ConfigRead(#[source] std::io::Error),
    #[error("Cannot parse config file")]
    ConfigParse(
        #[from]
        #[source]
        ini::ParseError,
    ),
    // Config parsing is deliberately lenient — a missing or malformed key warns
    // and falls back to the profile default rather than refusing to start, so
    // there are no "bad config value" error variants here.

    #[error("Cannot open fan controller handle")]
    FanOpen(#[source] std::io::Error),
    #[error("Cannot write to fan controller")]
    FanWrite(#[source] std::io::Error),

    #[error("Cannot setup shutdown signals")]
    Signal(#[source] std::io::Error),

    #[error("Programmer Error: Invalid glob pattern")]
    Glob(
        #[from]
        #[source]
        glob::PatternError,
    ),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
