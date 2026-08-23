use serde::Serialize;
use std::env;
use std::fmt;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::process::Command;
use std::str::FromStr;

use crate::i18n::tr;

pub mod config;
pub mod diagnostics;
pub mod i18n;
pub mod keyd;
pub mod keyd_apply;
pub mod statistics;
pub mod transcripts;

pub const BLOCKED_EXIT_CODE: i32 = 2;
pub const USAGE_EXIT_CODE: i32 = 64;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Phase {
    #[serde(rename = "0a")]
    ZeroA,
    #[serde(rename = "0b")]
    ZeroB,
    #[serde(rename = "1")]
    One,
    #[serde(rename = "2")]
    Two,
    #[serde(rename = "3")]
    Three,
    #[serde(rename = "4")]
    Four,
    #[serde(rename = "5")]
    Five,
    #[serde(rename = "6")]
    Six,
}

impl fmt::Display for Phase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ZeroA => "0a",
            Self::ZeroB => "0b",
            Self::One => "1",
            Self::Two => "2",
            Self::Three => "3",
            Self::Four => "4",
            Self::Five => "5",
            Self::Six => "6",
        })
    }
}

impl FromStr for Phase {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "0a" => Ok(Self::ZeroA),
            "0b" => Ok(Self::ZeroB),
            "1" => Ok(Self::One),
            "2" => Ok(Self::Two),
            "3" => Ok(Self::Three),
            "4" => Ok(Self::Four),
            "5" => Ok(Self::Five),
            "6" => Ok(Self::Six),
            _ => Err(format!(
                "{} {value}; {}: 0a, 0b, 1, 2, 3, 4, 5, 6",
                tr("Unsupported phase", "不支持的阶段"),
                tr("valid values", "可选值")
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DoctorOptions {
    pub json: bool,
    pub phase: Phase,
}

impl Default for DoctorOptions {
    fn default() -> Self {
        Self {
            json: false,
            phase: Phase::ZeroA,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CliAction {
    Run(DoctorOptions),
    Help,
}

pub fn parse_args<I, S>(arguments: I) -> Result<CliAction, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut options = DoctorOptions::default();
    let mut arguments = arguments.into_iter().map(Into::into).peekable();

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--json" => options.json = true,
            "--help" | "-h" => return Ok(CliAction::Help),
            "--phase" => {
                let value = arguments.next().ok_or_else(|| {
                    tr("--phase requires a value", "--phase 需要一个阶段值").to_string()
                })?;
                options.phase = value.parse()?;
            }
            _ if argument.starts_with("--phase=") => {
                let value = argument.trim_start_matches("--phase=");
                if value.is_empty() {
                    return Err(
                        tr("--phase requires a value", "--phase 需要一个阶段值").to_string()
                    );
                }
                options.phase = value.parse()?;
            }
            _ => {
                return Err(format!(
                    "{}: {argument}",
                    tr("Unknown argument", "无法识别的参数")
                ));
            }
        }
    }

    Ok(CliAction::Run(options))
}

pub fn help_text() -> &'static str {
    tr(
        "Usage: omavoice-doctor [--json] [--phase <0a|0b|1|2|3|4|5|6>]\n\nRead-only checks of the system capabilities OmaVoice needs on Omarchy Linux.\nThe default is the current phase, 0a. Components needed only by later phases do not block it.\n\nOptions:\n  --json          Print stable, machine-readable JSON\n  --phase <value> Check readiness for a development phase\n  -h, --help      Show help\n",
        "用法：omavoice-doctor [--json] [--phase <0a|0b|1|2|3|4|5|6>]\n\n只读检查 Omarchy Linux 上运行 OmaVoice 所需的系统能力。\n默认检查当前阶段 0a；缺少以后阶段才需要的组件不会阻塞当前阶段。\n\n选项：\n  --json          输出稳定、机器可读的 JSON\n  --phase <值>    检查指定开发阶段的就绪状态\n  -h, --help      显示帮助\n",
    )
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SystemSnapshot {
    pub linux: bool,
    pub wayland_session: bool,
    pub hyprland_session: bool,
    pub gtk4: bool,
    pub libadwaita: bool,
    pub bluez_active: bool,
    pub pipewire_active: bool,
    pub wireplumber_active: bool,
    pub input_group: bool,
    pub evdev_readable: bool,
    pub uinput_writable: bool,
    pub gtk3_layer_shell: bool,
    pub handy_available: bool,
    pub handy_service_ready: bool,
    pub statistics_service_ready: bool,
    pub atvvoice_available: bool,
    pub atvvoice_service_active: bool,
    pub atvvoice_source_available: bool,
    pub text_injectors: Vec<String>,
    pub remappers: Vec<String>,
}

pub fn collect_system_snapshot() -> SystemSnapshot {
    let identity = current_identity();

    SystemSnapshot {
        linux: cfg!(target_os = "linux"),
        wayland_session: env_is("XDG_SESSION_TYPE", "wayland") && env_nonempty("WAYLAND_DISPLAY"),
        hyprland_session: env_nonempty("HYPRLAND_INSTANCE_SIGNATURE") && command_exists("hyprctl"),
        gtk4: pkg_config_exists("gtk4"),
        libadwaita: pkg_config_exists("libadwaita-1"),
        bluez_active: command_success("systemctl", &["is-active", "--quiet", "bluetooth.service"]),
        pipewire_active: command_success(
            "systemctl",
            &["--user", "is-active", "--quiet", "pipewire.service"],
        ),
        wireplumber_active: command_success(
            "systemctl",
            &["--user", "is-active", "--quiet", "wireplumber.service"],
        ),
        input_group: current_group_names().iter().any(|group| group == "input"),
        evdev_readable: identity.as_ref().is_some_and(|identity| {
            any_matching_device_accessible(Path::new("/dev/input"), "event", identity, 0o4)
        }),
        uinput_writable: identity
            .as_ref()
            .is_some_and(|identity| path_accessible(Path::new("/dev/uinput"), identity, 0o2)),
        gtk3_layer_shell: pkg_config_exists("gtk-layer-shell-0"),
        handy_available: command_exists("handy") || command_exists("Handy"),
        handy_service_ready: command_success(
            "systemctl",
            &["--user", "is-enabled", "--quiet", "omavoice-handy.service"],
        ) && command_success(
            "systemctl",
            &["--user", "is-active", "--quiet", "omavoice-handy.service"],
        ),
        statistics_service_ready: command_success(
            "systemctl",
            &[
                "--user",
                "is-enabled",
                "--quiet",
                "omavoice-statistics.service",
            ],
        ) && command_success(
            "systemctl",
            &[
                "--user",
                "is-active",
                "--quiet",
                "omavoice-statistics.service",
            ],
        ),
        atvvoice_available: command_exists("omavoice-atvvoice") || command_exists("atvvoice"),
        atvvoice_service_active: command_success(
            "systemctl",
            &[
                "--user",
                "is-active",
                "--quiet",
                "omavoice-atvvoice.service",
            ],
        ),
        atvvoice_source_available: command_output("wpctl", &["status", "-n"])
            .is_some_and(|output| output.contains("atvvoice-omavoice-rc003")),
        text_injectors: available_commands(&["wtype", "ydotool"]),
        remappers: available_commands(&["keyd", "makima", "input-remapper", "evremap"]),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Passed,
    Missing,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CheckResult {
    pub id: &'static str,
    pub label: &'static str,
    pub status: CheckStatus,
    pub required_from: Option<Phase>,
    pub blocking: bool,
    pub detail: String,
    pub remediation: Option<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReportSummary {
    pub passed: usize,
    pub missing: usize,
    pub blocking: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DoctorReport {
    pub schema_version: u32,
    pub target_phase: Phase,
    pub ready: bool,
    pub summary: ReportSummary,
    pub checks: Vec<CheckResult>,
}

pub fn evaluate(snapshot: &SystemSnapshot, target_phase: Phase) -> DoctorReport {
    let mut checks = Vec::new();

    push_check(
        &mut checks,
        target_phase,
        "linux",
        tr("Linux system", "Linux 系统"),
        snapshot.linux,
        Some(Phase::ZeroA),
        tr(
            "The current process is running on Linux",
            "当前进程运行在 Linux",
        ),
        tr(
            "Run this diagnostic on Omarchy or Arch Linux",
            "请在 Omarchy / Arch Linux 上运行诊断",
        ),
    );
    push_check(
        &mut checks,
        target_phase,
        "wayland-session",
        tr("Wayland session", "Wayland 会话"),
        snapshot.wayland_session,
        Some(Phase::ZeroB),
        tr(
            "The current graphical session is Wayland",
            "当前图形会话是 Wayland",
        ),
        tr(
            "Run from an Omarchy Wayland session, not a plain TTY or X11 session",
            "请从 Omarchy 的 Wayland 会话运行，不要从纯 TTY 或 X11 会话运行",
        ),
    );
    push_check(
        &mut checks,
        target_phase,
        "hyprland-session",
        tr("Hyprland session", "Hyprland 会话"),
        snapshot.hyprland_session,
        Some(Phase::ZeroB),
        tr(
            "The Hyprland session and hyprctl are available",
            "Hyprland 会话与 hyprctl 可用",
        ),
        tr(
            "Confirm that Hyprland is running and hyprctl is in PATH",
            "请确认 Hyprland 会话已启动且 hyprctl 在 PATH 中",
        ),
    );
    push_check(
        &mut checks,
        target_phase,
        "gtk4",
        "GTK4",
        snapshot.gtk4,
        Some(Phase::ZeroB),
        tr(
            "GTK4 for OmaVoice Settings is available",
            "OmaVoice 设置中心所需 GTK4 可用",
        ),
        tr(
            "Install the Arch packages gtk4 and pkgconf",
            "请安装 Arch 软件包 gtk4 和 pkgconf",
        ),
    );
    push_check(
        &mut checks,
        target_phase,
        "libadwaita",
        "libadwaita",
        snapshot.libadwaita,
        Some(Phase::ZeroB),
        tr(
            "libadwaita for OmaVoice Settings is available",
            "OmaVoice 设置中心所需 libadwaita 可用",
        ),
        tr(
            "Install the Arch packages libadwaita and pkgconf",
            "请安装 Arch 软件包 libadwaita 和 pkgconf",
        ),
    );
    push_check(
        &mut checks,
        target_phase,
        "bluez",
        tr("BlueZ service", "BlueZ 服务"),
        snapshot.bluez_active,
        Some(Phase::One),
        tr("bluetooth.service is running", "bluetooth.service 正在运行"),
        tr(
            "Install bluez and bluez-utils, then enable bluetooth.service",
            "请安装 bluez/bluez-utils 并启用 bluetooth.service",
        ),
    );
    push_check(
        &mut checks,
        target_phase,
        "pipewire",
        tr("PipeWire service", "PipeWire 服务"),
        snapshot.pipewire_active,
        Some(Phase::One),
        tr("pipewire.service is running", "pipewire.service 正在运行"),
        tr(
            "Install and start the PipeWire user service",
            "请安装并启动 PipeWire 用户服务",
        ),
    );
    push_check(
        &mut checks,
        target_phase,
        "wireplumber",
        tr("WirePlumber service", "WirePlumber 服务"),
        snapshot.wireplumber_active,
        Some(Phase::One),
        tr(
            "wireplumber.service is running",
            "wireplumber.service 正在运行",
        ),
        tr(
            "Install and start the WirePlumber user service",
            "请安装并启动 WirePlumber 用户服务",
        ),
    );
    push_check(
        &mut checks,
        target_phase,
        "input-group",
        tr("input user group", "input 用户组"),
        snapshot.input_group,
        Some(Phase::One),
        tr(
            "The current user belongs to the input group",
            "当前用户属于 input 组",
        ),
        tr(
            "After user confirmation, add the current user to the input group and log in again",
            "请在用户确认后把当前用户加入 input 组，并重新登录",
        ),
    );
    push_check(
        &mut checks,
        target_phase,
        "evdev-read",
        tr("evdev read access", "evdev 读取权限"),
        snapshot.evdev_readable,
        Some(Phase::One),
        tr(
            "At least one /dev/input/event* node is readable",
            "至少一个 /dev/input/event* 节点可读",
        ),
        tr(
            "Check the input group and udev permissions for /dev/input/event*; do not run Handy as root",
            "请检查 input 组和 /dev/input/event* 的 udev 权限；不要以 root 运行 Handy",
        ),
    );
    push_check(
        &mut checks,
        target_phase,
        "atvvoice",
        "ATVVoice",
        snapshot.atvvoice_available,
        Some(Phase::One),
        tr(
            "The OmaVoice ATVVoice executable is available",
            "OmaVoice ATVVoice 可执行文件可用",
        ),
        tr(
            "Build the pinned ATVVoice revision gated for RC001 and RC003",
            "请构建经过 RC001 / RC003 门禁的固定 ATVVoice 提交",
        ),
    );
    push_check(
        &mut checks,
        target_phase,
        "atvvoice-service",
        tr("ATVVoice user service", "ATVVoice 用户服务"),
        snapshot.atvvoice_service_active,
        Some(Phase::One),
        tr(
            "omavoice-atvvoice.service is running",
            "omavoice-atvvoice.service 正在运行",
        ),
        tr(
            "Run the OmaVoice user installer or start omavoice-atvvoice.service",
            "请运行 OmaVoice 用户态安装脚本，或启动 omavoice-atvvoice.service",
        ),
    );
    push_check(
        &mut checks,
        target_phase,
        "atvvoice-source",
        tr("ATVVoice PipeWire source", "ATVVoice PipeWire 输入源"),
        snapshot.atvvoice_source_available,
        Some(Phase::One),
        tr(
            "The atvvoice-omavoice-rc003 source is available",
            "atvvoice-omavoice-rc003 输入源可用",
        ),
        tr(
            "Check omavoicectl status and the user journal; if HCI timeouts occur at the same time, stop the service and recover the Bluetooth controller first",
            "请检查 omavoicectl status 与用户日志；若同期存在 HCI timeout，请停止服务并先恢复蓝牙控制器",
        ),
    );
    push_check(
        &mut checks,
        target_phase,
        "gtk3-layer-shell",
        tr("GTK3 layer shell", "GTK3 layer-shell"),
        snapshot.gtk3_layer_shell,
        Some(Phase::Two),
        tr(
            "gtk-layer-shell-0 for the Handy overlay is available",
            "Handy overlay 所需 gtk-layer-shell-0 可用",
        ),
        tr(
            "Install the Arch package gtk-layer-shell; gtk4-layer-shell is not a replacement",
            "请安装 Arch 软件包 gtk-layer-shell；gtk4-layer-shell 不能替代它",
        ),
    );
    push_check(
        &mut checks,
        target_phase,
        "handy",
        "Handy",
        snapshot.handy_available,
        Some(Phase::Two),
        tr("The Handy executable is available", "Handy 可执行文件可用"),
        tr(
            "Install the reviewed Handy build with its Wayland overlay support",
            "请安装包含 Wayland overlay 修复的 Handy 构建",
        ),
    );
    push_check(
        &mut checks,
        target_phase,
        "handy-service",
        tr("Handy login service", "Handy 登录服务"),
        snapshot.handy_service_ready,
        Some(Phase::Six),
        tr(
            "omavoice-handy.service is enabled and running",
            "omavoice-handy.service 已启用并正在运行",
        ),
        tr(
            "Run systemctl --user enable --now omavoice-handy.service",
            "请运行 systemctl --user enable --now omavoice-handy.service",
        ),
    );
    push_check(
        &mut checks,
        target_phase,
        "statistics-service",
        tr("Local statistics service", "本机统计服务"),
        snapshot.statistics_service_ready,
        Some(Phase::Six),
        tr(
            "omavoice-statistics.service is enabled and running",
            "omavoice-statistics.service 已启用并正在运行",
        ),
        tr(
            "Run the OmaVoice user installer again or start omavoice-statistics.service",
            "请重新运行 OmaVoice 用户态安装脚本，或启动 omavoice-statistics.service",
        ),
    );
    push_check_with_detail(
        &mut checks,
        target_phase,
        "text-injector",
        tr("Wayland text-insertion tool", "Wayland 文字写入工具"),
        !snapshot.text_injectors.is_empty(),
        Some(Phase::Two),
        available_detail(
            &snapshot.text_injectors,
            tr(
                "Neither wtype nor ydotool is installed",
                "wtype / ydotool 均未安装",
            ),
        ),
        tr(
            "Install wtype first; evaluate ydotool only if the target application is incompatible",
            "请优先安装 wtype；如目标 App 不兼容，再评估 ydotool",
        ),
    );
    push_check(
        &mut checks,
        target_phase,
        "uinput-write",
        tr("uinput write access", "uinput 写入权限"),
        snapshot.uinput_writable,
        Some(Phase::Four),
        tr(
            "/dev/uinput is writable by the current user",
            "/dev/uinput 对当前用户可写",
        ),
        tr(
            "Check the uinput module, input group and udev permissions for /dev/uinput",
            "请检查 uinput 模块、input 组和 /dev/uinput 的 udev 权限",
        ),
    );
    push_check_with_detail(
        &mut checks,
        target_phase,
        "key-remapper",
        tr("Key-remapping tool", "按键映射工具"),
        !snapshot.remappers.is_empty(),
        Some(Phase::Five),
        available_detail(
            &snapshot.remappers,
            tr(
                "None of keyd, makima, input-remapper or evremap is installed",
                "keyd / makima / input-remapper / evremap 均未安装",
            ),
        ),
        tr(
            "Prefer keyd from the official Arch repository; evaluate makima-bin only for per-application mappings",
            "请优先安装 Arch 官方仓库中的 keyd；需要按 App 映射时再评估 Omarchy 仓库中的 makima-bin",
        ),
    );

    let passed = checks
        .iter()
        .filter(|check| check.status == CheckStatus::Passed)
        .count();
    let missing = checks.len() - passed;
    let blocking = checks.iter().filter(|check| check.blocking).count();

    DoctorReport {
        schema_version: 1,
        target_phase,
        ready: blocking == 0,
        summary: ReportSummary {
            passed,
            missing,
            blocking,
        },
        checks,
    }
}

pub fn render_human(report: &DoctorReport) -> String {
    let mut output = match i18n::language() {
        i18n::Language::English => format!(
            "OmaVoice Linux capability report (target phase {})\n\n",
            report.target_phase
        ),
        i18n::Language::SimplifiedChinese => format!(
            "OmaVoice Linux 能力诊断（目标阶段 {}）\n\n",
            report.target_phase
        ),
    };

    for check in &report.checks {
        let status = match (check.status, check.blocking) {
            (CheckStatus::Passed, _) => tr("passed", "通过"),
            (CheckStatus::Missing, true) => tr("blocked", "阻塞"),
            (CheckStatus::Missing, false) => tr("needed later", "稍后需要"),
        };
        let required = check
            .required_from
            .map(|phase| match i18n::language() {
                i18n::Language::English => format!("; required from phase {phase}"),
                i18n::Language::SimplifiedChinese => format!("；阶段 {phase} 起必需"),
            })
            .unwrap_or_else(|| tr("; optional", "；可选").to_string());
        output.push_str(&match i18n::language() {
            i18n::Language::English => {
                format!("[{status}] {}: {}{required}\n", check.label, check.detail)
            }
            i18n::Language::SimplifiedChinese => {
                format!("[{status}] {}：{}{required}\n", check.label, check.detail)
            }
        });
        if check.status == CheckStatus::Missing {
            if let Some(remediation) = check.remediation {
                output.push_str(&match i18n::language() {
                    i18n::Language::English => format!("       Fix: {remediation}\n"),
                    i18n::Language::SimplifiedChinese => {
                        format!("       建议：{remediation}\n")
                    }
                });
            }
        }
    }

    match i18n::language() {
        i18n::Language::English => {
            output.push_str(&format!(
                "\nSummary: {} passed, {} missing, {} currently blocking.\n",
                report.summary.passed, report.summary.missing, report.summary.blocking
            ));
            output.push_str(if report.ready {
                "Conclusion: the current phase can continue.\n"
            } else {
                "Conclusion: resolve the blocked conditions before continuing.\n"
            });
        }
        i18n::Language::SimplifiedChinese => {
            output.push_str(&format!(
                "\n汇总：通过 {}，缺失 {}，当前阻塞 {}。\n",
                report.summary.passed, report.summary.missing, report.summary.blocking
            ));
            output.push_str(if report.ready {
                "结论：当前阶段可以继续。\n"
            } else {
                "结论：请先解决标记为“阻塞”的条件。\n"
            });
        }
    }
    output
}

pub fn render_json(report: &DoctorReport) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(report)
}

pub fn exit_code(report: &DoctorReport) -> i32 {
    if report.ready { 0 } else { BLOCKED_EXIT_CODE }
}

#[allow(clippy::too_many_arguments)]
fn push_check(
    checks: &mut Vec<CheckResult>,
    target_phase: Phase,
    id: &'static str,
    label: &'static str,
    passed: bool,
    required_from: Option<Phase>,
    passed_detail: &'static str,
    remediation: &'static str,
) {
    let detail = if passed {
        passed_detail.to_string()
    } else {
        tr("Not satisfied", "未满足").to_string()
    };
    push_check_with_detail(
        checks,
        target_phase,
        id,
        label,
        passed,
        required_from,
        detail,
        remediation,
    );
}

#[allow(clippy::too_many_arguments)]
fn push_check_with_detail(
    checks: &mut Vec<CheckResult>,
    target_phase: Phase,
    id: &'static str,
    label: &'static str,
    passed: bool,
    required_from: Option<Phase>,
    detail: String,
    remediation: &'static str,
) {
    let blocking = !passed && required_from.is_some_and(|phase| phase <= target_phase);
    checks.push(CheckResult {
        id,
        label,
        status: if passed {
            CheckStatus::Passed
        } else {
            CheckStatus::Missing
        },
        required_from,
        blocking,
        detail,
        remediation: (!passed).then_some(remediation),
    });
}

fn available_detail(commands: &[String], missing_detail: &str) -> String {
    if commands.is_empty() {
        missing_detail.to_string()
    } else {
        match i18n::language() {
            i18n::Language::English => format!("Found: {}", commands.join(", ")),
            i18n::Language::SimplifiedChinese => format!("已找到：{}", commands.join("、")),
        }
    }
}

fn env_nonempty(name: &str) -> bool {
    env::var_os(name).is_some_and(|value| !value.is_empty())
}

fn env_is(name: &str, expected: &str) -> bool {
    env::var(name).is_ok_and(|value| value.eq_ignore_ascii_case(expected))
}

fn command_success(program: &str, arguments: &[&str]) -> bool {
    Command::new(program)
        .args(arguments)
        .status()
        .is_ok_and(|status| status.success())
}

fn pkg_config_exists(package: &str) -> bool {
    command_success("pkg-config", &["--exists", package])
}

fn available_commands(commands: &[&str]) -> Vec<String> {
    commands
        .iter()
        .filter(|command| command_exists(command))
        .map(|command| (*command).to_string())
        .collect()
}

pub fn command_exists(command: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path).any(|directory| {
        fs::metadata(directory.join(command))
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    })
}

#[derive(Clone, Debug)]
struct Identity {
    uid: u32,
    group_ids: Vec<u32>,
}

fn current_identity() -> Option<Identity> {
    let uid = command_output("id", &["-u"])?.trim().parse().ok()?;
    let group_ids = command_output("id", &["-G"])?
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    Some(Identity { uid, group_ids })
}

fn current_group_names() -> Vec<String> {
    command_output("id", &["-nG"])
        .map(|output| output.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default()
}

fn command_output(program: &str, arguments: &[&str]) -> Option<String> {
    let output = Command::new(program).args(arguments).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn any_matching_device_accessible(
    directory: &Path,
    prefix: &str,
    identity: &Identity,
    access_bit: u32,
) -> bool {
    fs::read_dir(directory).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(prefix))
                && path_accessible(&entry.path(), identity, access_bit)
        })
    })
}

fn path_accessible(path: &Path, identity: &Identity, access_bit: u32) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if identity.uid == 0 {
        return true;
    }

    let mode = metadata.mode();
    if metadata.uid() == identity.uid {
        mode & (access_bit << 6) != 0
    } else if identity.group_ids.contains(&metadata.gid()) {
        mode & (access_bit << 3) != 0
    } else {
        mode & access_bit != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready_snapshot() -> SystemSnapshot {
        SystemSnapshot {
            linux: true,
            wayland_session: true,
            hyprland_session: true,
            gtk4: true,
            libadwaita: true,
            bluez_active: true,
            pipewire_active: true,
            wireplumber_active: true,
            input_group: true,
            evdev_readable: true,
            uinput_writable: true,
            gtk3_layer_shell: true,
            handy_available: true,
            handy_service_ready: true,
            statistics_service_ready: true,
            atvvoice_available: true,
            atvvoice_service_active: true,
            atvvoice_source_available: true,
            text_injectors: vec!["wtype".to_string()],
            remappers: vec!["keyd".to_string()],
        }
    }

    fn check<'a>(report: &'a DoctorReport, id: &str) -> &'a CheckResult {
        report
            .checks
            .iter()
            .find(|check| check.id == id)
            .expect("missing expected check")
    }

    #[test]
    fn future_requirements_do_not_block_phase_zero_a() {
        let snapshot = SystemSnapshot {
            linux: true,
            ..SystemSnapshot::default()
        };

        let report = evaluate(&snapshot, Phase::ZeroA);

        assert!(report.ready);
        assert_eq!(report.summary.blocking, 0);
        assert_eq!(check(&report, "handy").status, CheckStatus::Missing);
        assert!(!check(&report, "handy").blocking);
        assert_eq!(exit_code(&report), 0);
    }

    #[test]
    fn phase_two_blocks_on_missing_handy_overlay_and_text_injector() {
        let mut snapshot = ready_snapshot();
        snapshot.handy_available = false;
        snapshot.gtk3_layer_shell = false;
        snapshot.text_injectors.clear();

        let report = evaluate(&snapshot, Phase::Two);

        assert!(!report.ready);
        assert_eq!(report.summary.blocking, 3);
        assert!(check(&report, "handy").blocking);
        assert!(check(&report, "gtk3-layer-shell").blocking);
        assert!(check(&report, "text-injector").blocking);
        assert_eq!(exit_code(&report), BLOCKED_EXIT_CODE);
    }

    #[test]
    fn phase_six_requires_the_enabled_running_handy_service() {
        let mut snapshot = ready_snapshot();
        snapshot.handy_service_ready = false;

        let phase_two = evaluate(&snapshot, Phase::Two);
        assert!(phase_two.ready);
        assert!(!check(&phase_two, "handy-service").blocking);

        let phase_six = evaluate(&snapshot, Phase::Six);
        assert!(!phase_six.ready);
        assert!(check(&phase_six, "handy-service").blocking);
        assert_eq!(exit_code(&phase_six), BLOCKED_EXIT_CODE);
    }

    #[test]
    fn phase_six_requires_the_private_statistics_service() {
        let mut snapshot = ready_snapshot();
        snapshot.statistics_service_ready = false;

        let phase_five = evaluate(&snapshot, Phase::Five);
        assert!(phase_five.ready);
        assert!(!check(&phase_five, "statistics-service").blocking);

        let phase_six = evaluate(&snapshot, Phase::Six);
        assert!(!phase_six.ready);
        assert!(check(&phase_six, "statistics-service").blocking);
        assert_eq!(exit_code(&phase_six), BLOCKED_EXIT_CODE);
    }

    #[test]
    fn phase_one_requires_the_atvvoice_binary_and_running_user_service() {
        let mut snapshot = ready_snapshot();
        snapshot.atvvoice_available = false;
        snapshot.atvvoice_service_active = false;

        let report = evaluate(&snapshot, Phase::One);

        assert!(!report.ready);
        assert_eq!(report.summary.blocking, 2);
        assert!(check(&report, "atvvoice").blocking);
        assert!(check(&report, "atvvoice-service").blocking);
    }

    #[test]
    fn phase_one_blocks_when_the_service_has_no_pipewire_source() {
        let mut snapshot = ready_snapshot();
        snapshot.atvvoice_source_available = false;

        let report = evaluate(&snapshot, Phase::One);

        assert!(!report.ready);
        assert_eq!(report.summary.blocking, 1);
        assert_eq!(
            check(&report, "atvvoice-service").status,
            CheckStatus::Passed
        );
        assert!(check(&report, "atvvoice-source").blocking);
    }

    #[test]
    fn gtk4_does_not_satisfy_handys_gtk3_layer_shell_requirement() {
        let mut snapshot = ready_snapshot();
        snapshot.gtk3_layer_shell = false;

        let report = evaluate(&snapshot, Phase::Two);

        assert_eq!(check(&report, "gtk4").status, CheckStatus::Passed);
        assert_eq!(
            check(&report, "gtk3-layer-shell").status,
            CheckStatus::Missing
        );
        assert!(check(&report, "gtk3-layer-shell").blocking);
    }

    #[test]
    fn accepted_alternatives_satisfy_combined_checks() {
        let mut snapshot = ready_snapshot();
        snapshot.text_injectors = vec!["ydotool".to_string()];
        snapshot.remappers = vec!["makima".to_string()];

        let report = evaluate(&snapshot, Phase::Six);

        assert!(report.ready);
        assert_eq!(check(&report, "text-injector").status, CheckStatus::Passed);
        assert_eq!(check(&report, "key-remapper").status, CheckStatus::Passed);
    }

    #[test]
    fn json_has_stable_schema_without_private_device_fields() {
        let report = evaluate(&ready_snapshot(), Phase::Six);

        let json = render_json(&report).expect("report should serialize");

        assert!(json.contains("\"schema_version\": 1"));
        assert!(json.contains("\"target_phase\": \"6\""));
        assert!(!json.contains("bluetooth_address"));
        assert!(!json.contains("device_serial"));
        assert!(!json.contains("voice_text"));
    }

    #[test]
    fn parses_json_and_phase_arguments() {
        assert_eq!(
            parse_args(["--json", "--phase", "2"]),
            Ok(CliAction::Run(DoctorOptions {
                json: true,
                phase: Phase::Two,
            }))
        );
        assert_eq!(
            parse_args(["--phase=0b"]),
            Ok(CliAction::Run(DoctorOptions {
                json: false,
                phase: Phase::ZeroB,
            }))
        );
    }

    #[test]
    fn rejects_unknown_or_incomplete_arguments() {
        assert!(parse_args(["--phase"]).is_err());
        assert!(parse_args(["--phase=7"]).is_err());
        assert!(parse_args(["--unknown"]).is_err());
    }
}
