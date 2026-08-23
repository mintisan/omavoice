use crate::i18n::{Language, language, tr};
use chrono::{DateTime, Local, SecondsFormat};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const TRANSCRIPT_DATABASE_SCHEMA_VERSION: i64 = 1;
pub const TRANSCRIPT_ARCHIVE_VERSION: u32 = 1;
pub const TRANSCRIPT_ARCHIVE_FORMAT: &str = "app.sayall.transcript-archive";
pub const HANDY_HISTORY_SCHEMA_VERSION: i64 = 4;
const MAX_ARCHIVE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 10_000;
const MAX_TRANSCRIPT_BYTES: usize = 256 * 1024;
const MAX_RECORD_ID_BYTES: usize = 160;
const MAX_SOURCE_BYTES: usize = 32;
const MAX_DURATION_MS: u64 = 24 * 60 * 60 * 1_000;
static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranscriptPaths {
    pub database: PathBuf,
    pub statistics_database: PathBuf,
    pub handy_history: PathBuf,
}

impl TranscriptPaths {
    pub fn from_xdg_environment() -> Result<Self, TranscriptError> {
        let data_home = match env::var_os("XDG_DATA_HOME") {
            Some(value) => PathBuf::from(value),
            None => {
                let home = env::var_os("HOME").ok_or_else(|| {
                    TranscriptError::Configuration(
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
            return Err(TranscriptError::Configuration(
                tr(
                    "XDG_DATA_HOME must be an absolute path",
                    "XDG_DATA_HOME 必须是绝对路径",
                )
                .into(),
            ));
        }
        Ok(Self {
            database: data_home.join("sayall/transcripts.db"),
            statistics_database: data_home.join("sayall/statistics.db"),
            handy_history: data_home.join("com.pais.handy/history.db"),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptArchive {
    pub format: String,
    pub version: u32,
    pub exported_at: String,
    pub entries: Vec<TranscriptArchiveEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptArchiveEntry {
    pub id: String,
    pub completed_at: String,
    pub utc_offset_minutes: i32,
    pub duration_ms: Option<u64>,
    pub text: String,
    pub source: String,
    pub archived_at: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TranscriptImportPreview {
    pub total: usize,
    pub new_entries: usize,
    pub duplicates: usize,
}

#[derive(Debug)]
pub enum TranscriptError {
    Configuration(String),
    Io(io::Error),
    Database(rusqlite::Error),
    Json(serde_json::Error),
    FutureSchema(i64),
    HandySchema(i64),
    InvalidArchive(String),
}

impl fmt::Display for TranscriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(message) => formatter.write_str(message),
            Self::Io(error) => match language() {
                Language::English => write!(formatter, "Transcript archive file error: {error}"),
                Language::SimplifiedChinese => write!(formatter, "文字档案文件错误：{error}"),
            },
            Self::Database(error) => match language() {
                Language::English => {
                    write!(formatter, "Transcript archive database error: {error}")
                }
                Language::SimplifiedChinese => write!(formatter, "文字档案数据库错误：{error}"),
            },
            Self::Json(error) => match language() {
                Language::English => write!(formatter, "Transcript archive JSON error: {error}"),
                Language::SimplifiedChinese => write!(formatter, "文字档案 JSON 错误：{error}"),
            },
            Self::FutureSchema(version) => {
                write!(
                    formatter,
                    "{}",
                    match language() {
                        Language::English => format!(
                            "Transcript archive database schema {version} is newer than the supported version"
                        ),
                        Language::SimplifiedChinese =>
                            format!("文字档案数据库 Schema {version} 高于当前支持版本"),
                    }
                )
            }
            Self::HandySchema(version) => match language() {
                Language::English => write!(
                    formatter,
                    "Handy history schema {version} is incompatible with supported schema {HANDY_HISTORY_SCHEMA_VERSION}"
                ),
                Language::SimplifiedChinese => write!(
                    formatter,
                    "Handy 历史库 Schema {version} 与当前支持的 {HANDY_HISTORY_SCHEMA_VERSION} 不兼容"
                ),
            },
            Self::InvalidArchive(message) => match language() {
                Language::English => write!(formatter, "Invalid transcript archive: {message}"),
                Language::SimplifiedChinese => write!(formatter, "文字档案无效：{message}"),
            },
        }
    }
}

impl Error for TranscriptError {}

impl From<io::Error> for TranscriptError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<rusqlite::Error> for TranscriptError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

impl From<serde_json::Error> for TranscriptError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

pub struct TranscriptDatabase {
    connection: Connection,
}

impl TranscriptDatabase {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, TranscriptError> {
        let path = path.as_ref();
        let parent = path.parent().ok_or_else(|| {
            TranscriptError::Configuration(
                tr(
                    "The transcript archive database has no parent directory",
                    "文字档案数据库缺少父目录",
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
        let schema_version: i64 =
            connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if schema_version > TRANSCRIPT_DATABASE_SCHEMA_VERSION {
            return Err(TranscriptError::FutureSchema(schema_version));
        }
        if schema_version == 0 {
            connection.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE transcript_entries (
                     record_id TEXT PRIMARY KEY,
                     completed_at INTEGER NOT NULL,
                     utc_offset_minutes INTEGER NOT NULL,
                     duration_ms INTEGER CHECK (duration_ms IS NULL OR duration_ms > 0),
                     final_text TEXT NOT NULL CHECK (length(final_text) > 0),
                     source TEXT NOT NULL,
                     archived_at INTEGER NOT NULL
                 );
                 CREATE INDEX transcript_entries_completed_at
                     ON transcript_entries(completed_at DESC);
                 CREATE TABLE transcript_state (
                     key TEXT PRIMARY KEY,
                     value INTEGER NOT NULL
                 );
                 PRAGMA user_version = 1;
                 COMMIT;",
            )?;
        }
        Ok(Self { connection })
    }

    pub fn archive_enabled(&self) -> Result<bool, TranscriptError> {
        Ok(self.state("archive_enabled")?.unwrap_or(0) == 1)
    }

    pub fn set_archive_enabled(&self, enabled: bool) -> Result<(), TranscriptError> {
        self.set_state("archive_enabled", i64::from(enabled))
    }

    pub fn entry_count(&self) -> Result<u64, TranscriptError> {
        Ok(self
            .connection
            .query_row("SELECT COUNT(*) FROM transcript_entries", [], |row| {
                row.get(0)
            })?)
    }

    pub fn import_handy_history(
        &self,
        path: &Path,
        statistics_database: &Path,
        now: SystemTime,
    ) -> Result<usize, TranscriptError> {
        if !self.archive_enabled()? {
            return Ok(0);
        }
        let history = match Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(connection) => connection,
            Err(rusqlite::Error::SqliteFailure(error, _))
                if error.code == rusqlite::ErrorCode::CannotOpen =>
            {
                return Ok(0);
            }
            Err(error) => return Err(error.into()),
        };
        history.busy_timeout(Duration::from_secs(2))?;
        let schema_version: i64 = history.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if schema_version != HANDY_HISTORY_SCHEMA_VERSION {
            return Err(TranscriptError::HandySchema(schema_version));
        }

        let now = system_time_timestamp(now)?;
        let mut watermark = self.state("handy_history_after_id")?.unwrap_or(0);
        let import_after = self.state("handy_history_import_after")?;
        let latest_id: i64 = history.query_row(
            "SELECT COALESCE(MAX(id), 0) FROM transcription_history",
            [],
            |row| row.get(0),
        )?;
        if latest_id < watermark {
            watermark = 0;
        }

        let mut statement = history.prepare(
            "SELECT id, file_name, timestamp, transcription_text, post_processed_text
             FROM transcription_history WHERE id > ?1 ORDER BY id",
        )?;
        let entries = statement
            .query_map([watermark], |row| {
                Ok(HandyTranscript {
                    id: row.get(0)?,
                    file_name: row.get(1)?,
                    completed_at: row.get(2)?,
                    transcription_text: row.get(3)?,
                    post_processed_text: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        let entries = entries
            .into_iter()
            .map(|entry| {
                let duration_ms = handy_recording_duration(statistics_database, &entry.file_name)?;
                Ok((entry, duration_ms))
            })
            .collect::<Result<Vec<_>, TranscriptError>>()?;

        let mut imported = 0;
        let mut last_seen_id = watermark;
        let transaction = self.connection.unchecked_transaction()?;
        for (entry, duration_ms) in entries {
            last_seen_id = last_seen_id.max(entry.id);
            let final_text = entry
                .post_processed_text
                .unwrap_or(entry.transcription_text);
            if !valid_text(&final_text)
                || !valid_handy_file_name(&entry.file_name)
                || entry.completed_at <= 0
                || entry.completed_at > now + 86_400
                || import_after.is_some_and(|cutoff| entry.completed_at <= cutoff)
            {
                continue;
            }
            let offset = local_offset_minutes(entry.completed_at)?;
            imported += transaction.execute(
                "INSERT INTO transcript_entries (
                     record_id, completed_at, utc_offset_minutes, duration_ms,
                     final_text, source, archived_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'handy', ?6)
                 ON CONFLICT(record_id) DO NOTHING",
                params![
                    format!("handy:{}:{}", entry.id, entry.file_name),
                    entry.completed_at,
                    offset,
                    duration_ms,
                    final_text,
                    now
                ],
            )?;
        }
        transaction.execute(
            "INSERT INTO transcript_state (key, value)
             VALUES ('handy_history_after_id', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [last_seen_id],
        )?;
        transaction.commit()?;
        Ok(imported)
    }

    pub fn export_archive(&self, now: SystemTime) -> Result<TranscriptArchive, TranscriptError> {
        let exported_at = system_time_timestamp(now)?;
        let mut statement = self.connection.prepare(
            "SELECT record_id, completed_at, utc_offset_minutes, duration_ms,
                    final_text, source, archived_at
             FROM transcript_entries ORDER BY completed_at, record_id",
        )?;
        let entries = statement
            .query_map([], |row| {
                let completed_at: i64 = row.get(1)?;
                let utc_offset_minutes: i32 = row.get(2)?;
                let archived_at: i64 = row.get(6)?;
                Ok(TranscriptArchiveEntry {
                    id: row.get(0)?,
                    completed_at: format_timestamp(completed_at, utc_offset_minutes),
                    utc_offset_minutes,
                    duration_ms: row.get(3)?,
                    text: row.get(4)?,
                    source: row.get(5)?,
                    archived_at: format_timestamp(archived_at, 0),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(TranscriptArchive {
            format: TRANSCRIPT_ARCHIVE_FORMAT.into(),
            version: TRANSCRIPT_ARCHIVE_VERSION,
            exported_at: format_timestamp(exported_at, 0),
            entries,
        })
    }

    pub fn preview_import(
        &self,
        archive: &TranscriptArchive,
        now: SystemTime,
    ) -> Result<TranscriptImportPreview, TranscriptError> {
        validate_archive(archive, now)?;
        let mut preview = TranscriptImportPreview {
            total: archive.entries.len(),
            ..TranscriptImportPreview::default()
        };
        for entry in &archive.entries {
            let exists = self
                .connection
                .query_row(
                    "SELECT 1 FROM transcript_entries WHERE record_id = ?1",
                    [&entry.id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if exists {
                preview.duplicates += 1;
            } else {
                preview.new_entries += 1;
            }
        }
        Ok(preview)
    }

    pub fn import_archive(
        &self,
        archive: &TranscriptArchive,
        now: SystemTime,
    ) -> Result<usize, TranscriptError> {
        validate_archive(archive, now)?;
        let transaction = self.connection.unchecked_transaction()?;
        let mut imported = 0;
        for entry in &archive.entries {
            let completed_at = parse_timestamp(&entry.completed_at)?;
            let archived_at = parse_timestamp(&entry.archived_at)?;
            imported += transaction.execute(
                "INSERT INTO transcript_entries (
                     record_id, completed_at, utc_offset_minutes, duration_ms,
                     final_text, source, archived_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(record_id) DO NOTHING",
                params![
                    entry.id,
                    completed_at,
                    entry.utc_offset_minutes,
                    entry.duration_ms,
                    entry.text,
                    entry.source,
                    archived_at
                ],
            )?;
        }
        transaction.commit()?;
        Ok(imported)
    }

    pub fn clear(&self) -> Result<(), TranscriptError> {
        self.clear_at(SystemTime::now())
    }

    fn clear_at(&self, now: SystemTime) -> Result<(), TranscriptError> {
        let now = system_time_timestamp(now)?;
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute("DELETE FROM transcript_entries", [])?;
        transaction.execute(
            "INSERT INTO transcript_state (key, value)
             VALUES ('handy_history_import_after', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [now],
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn state(&self, key: &str) -> Result<Option<i64>, TranscriptError> {
        Ok(self
            .connection
            .query_row(
                "SELECT value FROM transcript_state WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()?)
    }

    fn set_state(&self, key: &str, value: i64) -> Result<(), TranscriptError> {
        self.connection.execute(
            "INSERT INTO transcript_state (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

pub fn read_archive_file(
    path: &Path,
    now: SystemTime,
) -> Result<TranscriptArchive, TranscriptError> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(TranscriptError::InvalidArchive(
            tr(
                "The selected path is not a regular file",
                "所选路径不是普通文件",
            )
            .into(),
        ));
    }
    if metadata.len() > MAX_ARCHIVE_BYTES {
        return Err(TranscriptError::InvalidArchive(match language() {
            Language::English => format!(
                "The file exceeds the {} MiB limit",
                MAX_ARCHIVE_BYTES / 1024 / 1024
            ),
            Language::SimplifiedChinese => {
                format!("文件超过 {} MiB 上限", MAX_ARCHIVE_BYTES / 1024 / 1024)
            }
        }));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)?
        .take(MAX_ARCHIVE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_ARCHIVE_BYTES {
        return Err(TranscriptError::InvalidArchive(
            tr("The file is too large", "文件过大").into(),
        ));
    }
    let archive: TranscriptArchive = serde_json::from_slice(&bytes)?;
    validate_archive(&archive, now)?;
    Ok(archive)
}

pub fn write_archive_file(path: &Path, archive: &TranscriptArchive) -> Result<(), TranscriptError> {
    validate_archive(archive, SystemTime::now())?;
    let parent = path.parent().ok_or_else(|| {
        TranscriptError::InvalidArchive(
            tr(
                "The export path has no parent directory",
                "导出路径缺少父目录",
            )
            .into(),
        )
    })?;
    let bytes = serde_json::to_vec_pretty(archive)?;
    if bytes.len() as u64 > MAX_ARCHIVE_BYTES {
        return Err(TranscriptError::InvalidArchive(
            tr(
                "The exported content exceeds the size limit",
                "导出内容超过大小上限",
            )
            .into(),
        ));
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            TranscriptError::InvalidArchive(
                tr("The export file name is invalid", "导出文件名无效").into(),
            )
        })?;
    let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{file_name}.{}.{sequence}.tmp",
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)?;
    let result = (|| -> io::Result<()> {
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(TranscriptError::Io)
}

#[derive(Debug)]
struct HandyTranscript {
    id: i64,
    file_name: String,
    completed_at: i64,
    transcription_text: String,
    post_processed_text: Option<String>,
}

fn handy_recording_duration(
    statistics_database: &Path,
    file_name: &str,
) -> Result<Option<u64>, TranscriptError> {
    let statistics = match Connection::open_with_flags(
        statistics_database,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(connection) => connection,
        Err(rusqlite::Error::SqliteFailure(error, _))
            if error.code == rusqlite::ErrorCode::CannotOpen =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
    };
    Ok(statistics
        .query_row(
            "SELECT duration_ms FROM voice_sessions WHERE source_id = ?1",
            [format!("handy-recording:{file_name}")],
            |row| row.get(0),
        )
        .optional()?)
}

fn validate_archive(archive: &TranscriptArchive, now: SystemTime) -> Result<(), TranscriptError> {
    if archive.format != TRANSCRIPT_ARCHIVE_FORMAT {
        return Err(TranscriptError::InvalidArchive(
            tr("Unsupported format", "format 不受支持").into(),
        ));
    }
    if archive.version != TRANSCRIPT_ARCHIVE_VERSION {
        return Err(TranscriptError::InvalidArchive(match language() {
            Language::English => format!("Version {} is not supported", archive.version),
            Language::SimplifiedChinese => format!("版本 {} 不受支持", archive.version),
        }));
    }
    let now = system_time_timestamp(now)?;
    let exported_at = parse_timestamp(&archive.exported_at)?;
    if exported_at <= 0 || exported_at > now + 86_400 {
        return Err(TranscriptError::InvalidArchive(
            tr("The export time is out of range", "导出时间超出支持范围").into(),
        ));
    }
    if archive.entries.len() > MAX_ARCHIVE_ENTRIES {
        return Err(TranscriptError::InvalidArchive(match language() {
            Language::English => {
                format!("The archive exceeds the limit of {MAX_ARCHIVE_ENTRIES} entries")
            }
            Language::SimplifiedChinese => format!("记录超过 {MAX_ARCHIVE_ENTRIES} 条上限"),
        }));
    }
    let mut ids = std::collections::HashSet::with_capacity(archive.entries.len());
    for entry in &archive.entries {
        if !valid_record_id(&entry.id) {
            return Err(TranscriptError::InvalidArchive(
                tr("Invalid record ID", "记录 ID 无效").into(),
            ));
        }
        if !ids.insert(&entry.id) {
            return Err(TranscriptError::InvalidArchive(
                tr(
                    "The file contains duplicate record IDs",
                    "同一文件含重复记录 ID",
                )
                .into(),
            ));
        }
        let completed_at = parse_timestamp(&entry.completed_at)?;
        if completed_at <= 0 || completed_at > now + 86_400 {
            return Err(TranscriptError::InvalidArchive(
                tr(
                    "The completion time is out of range",
                    "完成时间超出支持范围",
                )
                .into(),
            ));
        }
        let parsed_offset = DateTime::parse_from_rfc3339(&entry.completed_at)
            .map_err(|_| {
                TranscriptError::InvalidArchive(
                    tr("Invalid completion time format", "完成时间格式无效").into(),
                )
            })?
            .offset()
            .local_minus_utc()
            / 60;
        if parsed_offset != entry.utc_offset_minutes || !(-1_440..=1_440).contains(&parsed_offset) {
            return Err(TranscriptError::InvalidArchive(
                tr(
                    "The completion time has an inconsistent time-zone offset",
                    "完成时间的时区偏移不一致",
                )
                .into(),
            ));
        }
        if entry
            .duration_ms
            .is_some_and(|duration| duration == 0 || duration > MAX_DURATION_MS)
        {
            return Err(TranscriptError::InvalidArchive(
                tr("Invalid voice duration", "语音时长无效").into(),
            ));
        }
        if !valid_text(&entry.text) {
            return Err(TranscriptError::InvalidArchive(
                tr("The transcript is empty or too long", "正文为空或过长").into(),
            ));
        }
        if !valid_source(&entry.source) {
            return Err(TranscriptError::InvalidArchive(
                tr("Invalid source ID", "来源 ID 无效").into(),
            ));
        }
        let archived_at = parse_timestamp(&entry.archived_at)?;
        if archived_at <= 0 || archived_at > now + 86_400 {
            return Err(TranscriptError::InvalidArchive(
                tr("The archive time is out of range", "归档时间超出支持范围").into(),
            ));
        }
    }
    Ok(())
}

fn valid_record_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_RECORD_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn valid_source(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SOURCE_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

fn valid_text(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_TRANSCRIPT_BYTES
}

fn valid_handy_file_name(value: &str) -> bool {
    value.len() <= 128
        && value.starts_with("handy-")
        && value.ends_with(".wav")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn local_offset_minutes(timestamp: i64) -> Result<i32, TranscriptError> {
    DateTime::from_timestamp(timestamp, 0)
        .map(|date_time| date_time.with_timezone(&Local).offset().local_minus_utc() / 60)
        .ok_or_else(|| {
            TranscriptError::InvalidArchive(
                tr(
                    "The completion time is out of range",
                    "完成时间超出支持范围",
                )
                .into(),
            )
        })
}

fn format_timestamp(timestamp: i64, offset_minutes: i32) -> String {
    DateTime::from_timestamp(timestamp, 0)
        .and_then(|date_time| {
            chrono::FixedOffset::east_opt(offset_minutes * 60)
                .map(|offset| date_time.with_timezone(&offset))
        })
        .map(|date_time| date_time.to_rfc3339_opts(SecondsFormat::Secs, true))
        .unwrap_or_default()
}

fn parse_timestamp(value: &str) -> Result<i64, TranscriptError> {
    DateTime::parse_from_rfc3339(value)
        .map(|date_time| date_time.timestamp())
        .map_err(|_| {
            TranscriptError::InvalidArchive(
                tr("Invalid RFC 3339 timestamp", "RFC 3339 时间格式无效").into(),
            )
        })
}

fn system_time_timestamp(time: SystemTime) -> Result<i64, TranscriptError> {
    let duration = time.duration_since(UNIX_EPOCH).map_err(|_| {
        TranscriptError::InvalidArchive(tr("Invalid system time", "系统时间无效").into())
    })?;
    i64::try_from(duration.as_secs()).map_err(|_| {
        TranscriptError::InvalidArchive(
            tr("System time is out of range", "系统时间超出范围").into(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const NOW: i64 = 1_787_454_200;

    fn database(temporary: &TempDir) -> TranscriptDatabase {
        TranscriptDatabase::open(temporary.path().join("data/sayall/transcripts.db")).unwrap()
    }

    fn system_time(timestamp: i64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(timestamp as u64)
    }

    fn create_handy_history(path: &Path, schema: i64) -> Connection {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let connection = Connection::open(path).unwrap();
        connection
            .execute_batch(&format!(
                "CREATE TABLE transcription_history (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     file_name TEXT NOT NULL,
                     timestamp INTEGER NOT NULL,
                     saved BOOLEAN NOT NULL DEFAULT 0,
                     title TEXT NOT NULL,
                     transcription_text TEXT NOT NULL,
                     post_processed_text TEXT,
                     post_process_prompt TEXT,
                     post_process_requested BOOLEAN NOT NULL DEFAULT 0
                 );
                 PRAGMA user_version = {schema};"
            ))
            .unwrap();
        connection
    }

    fn sample_archive() -> TranscriptArchive {
        TranscriptArchive {
            format: TRANSCRIPT_ARCHIVE_FORMAT.into(),
            version: TRANSCRIPT_ARCHIVE_VERSION,
            exported_at: "2026-08-23T01:03:20Z".into(),
            entries: vec![TranscriptArchiveEntry {
                id: "fixture:one".into(),
                completed_at: "2026-08-23T09:00:00+08:00".into(),
                utc_offset_minutes: 480,
                duration_ms: Some(2_500),
                text: "合成测试文字".into(),
                source: "handy".into(),
                archived_at: "2026-08-23T01:01:00Z".into(),
            }],
        }
    }

    #[test]
    fn database_is_private_and_archive_is_disabled_by_default() {
        let temporary = TempDir::new().unwrap();
        let path = temporary.path().join("data/sayall/transcripts.db");
        let database = TranscriptDatabase::open(&path).unwrap();

        assert!(!database.archive_enabled().unwrap());
        assert_eq!(database.entry_count().unwrap(), 0);
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn handy_history_is_only_read_after_explicit_enablement() {
        let temporary = TempDir::new().unwrap();
        let history_path = temporary.path().join("handy/history.db");
        let statistics_path = temporary.path().join("statistics.db");
        let statistics = Connection::open(&statistics_path).unwrap();
        statistics
            .execute_batch(
                "CREATE TABLE voice_sessions (
                     source_id TEXT PRIMARY KEY,
                     started_at INTEGER NOT NULL,
                     duration_ms INTEGER NOT NULL
                 );
                 INSERT INTO voice_sessions VALUES (
                     'handy-recording:handy-1787454000.wav', 1787454000, 2500
                 );",
            )
            .unwrap();
        drop(statistics);
        let history = create_handy_history(&history_path, HANDY_HISTORY_SCHEMA_VERSION);
        history
            .execute(
                "INSERT INTO transcription_history (
                     file_name, timestamp, title, transcription_text,
                     post_processed_text, post_process_requested
                 ) VALUES ('handy-1787454000.wav', 1787454001, 'fixture',
                           'raw fixture', '最终合成文字', 1)",
                [],
            )
            .unwrap();
        drop(history);

        let database = database(&temporary);
        assert_eq!(
            database
                .import_handy_history(&history_path, &statistics_path, system_time(NOW),)
                .unwrap(),
            0
        );
        database.set_archive_enabled(true).unwrap();
        assert_eq!(
            database
                .import_handy_history(&history_path, &statistics_path, system_time(NOW),)
                .unwrap(),
            1
        );
        assert_eq!(
            database
                .import_handy_history(&history_path, &statistics_path, system_time(NOW),)
                .unwrap(),
            0
        );
        let archive = database.export_archive(system_time(NOW)).unwrap();
        assert_eq!(archive.entries.len(), 1);
        assert_eq!(archive.entries[0].text, "最终合成文字");
        assert_eq!(archive.entries[0].source, "handy");
        assert_eq!(archive.entries[0].duration_ms, Some(2_500));
    }

    #[test]
    fn empty_post_processed_result_does_not_archive_unpasted_raw_text() {
        let temporary = TempDir::new().unwrap();
        let history_path = temporary.path().join("handy/history.db");
        let history = create_handy_history(&history_path, HANDY_HISTORY_SCHEMA_VERSION);
        history
            .execute(
                "INSERT INTO transcription_history (
                     file_name, timestamp, title, transcription_text,
                     post_processed_text, post_process_requested
                 ) VALUES ('handy-1787454000.wav', 1787454001, 'fixture',
                           'raw fixture', '', 1)",
                [],
            )
            .unwrap();
        drop(history);
        let database = database(&temporary);
        database.set_archive_enabled(true).unwrap();

        assert_eq!(
            database
                .import_handy_history(
                    &history_path,
                    &temporary.path().join("statistics.db"),
                    system_time(NOW),
                )
                .unwrap(),
            0
        );
        assert_eq!(database.entry_count().unwrap(), 0);
    }

    #[test]
    fn incompatible_handy_schema_stops_without_copying_text() {
        let temporary = TempDir::new().unwrap();
        let history_path = temporary.path().join("handy/history.db");
        drop(create_handy_history(&history_path, 5));
        let database = database(&temporary);
        database.set_archive_enabled(true).unwrap();

        assert!(matches!(
            database.import_handy_history(
                &history_path,
                &temporary.path().join("statistics.db"),
                system_time(NOW),
            ),
            Err(TranscriptError::HandySchema(5))
        ));
        assert_eq!(database.entry_count().unwrap(), 0);
    }

    #[test]
    fn archive_round_trip_preserves_text_time_duration_and_source() {
        let temporary = TempDir::new().unwrap();
        let database = database(&temporary);
        let archive = sample_archive();
        assert_eq!(
            database.preview_import(&archive, system_time(NOW)).unwrap(),
            TranscriptImportPreview {
                total: 1,
                new_entries: 1,
                duplicates: 0
            }
        );
        assert_eq!(
            database.import_archive(&archive, system_time(NOW)).unwrap(),
            1
        );
        assert_eq!(
            database.import_archive(&archive, system_time(NOW)).unwrap(),
            0
        );
        let exported = database.export_archive(system_time(NOW)).unwrap();
        assert_eq!(exported.entries, archive.entries);
    }

    #[test]
    fn json_export_is_atomic_private_and_importable() {
        let temporary = TempDir::new().unwrap();
        let path = temporary.path().join("sayall-transcripts.json");
        let archive = sample_archive();
        write_archive_file(&path, &archive).unwrap();

        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(read_archive_file(&path, system_time(NOW)).unwrap(), archive);
        assert!(
            fs::read_dir(temporary.path())
                .unwrap()
                .filter_map(Result::ok)
                .all(|entry| !entry.file_name().to_string_lossy().ends_with(".tmp"))
        );
    }

    #[test]
    fn invalid_or_oversized_archives_are_rejected_before_writes() {
        let temporary = TempDir::new().unwrap();
        let database = database(&temporary);
        let mut archive = sample_archive();
        archive.entries[0].utc_offset_minutes = 0;
        assert!(database.import_archive(&archive, system_time(NOW)).is_err());
        assert_eq!(database.entry_count().unwrap(), 0);

        let path = temporary.path().join("oversized.json");
        let file = File::create(&path).unwrap();
        file.set_len(MAX_ARCHIVE_BYTES + 1).unwrap();
        assert!(read_archive_file(&path, system_time(NOW)).is_err());
    }

    #[test]
    fn clearing_text_does_not_disable_future_opt_in_collection() {
        let temporary = TempDir::new().unwrap();
        let database = database(&temporary);
        database.set_archive_enabled(true).unwrap();
        database
            .import_archive(&sample_archive(), system_time(NOW))
            .unwrap();

        database.clear_at(system_time(NOW)).unwrap();

        assert_eq!(database.entry_count().unwrap(), 0);
        assert!(database.archive_enabled().unwrap());
    }

    #[test]
    fn cleared_handy_text_is_not_reimported_but_newer_text_is() {
        let temporary = TempDir::new().unwrap();
        let history_path = temporary.path().join("handy/history.db");
        let history = create_handy_history(&history_path, HANDY_HISTORY_SCHEMA_VERSION);
        history
            .execute(
                "INSERT INTO transcription_history (
                     file_name, timestamp, title, transcription_text, post_process_requested
                 ) VALUES ('handy-1787454000.wav', 1787454001, 'old', '旧合成文字', 0)",
                [],
            )
            .unwrap();
        let database = database(&temporary);
        database.set_archive_enabled(true).unwrap();
        let statistics_path = temporary.path().join("statistics.db");
        assert_eq!(
            database
                .import_handy_history(&history_path, &statistics_path, system_time(NOW))
                .unwrap(),
            1
        );
        database.clear_at(system_time(NOW)).unwrap();
        assert_eq!(
            database
                .import_handy_history(&history_path, &statistics_path, system_time(NOW + 10))
                .unwrap(),
            0
        );

        history
            .execute(
                "INSERT INTO transcription_history (
                     file_name, timestamp, title, transcription_text, post_process_requested
                 ) VALUES ('handy-1787454210.wav', 1787454210, 'new', '新合成文字', 0)",
                [],
            )
            .unwrap();
        drop(history);
        assert_eq!(
            database
                .import_handy_history(&history_path, &statistics_path, system_time(NOW + 20))
                .unwrap(),
            1
        );
        assert_eq!(database.entry_count().unwrap(), 1);
    }
}
