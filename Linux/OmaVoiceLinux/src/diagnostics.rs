use crate::i18n::{Language, language, tr};
use crate::{DoctorReport, render_json};
use std::env;
use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

const REPORT_FILE_NAME: &str = "omavoice-doctor.json";

#[derive(Debug)]
pub enum DiagnosticError {
    MissingStateHome,
    Io {
        operation: &'static str,
        source: io::Error,
    },
    Json(serde_json::Error),
}

impl DiagnosticError {
    fn io(operation: &'static str, source: io::Error) -> Self {
        Self::Io { operation, source }
    }
}

impl fmt::Display for DiagnosticError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingStateHome => formatter.write_str(tr(
                "Could not determine the XDG state directory",
                "无法确定 XDG 状态目录",
            )),
            Self::Io { operation, source } => match language() {
                Language::English => write!(
                    formatter,
                    "Could not {operation} the OmaVoice diagnostic report: {source}"
                ),
                Language::SimplifiedChinese => {
                    write!(formatter, "无法{operation} OmaVoice 诊断报告：{source}")
                }
            },
            Self::Json(error) => match language() {
                Language::English => write!(
                    formatter,
                    "Could not generate OmaVoice diagnostic JSON: {error}"
                ),
                Language::SimplifiedChinese => {
                    write!(formatter, "无法生成 OmaVoice 诊断 JSON：{error}")
                }
            },
        }
    }
}

impl Error for DiagnosticError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json(source) => Some(source),
            Self::MissingStateHome => None,
        }
    }
}

impl From<serde_json::Error> for DiagnosticError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

pub fn diagnostic_directory_from_environment() -> Result<PathBuf, DiagnosticError> {
    Ok(state_home_from_environment()?.join("omavoice/diagnostics"))
}

pub fn log_directory_from_environment() -> Result<PathBuf, DiagnosticError> {
    Ok(state_home_from_environment()?.join("omavoice/logs"))
}

fn state_home_from_environment() -> Result<PathBuf, DiagnosticError> {
    let base = env::var_os("XDG_STATE_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|path| path.join(".local/state"))
        })
        .ok_or(DiagnosticError::MissingStateHome)?;

    Ok(base)
}

pub fn export_diagnostic_report(
    directory: &Path,
    report: &DoctorReport,
) -> Result<PathBuf, DiagnosticError> {
    fs::create_dir_all(directory)
        .map_err(|error| DiagnosticError::io(tr("create the directory", "创建目录"), error))?;
    let path = directory.join(REPORT_FILE_NAME);
    let temporary = directory.join(format!(".{REPORT_FILE_NAME}.tmp-{}", std::process::id()));
    let mut bytes = render_json(report)?.into_bytes();
    bytes.push(b'\n');

    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|error| {
                DiagnosticError::io(tr("create the temporary file", "创建临时文件"), error)
            })?;
        file.write_all(&bytes)
            .map_err(|error| DiagnosticError::io(tr("write", "写入"), error))?;
        file.sync_all()
            .map_err(|error| DiagnosticError::io(tr("sync", "同步"), error))?;
        fs::rename(&temporary, &path)
            .map_err(|error| DiagnosticError::io(tr("replace", "替换"), error))?;
        Ok(path)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Phase, SystemSnapshot, evaluate};
    use serde_json::Value;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    #[test]
    fn exported_report_is_private_valid_json_without_transient_identity() {
        let directory = tempdir().unwrap();
        let report = evaluate(&SystemSnapshot::default(), Phase::ZeroB);

        let path = export_diagnostic_report(directory.path(), &report).unwrap();
        let bytes = fs::read(&path).unwrap();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        let text = String::from_utf8(bytes).unwrap();

        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["target_phase"], "0b");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(!text.contains("bluetooth_address"));
        assert!(!text.contains("event_node"));
        assert!(!text.contains("pipewire_node_id"));
    }

    #[test]
    fn export_atomically_replaces_report_without_leaving_temporary_files() {
        let directory = tempdir().unwrap();
        let first = evaluate(&SystemSnapshot::default(), Phase::ZeroB);
        let second = evaluate(&SystemSnapshot::default(), Phase::Two);

        export_diagnostic_report(directory.path(), &first).unwrap();
        let path = export_diagnostic_report(directory.path(), &second).unwrap();
        let json: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        let entries = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();

        assert_eq!(json["target_phase"], "2");
        assert_eq!(entries, [REPORT_FILE_NAME]);
    }
}
