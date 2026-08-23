use crate::i18n::{Language, language, tr};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::env;
use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

pub const CONFIG_SCHEMA_VERSION: u32 = 2;

const SHORTCUT_KEYS: [&str; 89] = [
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "a",
    "b",
    "c",
    "d",
    "e",
    "f",
    "g",
    "h",
    "i",
    "j",
    "k",
    "l",
    "m",
    "n",
    "o",
    "p",
    "q",
    "r",
    "s",
    "t",
    "u",
    "v",
    "w",
    "x",
    "y",
    "z",
    "f1",
    "f2",
    "f3",
    "f4",
    "f5",
    "f6",
    "f7",
    "f8",
    "f9",
    "f10",
    "f11",
    "f12",
    "f13",
    "f14",
    "f15",
    "f16",
    "f17",
    "f18",
    "f19",
    "f21",
    "f22",
    "f23",
    "f24",
    "esc",
    "enter",
    "tab",
    "space",
    "backspace",
    "delete",
    "insert",
    "home",
    "end",
    "pageup",
    "pagedown",
    "up",
    "down",
    "left",
    "right",
    "minus",
    "equal",
    "leftbrace",
    "rightbrace",
    "semicolon",
    "apostrophe",
    "grave",
    "backslash",
    "comma",
    "dot",
    "slash",
    "mute",
    "volumeup",
    "volumedown",
    "playpause",
];

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlSource {
    #[default]
    Evdev,
    Disabled,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceSource {
    #[default]
    Atvv,
    #[serde(rename = "pipewire", alias = "pipe_wire")]
    PipeWire,
    Disabled,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    #[default]
    Ble,
    BluetoothClassic,
    Usb,
    #[serde(rename = "receiver_2_4_ghz", alias = "receiver2_4_ghz")]
    Receiver2_4Ghz,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteButton {
    Power,
    Up,
    Left,
    Ok,
    Right,
    Down,
    Back,
    VolumeUp,
    Home,
    VolumeDown,
    Menu,
    Tv,
}

pub const REMOTE_BUTTONS: [RemoteButton; 12] = [
    RemoteButton::Power,
    RemoteButton::Up,
    RemoteButton::Left,
    RemoteButton::Ok,
    RemoteButton::Right,
    RemoteButton::Down,
    RemoteButton::Back,
    RemoteButton::VolumeUp,
    RemoteButton::Home,
    RemoteButton::VolumeDown,
    RemoteButton::Menu,
    RemoteButton::Tv,
];

impl RemoteButton {
    pub const fn default_action(self) -> ButtonAction {
        match self {
            Self::Power => ButtonAction::Escape,
            Self::Up => ButtonAction::ArrowUp,
            Self::Left => ButtonAction::ArrowLeft,
            Self::Ok => ButtonAction::Enter,
            Self::Right => ButtonAction::ArrowRight,
            Self::Down => ButtonAction::ArrowDown,
            Self::Back => ButtonAction::Backspace,
            Self::VolumeUp => ButtonAction::VolumeUp,
            Self::Home => ButtonAction::ShowDesktop,
            Self::VolumeDown => ButtonAction::VolumeDown,
            Self::Menu => ButtonAction::ContextMenu,
            Self::Tv => ButtonAction::AppSwitcher,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonAction {
    #[default]
    Disabled,
    PassThrough,
    Escape,
    Enter,
    Backspace,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ShowDesktop,
    ContextMenu,
    AppSwitcher,
    VolumeUp,
    VolumeDown,
    VolumeMute,
    PlayPause,
    CustomShortcut,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct KeyboardShortcut {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    #[serde(rename = "super")]
    pub super_key: bool,
    pub key: String,
}

impl KeyboardShortcut {
    pub fn from_input(
        control: bool,
        alt: bool,
        shift: bool,
        super_key: bool,
        key: &str,
    ) -> Result<Self, String> {
        let key = normalize_shortcut_key(key)
            .ok_or_else(|| match language() {
                Language::English => format!("Unsupported primary key: “{}”", key.trim()),
                Language::SimplifiedChinese => format!("不支持主键“{}”", key.trim()),
            })?
            .to_string();
        let shortcut = Self {
            control,
            alt,
            shift,
            super_key,
            key,
        };
        shortcut.validate()?;
        Ok(shortcut)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.key.eq_ignore_ascii_case("f20") {
            return Err(tr(
                "F20 is reserved for push-to-talk",
                "F20 已保留给 Mic 按住说话",
            )
            .into());
        }
        if !is_supported_shortcut_key(&self.key) {
            return Err(match language() {
                Language::English => format!("Unsupported primary key: “{}”", self.key),
                Language::SimplifiedChinese => format!("不支持主键“{}”", self.key),
            });
        }
        Ok(())
    }

    pub fn keyd_binding(&self) -> Result<String, String> {
        self.validate()?;
        let mut binding = String::new();
        for (enabled, prefix) in [
            (self.control, "C-"),
            (self.alt, "A-"),
            (self.shift, "S-"),
            (self.super_key, "M-"),
        ] {
            if enabled {
                binding.push_str(prefix);
            }
        }
        binding.push_str(&self.key);
        Ok(binding)
    }

    pub fn display_name(&self) -> String {
        let mut parts = Vec::new();
        if self.control {
            parts.push("Ctrl".to_string());
        }
        if self.alt {
            parts.push("Alt".to_string());
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        if self.super_key {
            parts.push("Super".to_string());
        }
        parts.push(shortcut_key_label(&self.key));
        parts.join(" + ")
    }
}

pub fn is_supported_shortcut_key(key: &str) -> bool {
    SHORTCUT_KEYS.contains(&key)
}

fn normalize_shortcut_key(input: &str) -> Option<&'static str> {
    let normalized = input.trim().to_ascii_lowercase().replace([' ', '_'], "");
    let alias = match normalized.as_str() {
        "escape" => "esc",
        "return" => "enter",
        "del" => "delete",
        "ins" => "insert",
        "pgup" => "pageup",
        "pgdn" | "pgdown" => "pagedown",
        "volup" => "volumeup",
        "voldown" => "volumedown",
        "play" | "pause" | "playpause" => "playpause",
        "-" => "minus",
        "=" => "equal",
        "," => "comma",
        "." => "dot",
        "/" => "slash",
        ";" => "semicolon",
        "'" => "apostrophe",
        "`" => "grave",
        "\\" => "backslash",
        "[" => "leftbrace",
        "]" => "rightbrace",
        value => value,
    };
    SHORTCUT_KEYS.iter().copied().find(|key| *key == alias)
}

fn shortcut_key_label(key: &str) -> String {
    match key {
        "esc" => "Esc".into(),
        "enter" => "Enter".into(),
        "tab" => "Tab".into(),
        "space" => "Space".into(),
        "backspace" => "Backspace".into(),
        "delete" => "Delete".into(),
        "insert" => "Insert".into(),
        "home" => "Home".into(),
        "end" => "End".into(),
        "pageup" => "Page Up".into(),
        "pagedown" => "Page Down".into(),
        "minus" => "-".into(),
        "equal" => "=".into(),
        "leftbrace" => "[".into(),
        "rightbrace" => "]".into(),
        "semicolon" => ";".into(),
        "apostrophe" => "'".into(),
        "grave" => "`".into(),
        "backslash" => "\\".into(),
        "comma" => ",".into(),
        "dot" => ".".into(),
        "slash" => "/".into(),
        "up" => "↑".into(),
        "down" => "↓".into(),
        "left" => "←".into(),
        "right" => "→".into(),
        "volumeup" => "Volume Up".into(),
        "volumedown" => "Volume Down".into(),
        "mute" => "Mute".into(),
        "playpause" => "Play/Pause".into(),
        value if value.len() == 1 => value.to_ascii_uppercase(),
        value if value.starts_with('f') => value.to_ascii_uppercase(),
        value => value.to_string(),
    }
}

fn default_button_mappings() -> BTreeMap<RemoteButton, ButtonAction> {
    REMOTE_BUTTONS
        .into_iter()
        .map(|button| (button, button.default_action()))
        .collect()
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct DeviceMatcher {
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
    pub device_name: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Default for DeviceMatcher {
    fn default() -> Self {
        Self {
            vendor_id: Some(0x2717),
            product_id: Some(0x32b8),
            device_name: Some("小米蓝牙语音遥控器".into()),
            extra: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct DeviceProfile {
    pub id: String,
    pub display_name: String,
    pub matcher: DeviceMatcher,
    pub control_source: ControlSource,
    pub voice_source: VoiceSource,
    pub transport: Transport,
    pub ptt_key: String,
    #[serde(default = "default_button_mappings")]
    pub button_mappings: BTreeMap<RemoteButton, ButtonAction>,
    pub button_shortcuts: BTreeMap<RemoteButton, KeyboardShortcut>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Default for DeviceProfile {
    fn default() -> Self {
        Self {
            id: "xiaomi-rc003".into(),
            display_name: tr("Xiaomi Bluetooth Voice Remote", "小米蓝牙语音遥控器").into(),
            matcher: DeviceMatcher::default(),
            control_source: ControlSource::Evdev,
            voice_source: VoiceSource::Atvv,
            transport: Transport::Ble,
            ptt_key: "F20".into(),
            button_mappings: default_button_mappings(),
            button_shortcuts: BTreeMap::new(),
            extra: BTreeMap::new(),
        }
    }
}

impl DeviceProfile {
    pub fn button_action(&self, button: RemoteButton) -> ButtonAction {
        self.button_mappings
            .get(&button)
            .copied()
            .unwrap_or_else(|| button.default_action())
    }

    pub fn button_shortcut(&self, button: RemoteButton) -> Option<&KeyboardShortcut> {
        self.button_shortcuts.get(&button)
    }

    fn normalize_button_mappings(&mut self) {
        for button in REMOTE_BUTTONS {
            self.button_mappings
                .entry(button)
                .or_insert_with(|| button.default_action());
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct LinuxConfig {
    pub schema_version: u32,
    pub selected_profile_id: Option<String>,
    pub profiles: Vec<DeviceProfile>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Default for LinuxConfig {
    fn default() -> Self {
        let profile = DeviceProfile::default();
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            selected_profile_id: Some(profile.id.clone()),
            profiles: vec![profile],
            extra: BTreeMap::new(),
        }
    }
}

impl LinuxConfig {
    pub fn selected_profile(&self) -> Option<&DeviceProfile> {
        let selected_id = self.selected_profile_id.as_deref()?;
        self.profiles
            .iter()
            .find(|profile| profile.id == selected_id)
    }

    pub fn selected_profile_mut(&mut self) -> Option<&mut DeviceProfile> {
        let selected_id = self.selected_profile_id.as_deref()?;
        self.profiles
            .iter_mut()
            .find(|profile| profile.id == selected_id)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.schema_version != CONFIG_SCHEMA_VERSION {
            return Err(ConfigError::UnsupportedSchema(self.schema_version));
        }

        let mut ids = HashSet::new();
        for profile in &self.profiles {
            if profile.id.trim().is_empty() {
                return Err(ConfigError::Validation(
                    tr(
                        "Device profile ID cannot be empty",
                        "设备 Profile ID 不能为空",
                    )
                    .into(),
                ));
            }
            if !ids.insert(profile.id.as_str()) {
                return Err(ConfigError::Validation(match language() {
                    Language::English => format!("Duplicate device profile ID: {}", profile.id),
                    Language::SimplifiedChinese => format!("设备 Profile ID 重复：{}", profile.id),
                }));
            }
            if profile.display_name.trim().is_empty() {
                return Err(ConfigError::Validation(match language() {
                    Language::English => format!(
                        "The display name for device profile “{}” cannot be empty",
                        profile.id
                    ),
                    Language::SimplifiedChinese => {
                        format!("设备 Profile“{}”的显示名称不能为空", profile.id)
                    }
                }));
            }
            if profile.ptt_key.trim().is_empty() {
                return Err(ConfigError::Validation(match language() {
                    Language::English => format!(
                        "The PTT key for device profile “{}” cannot be empty",
                        profile.id
                    ),
                    Language::SimplifiedChinese => {
                        format!("设备 Profile“{}”的 PTT 键不能为空", profile.id)
                    }
                }));
            }
            for (button, shortcut) in &profile.button_shortcuts {
                shortcut.validate().map_err(|error| {
                    ConfigError::Validation(match language() {
                        Language::English => format!(
                            "Invalid {:?} shortcut for device profile “{}”: {error}",
                            button, profile.id
                        ),
                        Language::SimplifiedChinese => format!(
                            "设备 Profile“{}”的 {:?} 快捷键无效：{error}",
                            profile.id, button
                        ),
                    })
                })?;
            }
            for button in REMOTE_BUTTONS {
                if profile.button_action(button) == ButtonAction::CustomShortcut
                    && profile.button_shortcut(button).is_none()
                {
                    return Err(ConfigError::Validation(match language() {
                        Language::English => format!(
                            "No custom shortcut is set for {:?} in device profile “{}”",
                            button, profile.id
                        ),
                        Language::SimplifiedChinese => format!(
                            "设备 Profile“{}”的 {:?} 尚未设置自定义快捷键",
                            profile.id, button
                        ),
                    }));
                }
            }
        }

        if let Some(selected_id) = self.selected_profile_id.as_deref()
            && !ids.contains(selected_id)
        {
            return Err(ConfigError::Validation(match language() {
                Language::English => {
                    format!("The selected device profile does not exist: {selected_id}")
                }
                Language::SimplifiedChinese => format!("当前设备 Profile 不存在：{selected_id}"),
            }));
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum ConfigError {
    MissingConfigHome,
    Io {
        operation: &'static str,
        source: io::Error,
    },
    Json(serde_json::Error),
    UnsupportedSchema(u32),
    Validation(String),
}

impl ConfigError {
    fn io(operation: &'static str, source: io::Error) -> Self {
        Self::Io { operation, source }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingConfigHome => formatter.write_str(tr(
                "Could not determine the XDG configuration directory",
                "无法确定 XDG 配置目录",
            )),
            Self::Io { operation, source } => match language() {
                Language::English => write!(
                    formatter,
                    "Could not {operation} the OmaVoice configuration: {source}"
                ),
                Language::SimplifiedChinese => {
                    write!(formatter, "无法{operation} OmaVoice 配置：{source}")
                }
            },
            Self::Json(error) => match language() {
                Language::English => write!(
                    formatter,
                    "The OmaVoice configuration is not valid JSON: {error}"
                ),
                Language::SimplifiedChinese => {
                    write!(formatter, "OmaVoice 配置不是有效 JSON：{error}")
                }
            },
            Self::UnsupportedSchema(version) => match language() {
                Language::English => write!(
                    formatter,
                    "Unsupported OmaVoice configuration schema {version}; this version supports {CONFIG_SCHEMA_VERSION}"
                ),
                Language::SimplifiedChinese => write!(
                    formatter,
                    "不支持 OmaVoice 配置 Schema {version}；当前支持 {CONFIG_SCHEMA_VERSION}"
                ),
            },
            Self::Validation(message) => match language() {
                Language::English => write!(formatter, "Invalid OmaVoice configuration: {message}"),
                Language::SimplifiedChinese => write!(formatter, "OmaVoice 配置无效：{message}"),
            },
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json(source) => Some(source),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for ConfigError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Clone, Debug)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn from_xdg_environment() -> Result<Self, ConfigError> {
        let base = env::var_os("XDG_CONFIG_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| {
                env::var_os("HOME")
                    .filter(|value| !value.is_empty())
                    .map(PathBuf::from)
                    .filter(|path| path.is_absolute())
                    .map(|path| path.join(".config"))
            })
            .ok_or(ConfigError::MissingConfigHome)?;

        Ok(Self::from_path(base.join("sayall/settings.json")))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn exists(&self) -> bool {
        self.path.is_file()
    }

    pub fn load(&self) -> Result<LinuxConfig, ConfigError> {
        let content = match fs::read_to_string(&self.path) {
            Ok(content) => content,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(LinuxConfig::default());
            }
            Err(error) => return Err(ConfigError::io(tr("read", "读取"), error)),
        };
        let value: Value = serde_json::from_str(&content)?;
        let source_schema = value
            .get("schema_version")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if source_schema > u64::from(CONFIG_SCHEMA_VERSION) {
            return Err(ConfigError::UnsupportedSchema(
                u32::try_from(source_schema).unwrap_or(u32::MAX),
            ));
        }

        let mut config: LinuxConfig = serde_json::from_value(value)?;
        if source_schema < u64::from(CONFIG_SCHEMA_VERSION) {
            config.schema_version = CONFIG_SCHEMA_VERSION;
        }
        for profile in &mut config.profiles {
            profile.normalize_button_mappings();
        }
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, config: &LinuxConfig) -> Result<(), ConfigError> {
        config.validate()?;
        let mut bytes = serde_json::to_vec_pretty(config)?;
        bytes.push(b'\n');

        let parent = self.path.parent().ok_or_else(|| {
            ConfigError::Validation(
                tr(
                    "The configuration file must be located in a directory",
                    "配置文件必须位于一个目录内",
                )
                .into(),
            )
        })?;
        fs::create_dir_all(parent)
            .map_err(|error| ConfigError::io(tr("create the directory for", "创建目录"), error))?;

        let file_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                ConfigError::Validation(
                    tr(
                        "The configuration file name is not valid UTF-8",
                        "配置文件名不是有效 UTF-8",
                    )
                    .into(),
                )
            })?;
        let temporary = parent.join(format!(".{file_name}.tmp-{}", std::process::id()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&temporary)
                .map_err(|error| {
                    ConfigError::io(tr("create a temporary file for", "创建临时文件"), error)
                })?;
            file.write_all(&bytes)
                .map_err(|error| ConfigError::io(tr("write", "写入"), error))?;
            file.sync_all()
                .map_err(|error| ConfigError::io(tr("sync", "同步"), error))?;
            fs::rename(&temporary, &self.path)
                .map_err(|error| ConfigError::io(tr("replace", "替换"), error))?;
            Ok(())
        })();

        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    #[test]
    fn missing_file_loads_a_stable_rc003_default() {
        let directory = tempdir().unwrap();
        let store = ConfigStore::from_path(directory.path().join("settings.json"));

        let config = store.load().unwrap();

        let profile = config.selected_profile().unwrap();
        assert_eq!(config.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(profile.id, "xiaomi-rc003");
        assert_eq!(profile.matcher.vendor_id, Some(0x2717));
        assert_eq!(profile.matcher.product_id, Some(0x32b8));
        assert_eq!(profile.control_source, ControlSource::Evdev);
        assert_eq!(profile.voice_source, VoiceSource::Atvv);
        assert_eq!(profile.transport, Transport::Ble);
        assert_eq!(profile.ptt_key, "F20");
        assert_eq!(profile.button_mappings.len(), REMOTE_BUTTONS.len());
        assert_eq!(profile.button_action(RemoteButton::Ok), ButtonAction::Enter);
        assert_eq!(
            profile.button_action(RemoteButton::Home),
            ButtonAction::ShowDesktop
        );
    }

    #[test]
    fn schema_zero_or_missing_fields_migrate_to_current_defaults() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(
            &path,
            r#"{
                "schema_version": 0,
                "selected_profile_id": "xiaomi-rc003",
                "profiles": [{"display_name": "客厅遥控器"}]
            }"#,
        )
        .unwrap();

        let config = ConfigStore::from_path(path).load().unwrap();
        let profile = config.selected_profile().unwrap();

        assert_eq!(config.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(profile.display_name, "客厅遥控器");
        assert_eq!(profile.control_source, ControlSource::Evdev);
        assert_eq!(profile.voice_source, VoiceSource::Atvv);
        assert_eq!(profile.button_mappings.len(), REMOTE_BUTTONS.len());
    }

    #[test]
    fn partial_button_mappings_fill_stable_defaults_without_overwriting_choices() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(
            &path,
            r#"{
                "schema_version": 1,
                "selected_profile_id": "xiaomi-rc003",
                "profiles": [{
                    "button_mappings": {
                        "ok": "play_pause",
                        "power": "disabled"
                    }
                }]
            }"#,
        )
        .unwrap();

        let config = ConfigStore::from_path(path).load().unwrap();
        let profile = config.selected_profile().unwrap();

        assert_eq!(profile.button_mappings.len(), REMOTE_BUTTONS.len());
        assert_eq!(
            profile.button_action(RemoteButton::Ok),
            ButtonAction::PlayPause
        );
        assert_eq!(
            profile.button_action(RemoteButton::Power),
            ButtonAction::Disabled
        );
        assert_eq!(
            profile.button_action(RemoteButton::Left),
            ButtonAction::ArrowLeft
        );
        assert_eq!(config.schema_version, CONFIG_SCHEMA_VERSION);
        assert!(profile.button_shortcuts.is_empty());
    }

    #[test]
    fn custom_shortcut_normalizes_persists_and_renders_canonical_keyd_syntax() {
        let shortcut = KeyboardShortcut::from_input(true, true, true, true, " Page Up ").unwrap();

        assert_eq!(shortcut.key, "pageup");
        assert_eq!(shortcut.keyd_binding().unwrap(), "C-A-S-M-pageup");
        assert_eq!(
            shortcut.display_name(),
            "Ctrl + Alt + Shift + Super + Page Up"
        );

        let mut config = LinuxConfig::default();
        let profile = config.selected_profile_mut().unwrap();
        profile
            .button_mappings
            .insert(RemoteButton::Tv, ButtonAction::CustomShortcut);
        profile
            .button_shortcuts
            .insert(RemoteButton::Tv, shortcut.clone());
        config.validate().unwrap();
        let restored: LinuxConfig =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        assert_eq!(
            restored
                .selected_profile()
                .unwrap()
                .button_shortcut(RemoteButton::Tv),
            Some(&shortcut)
        );
    }

    #[test]
    fn custom_shortcut_rejects_reserved_or_executable_keyd_syntax() {
        for key in [
            "F20",
            "macro(C-t enter)",
            "command(evil)",
            "C-A-delete",
            "+",
        ] {
            assert!(KeyboardShortcut::from_input(false, false, false, false, key).is_err());
        }

        let mut config = LinuxConfig::default();
        config
            .selected_profile_mut()
            .unwrap()
            .button_mappings
            .insert(RemoteButton::Tv, ButtonAction::CustomShortcut);
        assert!(config.validate().is_err());
    }

    #[test]
    fn custom_shortcut_accepts_common_punctuation_as_safe_named_keys() {
        for (input, expected) in [
            ("-", "minus"),
            ("=", "equal"),
            ("[", "leftbrace"),
            ("]", "rightbrace"),
            ("\\", "backslash"),
            (";", "semicolon"),
            ("'", "apostrophe"),
            ("`", "grave"),
            (",", "comma"),
            (".", "dot"),
            ("/", "slash"),
        ] {
            let shortcut = KeyboardShortcut::from_input(true, false, false, false, input).unwrap();
            assert_eq!(shortcut.key, expected);
            assert_eq!(shortcut.keyd_binding().unwrap(), format!("C-{expected}"));
        }
    }

    #[test]
    fn unknown_fields_survive_load_and_atomic_save() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(
            &path,
            r#"{
                "schema_version": 1,
                "selected_profile_id": "xiaomi-rc003",
                "future_setting": {"enabled": true},
                "profiles": [{
                    "future_profile_setting": "kept",
                    "matcher": {"future_matcher": 42}
                }]
            }"#,
        )
        .unwrap();
        let store = ConfigStore::from_path(&path);

        let config = store.load().unwrap();
        store.save(&config).unwrap();
        let saved: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();

        assert_eq!(saved["future_setting"]["enabled"], true);
        assert_eq!(saved["profiles"][0]["future_profile_setting"], "kept");
        assert_eq!(saved["profiles"][0]["matcher"]["future_matcher"], 42);
    }

    #[test]
    fn atomic_save_uses_private_permissions_and_leaves_no_temporary_file() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config/sayall/settings.json");
        let store = ConfigStore::from_path(&path);

        store.save(&LinuxConfig::default()).unwrap();

        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(store.load().unwrap(), LinuxConfig::default());
        let parent_entries = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(parent_entries, ["settings.json"]);
    }

    #[test]
    fn invalid_save_does_not_replace_the_last_valid_file() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let store = ConfigStore::from_path(&path);
        store.save(&LinuxConfig::default()).unwrap();
        let original = fs::read(&path).unwrap();
        let invalid = LinuxConfig {
            selected_profile_id: Some("missing-profile".into()),
            ..Default::default()
        };

        assert!(store.save(&invalid).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn corrupted_or_future_config_is_not_overwritten_during_load() {
        let directory = tempdir().unwrap();
        for content in ["not json", r#"{"schema_version": 99}"#] {
            let path = directory.path().join(format!("{}.json", content.len()));
            fs::write(&path, content).unwrap();
            let before = fs::read(&path).unwrap();

            assert!(ConfigStore::from_path(&path).load().is_err());
            assert_eq!(fs::read(&path).unwrap(), before);
        }
    }

    #[test]
    fn serialized_profile_contains_no_transient_device_identity() {
        let json = serde_json::to_string(&LinuxConfig::default()).unwrap();

        assert!(!json.contains("bluetooth_address"));
        assert!(!json.contains("event_node"));
        assert!(!json.contains("pipewire_node_id"));
        assert!(!json.contains("/dev/input/event"));
    }

    #[test]
    fn public_enum_values_use_stable_component_names() {
        assert_eq!(
            serde_json::to_string(&VoiceSource::PipeWire).unwrap(),
            "\"pipewire\""
        );
        assert_eq!(
            serde_json::to_string(&Transport::Receiver2_4Ghz).unwrap(),
            "\"receiver_2_4_ghz\""
        );
        assert_eq!(
            serde_json::from_str::<VoiceSource>("\"pipe_wire\"").unwrap(),
            VoiceSource::PipeWire
        );
        assert_eq!(
            serde_json::to_string(&RemoteButton::VolumeUp).unwrap(),
            "\"volume_up\""
        );
        assert_eq!(
            serde_json::to_string(&ButtonAction::ShowDesktop).unwrap(),
            "\"show_desktop\""
        );
    }
}
