use chrono::Utc;
use evdev::{Device, EventSummary, KeyCode};
use omavoice_linux::i18n::{Language, language, tr};
use omavoice_linux::statistics::{StatisticsDatabase, StatisticsPaths, StatisticsPeriod};
use omavoice_linux::transcripts::{TranscriptDatabase, TranscriptPaths};
use std::error::Error;
use std::fs;
use std::io;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

const INPUT_SCAN_INTERVAL: Duration = Duration::from_secs(2);
const RECORDING_SCAN_INTERVAL: Duration = Duration::from_secs(2);
const TRANSCRIPT_ERROR_REPORT_INTERVAL: Duration = Duration::from_secs(60);
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(20);
const KEYD_VIRTUAL_KEYBOARD: &str = "keyd virtual keyboard";
const HANDY_KEYD_PASSTHROUGH: &str = "handy-keys passthrough: keyd virtual keyboard";

fn main() {
    if let Err(error) = run() {
        match language() {
            Language::English => eprintln!("OmaVoice local statistics could not start: {error}"),
            Language::SimplifiedChinese => eprintln!("OmaVoice 本地统计无法启动：{error}"),
        }
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let import_once = match std::env::args().skip(1).collect::<Vec<_>>().as_slice() {
        [] => false,
        [argument] if argument == "--import-once" => true,
        [argument] if argument == "-h" || argument == "--help" => {
            println!(
                "{}",
                tr(
                    "Usage: omavoice-statistics [--import-once]\n\nBy default, continuously collect aggregate local statistics; --import-once only synchronizes metadata for completed Handy WAV files.",
                    "用法：omavoice-statistics [--import-once]\n\n默认持续采集本机聚合统计；--import-once 只同步已完成的 Handy WAV 元数据。"
                )
            );
            return Ok(());
        }
        _ => return Err(tr("Unrecognized argument", "无法识别的参数").into()),
    };

    let paths = StatisticsPaths::from_xdg_environment()?;
    let database = StatisticsDatabase::open(&paths.database)?;
    let imported = database.import_handy_recordings(&paths.handy_recordings, SystemTime::now())?;
    let transcripts = TranscriptPaths::from_xdg_environment().and_then(|paths| {
        TranscriptDatabase::open(&paths.database).map(|database| (database, paths))
    });
    let (transcripts, imported_transcripts) = match transcripts {
        Ok((database, paths)) => match database.import_handy_history(
            &paths.handy_history,
            &paths.statistics_database,
            SystemTime::now(),
        ) {
            Ok(imported) => (Some((database, paths)), imported),
            Err(error) => {
                report_transcript_unavailable(&error);
                (Some((database, paths)), 0)
            }
        },
        Err(error) => {
            report_transcript_unavailable(&error);
            (None, 0)
        }
    };
    if import_once {
        let summary = database.summary(StatisticsPeriod::All, Utc::now().timestamp())?;
        match language() {
            Language::English => println!(
                "Synchronized metadata for {imported} Handy recordings and {imported_transcripts} authorized transcripts; total: {} voice sessions and {} ms.",
                summary.voice_sessions, summary.voice_duration_ms
            ),
            Language::SimplifiedChinese => println!(
                "已同步 {imported} 个 Handy 录音元数据、{imported_transcripts} 条已授权文字；累计 {} 次语音、{} 毫秒。",
                summary.voice_sessions, summary.voice_duration_ms
            ),
        }
        return Ok(());
    }

    println!(
        "{}",
        tr(
            "OmaVoice local statistics started. The transcript archive is off by default; when enabled, completed Handy transcripts are synchronized read-only.",
            "OmaVoice 本地统计已启动；文字档案默认关闭，启用后才只读同步 Handy 最终文字。"
        )
    );
    collect(database, &paths.handy_recordings, transcripts)
}

fn collect(
    database: StatisticsDatabase,
    handy_recordings: &Path,
    transcripts: Option<(TranscriptDatabase, TranscriptPaths)>,
) -> Result<(), Box<dyn Error>> {
    let mut input: Option<StatisticsInput> = None;
    let mut last_input_scan = Instant::now() - INPUT_SCAN_INTERVAL;
    let mut last_recording_scan = Instant::now();
    let mut last_transcript_error = Instant::now();

    loop {
        if input.is_none() && last_input_scan.elapsed() >= INPUT_SCAN_INTERVAL {
            input = discover_statistics_input()?;
            last_input_scan = Instant::now();
            if let Some(input) = input.as_ref() {
                match language() {
                    Language::English => println!(
                        "Connected to aggregate remote-control button source: {}",
                        input.device_name
                    ),
                    Language::SimplifiedChinese => {
                        println!("已连接遥控器聚合按键源：{}", input.device_name)
                    }
                }
            }
        }

        let mut input_disconnected = false;
        if let Some(source) = input.as_mut() {
            match source.device.fetch_events() {
                Ok(events) => {
                    for event in events {
                        if let EventSummary::Key(_, key, 1) = event.destructure()
                            && let Some(key) = statistics_key(key)
                        {
                            database.record_button_at(Utc::now().timestamp(), &key)?;
                        }
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => {
                    match language() {
                        Language::English => eprintln!(
                            "The aggregate remote-control button source disconnected and will be rediscovered automatically: {error}"
                        ),
                        Language::SimplifiedChinese => {
                            eprintln!("遥控器聚合按键源已断开，将自动重新发现：{error}")
                        }
                    }
                    input_disconnected = true;
                }
            };
        }
        if input_disconnected {
            input = None;
            last_input_scan = Instant::now() - INPUT_SCAN_INTERVAL;
        }

        if last_recording_scan.elapsed() >= RECORDING_SCAN_INTERVAL {
            if let Err(error) =
                database.import_handy_recordings(handy_recordings, SystemTime::now())
            {
                match language() {
                    Language::English => eprintln!(
                        "Handy recording metadata cannot be synchronized right now; retrying later: {error}"
                    ),
                    Language::SimplifiedChinese => {
                        eprintln!("暂时无法同步 Handy 录音元数据，稍后重试：{error}")
                    }
                }
            }
            if let Some((database, paths)) = transcripts.as_ref()
                && let Err(error) = database.import_handy_history(
                    &paths.handy_history,
                    &paths.statistics_database,
                    SystemTime::now(),
                )
                && last_transcript_error.elapsed() >= TRANSCRIPT_ERROR_REPORT_INTERVAL
            {
                match language() {
                    Language::English => eprintln!(
                        "Authorized Handy transcripts cannot be synchronized right now; anonymous statistics will continue: {error}"
                    ),
                    Language::SimplifiedChinese => {
                        eprintln!("暂时无法同步已授权的 Handy 文字档案；匿名统计继续运行：{error}")
                    }
                }
                last_transcript_error = Instant::now();
            }
            last_recording_scan = Instant::now();
        }
        thread::sleep(EVENT_POLL_INTERVAL);
    }
}

fn report_transcript_unavailable(error: &dyn Error) {
    match language() {
        Language::English => eprintln!(
            "The transcript archive is temporarily unavailable; anonymous statistics will continue: {error}"
        ),
        Language::SimplifiedChinese => eprintln!("文字档案暂时不可用；匿名统计将继续运行：{error}"),
    }
}

struct StatisticsInput {
    device: Device,
    device_name: &'static str,
}

fn discover_statistics_input() -> io::Result<Option<StatisticsInput>> {
    let entries = match fs::read_dir("/dev/input") {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut candidates = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if !is_event_path(&path) {
            continue;
        }
        let Ok(device) = Device::open(&path) else {
            continue;
        };
        let Some(priority) = device.name().and_then(statistics_input_priority) else {
            continue;
        };
        candidates.push((priority, path, device));
    }
    candidates.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    let Some((priority, _, device)) = candidates.into_iter().next() else {
        return Ok(None);
    };
    device.set_nonblocking(true)?;
    Ok(Some(StatisticsInput {
        device,
        device_name: if priority == 2 {
            HANDY_KEYD_PASSTHROUGH
        } else {
            KEYD_VIRTUAL_KEYBOARD
        },
    }))
}

fn is_event_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.strip_prefix("event").is_some_and(|suffix| {
                !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
            })
        })
}

fn statistics_input_priority(name: &str) -> Option<u8> {
    match name {
        HANDY_KEYD_PASSTHROUGH => Some(2),
        KEYD_VIRTUAL_KEYBOARD => Some(1),
        _ => None,
    }
}

fn statistics_key(key: KeyCode) -> Option<String> {
    if matches!(
        key,
        KeyCode::KEY_LEFTCTRL
            | KeyCode::KEY_RIGHTCTRL
            | KeyCode::KEY_LEFTALT
            | KeyCode::KEY_RIGHTALT
            | KeyCode::KEY_LEFTSHIFT
            | KeyCode::KEY_RIGHTSHIFT
            | KeyCode::KEY_LEFTMETA
            | KeyCode::KEY_RIGHTMETA
            | KeyCode::KEY_F20
    ) {
        return None;
    }
    let debug = format!("{key:?}");
    let key = debug.strip_prefix("KEY_")?.to_ascii_lowercase();
    (!key.is_empty()
        && key.len() <= 48
        && key
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'))
    .then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_stable_keyd_output_names_are_candidates() {
        assert_eq!(statistics_input_priority(HANDY_KEYD_PASSTHROUGH), Some(2));
        assert_eq!(statistics_input_priority(KEYD_VIRTUAL_KEYBOARD), Some(1));
        assert_eq!(
            statistics_input_priority("handy-keys passthrough: AT Translated Set 2 keyboard"),
            None
        );
        assert_eq!(statistics_input_priority("小米蓝牙语音遥控器"), None);
    }

    #[test]
    fn button_counter_ignores_modifiers_and_reserved_ptt() {
        for ignored in [
            KeyCode::KEY_LEFTCTRL,
            KeyCode::KEY_RIGHTALT,
            KeyCode::KEY_LEFTSHIFT,
            KeyCode::KEY_LEFTMETA,
            KeyCode::KEY_F20,
        ] {
            assert_eq!(statistics_key(ignored), None);
        }
        assert_eq!(statistics_key(KeyCode::KEY_UP).as_deref(), Some("up"));
        assert_eq!(statistics_key(KeyCode::KEY_ENTER).as_deref(), Some("enter"));
        assert_eq!(statistics_key(KeyCode::KEY_P).as_deref(), Some("p"));
    }

    #[test]
    fn event_path_detection_does_not_persist_transient_event_numbers() {
        assert!(is_event_path(Path::new("/dev/input/event17")));
        assert!(!is_event_path(Path::new("/dev/input/mouse1")));
        assert!(!is_event_path(Path::new("/dev/input/event")));
    }
}
