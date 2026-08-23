use crate::i18n::{Language, language, tr};
use chrono::{Datelike, Duration as ChronoDuration, Local, NaiveDate, TimeZone};
use rusqlite::{Connection, OptionalExtension, params};
use std::env;
use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const STATISTICS_SCHEMA_VERSION: i64 = 2;
const RECORDING_SETTLE_TIME: Duration = Duration::from_secs(3);
const MAX_VOICE_DURATION_MS: u64 = 24 * 60 * 60 * 1_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatisticsPeriod {
    Today,
    Week,
    All,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ButtonCount {
    pub key: String,
    pub count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoiceSession {
    pub started_at: i64,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StatisticsSummary {
    pub button_presses: u64,
    pub voice_duration_ms: u64,
    pub voice_sessions: u64,
    pub button_counts: Vec<ButtonCount>,
    pub longest_voice_sessions: Vec<VoiceSession>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatisticsPaths {
    pub database: PathBuf,
    pub handy_recordings: PathBuf,
}

impl StatisticsPaths {
    pub fn from_xdg_environment() -> Result<Self, StatisticsError> {
        let data_home = match env::var_os("XDG_DATA_HOME") {
            Some(value) => PathBuf::from(value),
            None => {
                let home = env::var_os("HOME").ok_or_else(|| {
                    StatisticsError::Configuration(
                        tr(
                            "Neither HOME nor XDG_DATA_HOME is set",
                            "HOME 与 XDG_DATA_HOME 均未设置",
                        )
                        .into(),
                    )
                })?;
                PathBuf::from(home).join(".local/share")
            }
        };
        if !data_home.is_absolute() {
            return Err(StatisticsError::Configuration(
                tr(
                    "XDG_DATA_HOME must be an absolute path",
                    "XDG_DATA_HOME 必须是绝对路径",
                )
                .into(),
            ));
        }
        Ok(Self {
            database: data_home.join("omavoice/statistics.db"),
            handy_recordings: data_home.join("com.pais.handy/recordings"),
        })
    }
}

#[derive(Debug)]
pub enum StatisticsError {
    Configuration(String),
    Io(io::Error),
    Database(rusqlite::Error),
    Audio(hound::Error),
    InvalidTimestamp(i64),
    FutureSchema(i64),
}

impl fmt::Display for StatisticsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(message) => formatter.write_str(message),
            Self::Io(error) => match language() {
                Language::English => write!(formatter, "Statistics file error: {error}"),
                Language::SimplifiedChinese => write!(formatter, "统计文件错误：{error}"),
            },
            Self::Database(error) => match language() {
                Language::English => write!(formatter, "Statistics database error: {error}"),
                Language::SimplifiedChinese => write!(formatter, "统计数据库错误：{error}"),
            },
            Self::Audio(error) => match language() {
                Language::English => write!(formatter, "Handy recording metadata error: {error}"),
                Language::SimplifiedChinese => write!(formatter, "Handy 录音元数据错误：{error}"),
            },
            Self::InvalidTimestamp(timestamp) => match language() {
                Language::English => write!(
                    formatter,
                    "Statistics timestamp is out of range: {timestamp}"
                ),
                Language::SimplifiedChinese => {
                    write!(formatter, "统计时间戳超出支持范围：{timestamp}")
                }
            },
            Self::FutureSchema(version) => match language() {
                Language::English => write!(
                    formatter,
                    "Statistics database schema {version} is newer than the supported version"
                ),
                Language::SimplifiedChinese => {
                    write!(formatter, "统计数据库 Schema {version} 高于当前支持版本")
                }
            },
        }
    }
}

impl Error for StatisticsError {}

impl From<io::Error> for StatisticsError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<rusqlite::Error> for StatisticsError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

impl From<hound::Error> for StatisticsError {
    fn from(error: hound::Error) -> Self {
        Self::Audio(error)
    }
}

pub struct StatisticsDatabase {
    connection: Connection,
}

impl StatisticsDatabase {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StatisticsError> {
        let path = path.as_ref();
        let parent = path.parent().ok_or_else(|| {
            StatisticsError::Configuration(
                tr(
                    "The statistics database has no parent directory",
                    "统计数据库缺少父目录",
                )
                .into(),
            )
        })?;
        fs::create_dir_all(parent)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;

        let connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(2))?;
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        let schema_version: i64 =
            connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if schema_version > STATISTICS_SCHEMA_VERSION {
            return Err(StatisticsError::FutureSchema(schema_version));
        }
        if schema_version == 0 {
            connection.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE IF NOT EXISTS daily_button_counts (
                     local_day TEXT NOT NULL,
                     key TEXT NOT NULL,
                     count INTEGER NOT NULL CHECK (count >= 0),
                     PRIMARY KEY (local_day, key)
                 );
                 CREATE TABLE IF NOT EXISTS voice_sessions (
                     source_id TEXT PRIMARY KEY,
                     started_at INTEGER NOT NULL,
                     duration_ms INTEGER NOT NULL CHECK (duration_ms >= 0)
                 );
                 CREATE INDEX IF NOT EXISTS voice_sessions_started_at
                     ON voice_sessions(started_at);
                 CREATE TABLE IF NOT EXISTS statistics_state (
                     key TEXT PRIMARY KEY,
                     value INTEGER NOT NULL
                 );
                 PRAGMA user_version = 2;
                 COMMIT;",
            )?;
        } else if schema_version == 1 {
            connection.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE statistics_state (
                     key TEXT PRIMARY KEY,
                     value INTEGER NOT NULL
                 );
                 PRAGMA user_version = 2;
                 COMMIT;",
            )?;
        }
        Ok(Self { connection })
    }

    pub fn record_button_at(&self, timestamp: i64, key: &str) -> Result<(), StatisticsError> {
        if key.is_empty()
            || key.len() > 48
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err(StatisticsError::Configuration(
                tr(
                    "The statistics button ID is invalid",
                    "统计按键 ID 不在允许范围",
                )
                .into(),
            ));
        }
        let local_day = local_day(timestamp)?;
        self.connection.execute(
            "INSERT INTO daily_button_counts (local_day, key, count)
             VALUES (?1, ?2, 1)
             ON CONFLICT(local_day, key) DO UPDATE SET count = count + 1",
            params![local_day, key],
        )?;
        Ok(())
    }

    pub fn import_handy_recordings(
        &self,
        directory: &Path,
        now: SystemTime,
    ) -> Result<usize, StatisticsError> {
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error.into()),
        };
        let now_timestamp = system_time_timestamp(now)?;
        let import_after = self
            .connection
            .query_row(
                "SELECT value FROM statistics_state WHERE key = 'voice_import_after'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        let mut imported = 0;
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("wav") {
                continue;
            }
            let metadata = match entry.metadata() {
                Ok(metadata) if metadata.is_file() => metadata,
                _ => continue,
            };
            let modified = match metadata.modified() {
                Ok(modified) => modified,
                Err(_) => continue,
            };
            if now.duration_since(modified).unwrap_or_default() < RECORDING_SETTLE_TIME {
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if file_name.len() > 128
                || !file_name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            {
                continue;
            }
            let started_at = recording_timestamp(file_name)
                .or_else(|| system_time_timestamp(modified).ok())
                .filter(|timestamp| *timestamp > 0 && *timestamp <= now_timestamp + 86_400);
            let Some(started_at) = started_at else {
                continue;
            };
            if import_after.is_some_and(|cutoff| started_at <= cutoff) {
                continue;
            }
            let duration_ms = match wav_duration_ms(&path) {
                Ok(duration_ms) => duration_ms,
                Err(_) => continue,
            };
            if duration_ms == 0 || duration_ms > MAX_VOICE_DURATION_MS {
                continue;
            }
            let changed = self.connection.execute(
                "INSERT INTO voice_sessions (source_id, started_at, duration_ms)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(source_id) DO UPDATE SET
                     started_at = excluded.started_at,
                     duration_ms = excluded.duration_ms
                 WHERE started_at != excluded.started_at OR duration_ms != excluded.duration_ms",
                params![
                    format!("handy-recording:{file_name}"),
                    started_at,
                    duration_ms
                ],
            )?;
            imported += changed;
        }
        Ok(imported)
    }

    pub fn summary(
        &self,
        period: StatisticsPeriod,
        now: i64,
    ) -> Result<StatisticsSummary, StatisticsError> {
        let day_filter = period_start_day(period, now)?;
        let time_filter = period_start_timestamp(period, now)?;
        let mut summary = StatisticsSummary::default();

        let button_sql = if day_filter.is_some() {
            "SELECT COALESCE(SUM(count), 0) FROM daily_button_counts WHERE local_day >= ?1"
        } else {
            "SELECT COALESCE(SUM(count), 0) FROM daily_button_counts WHERE ?1 IS NULL"
        };
        summary.button_presses =
            self.connection
                .query_row(button_sql, params![day_filter.as_deref()], |row| row.get(0))?;

        let voice_sql = if time_filter.is_some() {
            "SELECT COUNT(*), COALESCE(SUM(duration_ms), 0)
             FROM voice_sessions WHERE started_at >= ?1"
        } else {
            "SELECT COUNT(*), COALESCE(SUM(duration_ms), 0)
             FROM voice_sessions WHERE ?1 IS NULL"
        };
        (summary.voice_sessions, summary.voice_duration_ms) =
            self.connection
                .query_row(voice_sql, params![time_filter], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })?;

        let button_rank_sql = if day_filter.is_some() {
            "SELECT key, SUM(count) AS total FROM daily_button_counts
             WHERE local_day >= ?1 GROUP BY key ORDER BY total DESC, key LIMIT 10"
        } else {
            "SELECT key, SUM(count) AS total FROM daily_button_counts
             WHERE ?1 IS NULL GROUP BY key ORDER BY total DESC, key LIMIT 10"
        };
        let mut statement = self.connection.prepare(button_rank_sql)?;
        summary.button_counts = statement
            .query_map(params![day_filter.as_deref()], |row| {
                Ok(ButtonCount {
                    key: row.get(0)?,
                    count: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let voice_rank_sql = if time_filter.is_some() {
            "SELECT started_at, duration_ms FROM voice_sessions
             WHERE started_at >= ?1 ORDER BY duration_ms DESC, started_at DESC LIMIT 10"
        } else {
            "SELECT started_at, duration_ms FROM voice_sessions
             WHERE ?1 IS NULL ORDER BY duration_ms DESC, started_at DESC LIMIT 10"
        };
        let mut statement = self.connection.prepare(voice_rank_sql)?;
        summary.longest_voice_sessions = statement
            .query_map(params![time_filter], |row| {
                Ok(VoiceSession {
                    started_at: row.get(0)?,
                    duration_ms: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(summary)
    }

    pub fn clear(&self) -> Result<(), StatisticsError> {
        self.clear_at(system_time_timestamp(SystemTime::now())?)
    }

    fn clear_at(&self, timestamp: i64) -> Result<(), StatisticsError> {
        local_day(timestamp)?;
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute("DELETE FROM daily_button_counts", [])?;
        transaction.execute("DELETE FROM voice_sessions", [])?;
        transaction.execute(
            "INSERT INTO statistics_state (key, value)
             VALUES ('voice_import_after', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [timestamp],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn voice_session_duration(&self, source_id: &str) -> Result<Option<u64>, StatisticsError> {
        Ok(self
            .connection
            .query_row(
                "SELECT duration_ms FROM voice_sessions WHERE source_id = ?1",
                params![source_id],
                |row| row.get(0),
            )
            .optional()?)
    }
}

fn local_day(timestamp: i64) -> Result<String, StatisticsError> {
    Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|date_time| date_time.format("%Y-%m-%d").to_string())
        .ok_or(StatisticsError::InvalidTimestamp(timestamp))
}

fn period_start_day(period: StatisticsPeriod, now: i64) -> Result<Option<String>, StatisticsError> {
    let now = Local
        .timestamp_opt(now, 0)
        .single()
        .ok_or(StatisticsError::InvalidTimestamp(now))?;
    let date = match period {
        StatisticsPeriod::Today => now.date_naive(),
        StatisticsPeriod::Week => {
            now.date_naive() - ChronoDuration::days(i64::from(now.weekday().num_days_from_monday()))
        }
        StatisticsPeriod::All => return Ok(None),
    };
    Ok(Some(date.format("%Y-%m-%d").to_string()))
}

fn period_start_timestamp(
    period: StatisticsPeriod,
    now: i64,
) -> Result<Option<i64>, StatisticsError> {
    let Some(day) = period_start_day(period, now)? else {
        return Ok(None);
    };
    let date = NaiveDate::parse_from_str(&day, "%Y-%m-%d")
        .map_err(|_| StatisticsError::InvalidTimestamp(now))?;
    let midnight = date
        .and_hms_opt(0, 0, 0)
        .ok_or(StatisticsError::InvalidTimestamp(now))?;
    Local
        .from_local_datetime(&midnight)
        .earliest()
        .map(|date_time| Some(date_time.timestamp()))
        .ok_or(StatisticsError::InvalidTimestamp(now))
}

fn system_time_timestamp(time: SystemTime) -> Result<i64, StatisticsError> {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StatisticsError::InvalidTimestamp(-1))?;
    i64::try_from(duration.as_secs()).map_err(|_| StatisticsError::InvalidTimestamp(i64::MAX))
}

fn recording_timestamp(file_name: &str) -> Option<i64> {
    file_name
        .strip_prefix("handy-")?
        .strip_suffix(".wav")?
        .parse()
        .ok()
}

fn wav_duration_ms(path: &Path) -> Result<u64, StatisticsError> {
    let reader = hound::WavReader::open(path)?;
    let sample_rate = reader.spec().sample_rate;
    if sample_rate == 0 {
        return Ok(0);
    }
    Ok(u64::from(reader.duration()) * 1_000 / u64::from(sample_rate))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn database(temporary: &TempDir) -> StatisticsDatabase {
        StatisticsDatabase::open(temporary.path().join("data/omavoice/statistics.db")).unwrap()
    }

    #[test]
    fn database_is_private_and_aggregates_buttons_without_event_rows() {
        let temporary = TempDir::new().unwrap();
        let path = temporary.path().join("data/omavoice/statistics.db");
        let database = StatisticsDatabase::open(&path).unwrap();

        database.record_button_at(1_787_450_000, "up").unwrap();
        database.record_button_at(1_787_450_001, "up").unwrap();
        database.record_button_at(1_787_450_002, "enter").unwrap();

        let summary = database
            .summary(StatisticsPeriod::All, 1_787_450_100)
            .unwrap();
        assert_eq!(summary.button_presses, 3);
        assert_eq!(
            summary.button_counts,
            vec![
                ButtonCount {
                    key: "up".into(),
                    count: 2,
                },
                ButtonCount {
                    key: "enter".into(),
                    count: 1,
                },
            ]
        );
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn invalid_button_ids_are_rejected_instead_of_storing_input_text() {
        let temporary = TempDir::new().unwrap();
        let database = database(&temporary);

        for invalid in ["", "P", "ctrl+p", "hello world", "用户文字"] {
            assert!(database.record_button_at(1_787_450_000, invalid).is_err());
        }
        assert_eq!(
            database
                .summary(StatisticsPeriod::All, 1_787_450_100)
                .unwrap()
                .button_presses,
            0
        );
    }

    #[test]
    fn handy_wav_import_is_settled_deduplicated_and_duration_only() {
        let temporary = TempDir::new().unwrap();
        let recordings = temporary.path().join("recordings");
        fs::create_dir(&recordings).unwrap();
        let path = recordings.join("handy-1787454000.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..32_000 {
            writer.write_sample(0_i16).unwrap();
        }
        writer.finalize().unwrap();

        let database = database(&temporary);
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(
            database
                .import_handy_recordings(&recordings, modified)
                .unwrap(),
            0
        );
        let settled = modified + Duration::from_secs(4);
        assert_eq!(
            database
                .import_handy_recordings(&recordings, settled)
                .unwrap(),
            1
        );
        assert_eq!(
            database
                .import_handy_recordings(&recordings, settled)
                .unwrap(),
            0
        );
        let summary = database
            .summary(StatisticsPeriod::All, 1_787_454_100)
            .unwrap();
        assert_eq!(summary.voice_sessions, 1);
        assert_eq!(summary.voice_duration_ms, 2_000);
        assert_eq!(summary.longest_voice_sessions[0].duration_ms, 2_000);
        assert_eq!(
            database
                .voice_session_duration("handy-recording:handy-1787454000.wav")
                .unwrap(),
            Some(2_000)
        );
    }

    #[test]
    fn corrupt_recording_does_not_block_other_handy_sessions() {
        let temporary = TempDir::new().unwrap();
        let recordings = temporary.path().join("recordings");
        fs::create_dir(&recordings).unwrap();
        let corrupt = recordings.join("handy-1787454000.wav");
        fs::write(&corrupt, b"not a wav").unwrap();
        let modified = fs::metadata(&corrupt).unwrap().modified().unwrap();

        let database = database(&temporary);
        assert_eq!(
            database
                .import_handy_recordings(&recordings, modified + Duration::from_secs(4))
                .unwrap(),
            0
        );
        assert_eq!(
            database
                .summary(StatisticsPeriod::All, 1_787_454_100)
                .unwrap(),
            StatisticsSummary::default()
        );
    }

    #[test]
    fn today_week_and_all_use_local_calendar_boundaries() {
        let temporary = TempDir::new().unwrap();
        let database = database(&temporary);
        let now = Local::now();
        let today = now.timestamp();
        let yesterday = (now - ChronoDuration::days(1)).timestamp();
        let last_week = (now - ChronoDuration::days(8)).timestamp();

        database.record_button_at(today, "enter").unwrap();
        database.record_button_at(yesterday, "enter").unwrap();
        database.record_button_at(last_week, "enter").unwrap();

        assert_eq!(
            database
                .summary(StatisticsPeriod::Today, today)
                .unwrap()
                .button_presses,
            1
        );
        let week = database
            .summary(StatisticsPeriod::Week, today)
            .unwrap()
            .button_presses;
        assert!((1..=2).contains(&week));
        assert_eq!(
            database
                .summary(StatisticsPeriod::All, today)
                .unwrap()
                .button_presses,
            3
        );
    }

    #[test]
    fn clear_removes_aggregates_and_voice_sessions() {
        let temporary = TempDir::new().unwrap();
        let database = database(&temporary);
        database.record_button_at(1_787_450_000, "enter").unwrap();
        database
            .connection
            .execute(
                "INSERT INTO voice_sessions (source_id, started_at, duration_ms)
                 VALUES ('test', 1787454000, 1000)",
                [],
            )
            .unwrap();

        database.clear().unwrap();

        assert_eq!(
            database
                .summary(StatisticsPeriod::All, 1_787_450_100)
                .unwrap(),
            StatisticsSummary::default()
        );
    }

    #[test]
    fn cleared_handy_sessions_are_not_reimported_from_retained_recordings() {
        let temporary = TempDir::new().unwrap();
        let recordings = temporary.path().join("recordings");
        fs::create_dir(&recordings).unwrap();
        let path = recordings.join("handy-1787454000.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..16_000 {
            writer.write_sample(0_i16).unwrap();
        }
        writer.finalize().unwrap();
        let settled = fs::metadata(&path).unwrap().modified().unwrap() + Duration::from_secs(4);

        let database = database(&temporary);
        assert_eq!(
            database
                .import_handy_recordings(&recordings, settled)
                .unwrap(),
            1
        );
        database.clear_at(1_787_454_100).unwrap();
        assert_eq!(
            database
                .import_handy_recordings(&recordings, settled)
                .unwrap(),
            0
        );
        assert_eq!(
            database
                .summary(StatisticsPeriod::All, 1_787_454_200)
                .unwrap(),
            StatisticsSummary::default()
        );
    }

    #[test]
    fn schema_one_database_migrates_without_losing_aggregates() {
        let temporary = TempDir::new().unwrap();
        let path = temporary.path().join("data/omavoice/statistics.db");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE daily_button_counts (
                     local_day TEXT NOT NULL,
                     key TEXT NOT NULL,
                     count INTEGER NOT NULL,
                     PRIMARY KEY (local_day, key)
                 );
                 CREATE TABLE voice_sessions (
                     source_id TEXT PRIMARY KEY,
                     started_at INTEGER NOT NULL,
                     duration_ms INTEGER NOT NULL
                 );
                 INSERT INTO daily_button_counts VALUES ('2026-08-23', 'enter', 2);
                 PRAGMA user_version = 1;",
            )
            .unwrap();
        drop(connection);

        let database = StatisticsDatabase::open(&path).unwrap();
        assert_eq!(
            database
                .connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            STATISTICS_SCHEMA_VERSION
        );
        assert_eq!(
            database
                .summary(StatisticsPeriod::All, 1_787_454_200)
                .unwrap()
                .button_presses,
            2
        );
    }
}
