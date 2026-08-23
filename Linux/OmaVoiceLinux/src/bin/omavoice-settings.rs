use std::cell::{Cell, RefCell};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::{
    LazyLock,
    mpsc::{self, Sender, TryRecvError},
};
use std::time::{Duration, SystemTime};

use chrono::{Local, TimeZone};
use gtk::{Align, Orientation};
use image::GenericImageView;
use ksni::blocking::TrayMethods;
use libadwaita as adw;

use adw::prelude::*;
use gtk::glib::{self, ControlFlow, Propagation};
use omavoice_linux::config::{
    ButtonAction, ConfigStore, ControlSource, DeviceProfile, KeyboardShortcut, LinuxConfig,
    RemoteButton, Transport, VoiceSource,
};
use omavoice_linux::diagnostics::{
    diagnostic_directory_from_environment, export_diagnostic_report, log_directory_from_environment,
};
use omavoice_linux::i18n::{Language, language, tr};
use omavoice_linux::keyd::render_rc003_keyd_preview;
use omavoice_linux::statistics::{
    StatisticsDatabase, StatisticsPaths, StatisticsPeriod, StatisticsSummary,
};
use omavoice_linux::transcripts::{
    TranscriptArchive, TranscriptDatabase, TranscriptPaths, read_archive_file, write_archive_file,
};
use omavoice_linux::{
    CheckResult, CheckStatus, DoctorReport, Phase, collect_system_snapshot, command_exists,
    evaluate,
};

macro_rules! trf {
    ($english:literal, $chinese:literal $(, $argument:expr)* $(,)?) => {
        match language() {
            Language::English => format!($english $(, $argument)*),
            Language::SimplifiedChinese => format!($chinese $(, $argument)*),
        }
    };
}

const APP_ID: &str = "app.omavoice.Settings";
const OMARCHY_SHELL_PATH: &str = "/usr/share/omarchy/shell";
const PKEXEC_PATH: &str = "/usr/bin/pkexec";
const KEYD_HELPER_PATH: &str = "/usr/lib/omavoice/omavoice-keyd-helper";
const KEYD_HELPER_PROTOCOL: &str = "apply-v1";
const REMOTE_IMAGE: &[u8] = include_bytes!("../../../../Resources/RC003-remote-photo.png");
const TRAY_ICON_PNG: &[u8] = include_bytes!("../../../icons/app.omavoice.Settings.tray.png");
const SETTINGS_CSS: &str = "window { font-size: 12pt; }";

type SharedUi = Rc<RefCell<Option<SettingsUi>>>;
type SharedProfiles = Rc<RefCell<ProfileSettings>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrayCommand {
    OpenSettings,
    Refresh,
    Quit,
}

struct SettingsUi {
    window: adw::ApplicationWindow,
    stack: gtk::Stack,
    profiles: SharedProfiles,
}

#[derive(Clone)]
struct KeydPreviewUi {
    buffer: gtk::TextBuffer,
    status: gtk::Label,
    summary: adw::ActionRow,
    apply_button: gtk::Button,
}

#[derive(Clone)]
struct ShortcutEditorUi {
    group: adw::PreferencesGroup,
    control: gtk::ToggleButton,
    alt: gtk::ToggleButton,
    shift: gtk::ToggleButton,
    super_key: gtk::ToggleButton,
    key: adw::EntryRow,
    summary: adw::ActionRow,
    status: gtk::Label,
}

#[derive(Clone)]
struct StatisticsUi {
    period: Rc<Cell<StatisticsPeriod>>,
    status: gtk::Label,
    button_presses: gtk::Label,
    voice_sessions: gtk::Label,
    voice_duration: gtk::Label,
    button_ranking: Vec<(adw::ActionRow, gtk::Label)>,
    voice_ranking: Vec<(adw::ActionRow, gtk::Label)>,
    clear_button: gtk::Button,
    transcript_status: gtk::Label,
    transcript_switch: adw::SwitchRow,
    transcript_count: gtk::Label,
    transcript_import_button: gtk::Button,
    transcript_export_button: gtk::Button,
    transcript_clear_button: gtk::Button,
    transcript_refreshing: Rc<Cell<bool>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LaunchSpec {
    program: &'static str,
    arguments: &'static [&'static str],
}

const OMARCHY_BLUETOOTH: LaunchSpec = LaunchSpec {
    program: "quickshell",
    arguments: &[
        "ipc",
        "-p",
        OMARCHY_SHELL_PATH,
        "call",
        "omarchy.bluetooth",
        "open",
    ],
};
const BLUEMAN: LaunchSpec = LaunchSpec {
    program: "blueman-manager",
    arguments: &[],
};
const OVERSKRIDE: LaunchSpec = LaunchSpec {
    program: "overskride",
    arguments: &[],
};
const GNOME_BLUETOOTH: LaunchSpec = LaunchSpec {
    program: "gnome-control-center",
    arguments: &["bluetooth"],
};
const KDE_BLUETOOTH: LaunchSpec = LaunchSpec {
    program: "systemsettings6",
    arguments: &["kcm_bluetooth"],
};
const OMAVOICE_HANDY: LaunchSpec = LaunchSpec {
    program: "omavoice-handy",
    arguments: &[],
};
const HANDY_LOWERCASE: LaunchSpec = LaunchSpec {
    program: "handy",
    arguments: &[],
};
const HANDY_TITLECASE: LaunchSpec = LaunchSpec {
    program: "Handy",
    arguments: &[],
};

#[derive(Debug)]
struct ProfileSettings {
    store: Option<ConfigStore>,
    config: Option<LinuxConfig>,
    error: Option<String>,
    persisted: bool,
    dirty: bool,
}

impl ProfileSettings {
    fn load() -> Self {
        let store = match ConfigStore::from_xdg_environment() {
            Ok(store) => store,
            Err(error) => {
                return Self {
                    store: None,
                    config: None,
                    error: Some(error.to_string()),
                    persisted: false,
                    dirty: false,
                };
            }
        };
        let persisted = store.exists();
        match store.load() {
            Ok(config) => Self {
                store: Some(store),
                config: Some(config),
                error: None,
                persisted,
                dirty: false,
            },
            Err(error) => Self {
                store: Some(store),
                config: None,
                error: Some(error.to_string()),
                persisted,
                dirty: false,
            },
        }
    }
}

#[derive(Debug)]
struct OmaVoiceTray {
    commands: Sender<TrayCommand>,
}

struct TrayRuntime {
    handle: ksni::blocking::Handle<OmaVoiceTray>,
    _application_hold: gtk::gio::ApplicationHoldGuard,
}

impl OmaVoiceTray {
    fn send(&self, command: TrayCommand) {
        let _ = self.commands.send(command);
    }
}

fn tray_icon() -> &'static ksni::Icon {
    static ICON: LazyLock<ksni::Icon> = LazyLock::new(|| {
        let image = image::load_from_memory_with_format(TRAY_ICON_PNG, image::ImageFormat::Png)
            .expect("the embedded tray icon is a valid PNG");
        let (width, height) = image.dimensions();
        let mut data = image.into_rgba8().into_vec();
        for pixel in data.chunks_exact_mut(4) {
            pixel.rotate_right(1);
        }
        ksni::Icon {
            width: width as i32,
            height: height as i32,
            data,
        }
    });

    &ICON
}

impl ksni::Tray for OmaVoiceTray {
    fn id(&self) -> String {
        "omavoice-settings".into()
    }

    fn title(&self) -> String {
        "OmaVoice".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![tray_icon().clone()]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            icon_pixmap: self.icon_pixmap(),
            title: self.title(),
            description: tr(
                "Xiaomi Bluetooth Voice Remote Control and Global Voice Input",
                tr(
                    "Xiaomi Bluetooth Voice Remote Control and Global Voice Input",
                    "小米蓝牙语音遥控器与全局语音输入",
                ),
            )
            .into(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(TrayCommand::OpenSettings);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;

        vec![
            StandardItem {
                label: tr("Open Settings", tr("Open Settings", "打开设置")).into(),
                icon_name: "preferences-system".into(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayCommand::OpenSettings)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: tr(
                    "Omarchy Linux Preview",
                    tr("Omarchy Linux Preview", "Omarchy Linux 预览"),
                )
                .into(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: tr("Retest", tr("Retest", "重新检测")).into(),
                icon_name: "view-refresh".into(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayCommand::Refresh)),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: tr("Quit OmaVoice", tr("Quit OmaVoice", "退出 OmaVoice")).into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayCommand::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PageDefinition {
    id: &'static str,
    title: &'static str,
    icon: &'static str,
    description: &'static str,
    check_ids: &'static [&'static str],
}

type RemoteButtonRow = (&'static str, Vec<(RemoteButton, &'static str)>);
type ButtonActionRow = (&'static str, Vec<(ButtonAction, &'static str)>);

static REMOTE_BUTTON_ROWS: LazyLock<[RemoteButtonRow; 4]> = LazyLock::new(|| {
    [
        (
            tr("System", "系统"),
            vec![(RemoteButton::Power, tr("Power", "电源"))],
        ),
        (
            tr("D-pad", "方向盘"),
            vec![
                (RemoteButton::Up, tr("Up", "上")),
                (RemoteButton::Left, tr("Left", "左")),
                (RemoteButton::Ok, tr("OK", "确定")),
                (RemoteButton::Right, tr("Right", "右")),
                (RemoteButton::Down, tr("Down", "下")),
            ],
        ),
        (
            tr("Function keys", "功能键"),
            vec![
                (RemoteButton::Back, tr("Back", "返回")),
                (RemoteButton::Home, tr("Home", "主页")),
                (RemoteButton::Menu, tr("Menu", "菜单")),
                (RemoteButton::Tv, "TV"),
            ],
        ),
        (
            tr("Volume", "音量"),
            vec![
                (RemoteButton::VolumeUp, tr("Volume +", "音量+")),
                (RemoteButton::VolumeDown, tr("Volume -", "音量−")),
            ],
        ),
    ]
});

static BUTTON_ACTION_ROWS: LazyLock<[ButtonActionRow; 6]> = LazyLock::new(|| {
    [
        (
            tr("Behavior", "处理方式"),
            vec![
                (ButtonAction::Disabled, tr("Disable", "禁用")),
                (
                    ButtonAction::PassThrough,
                    tr("Keep original key", "保持原键"),
                ),
            ],
        ),
        (
            tr("Basic buttons", "基础按键"),
            vec![
                (ButtonAction::Escape, "Esc"),
                (ButtonAction::Enter, "Enter"),
                (ButtonAction::Backspace, tr("Backspace", "退格")),
            ],
        ),
        (
            tr("Direction", "方向"),
            vec![
                (ButtonAction::ArrowUp, tr("Up", "上")),
                (ButtonAction::ArrowDown, tr("Down", "下")),
                (ButtonAction::ArrowLeft, tr("Left", "左")),
                (ButtonAction::ArrowRight, tr("Right", "右")),
            ],
        ),
        (
            tr("Desktop", "桌面"),
            vec![
                (ButtonAction::ShowDesktop, tr("Show desktop", "显示桌面")),
                (ButtonAction::ContextMenu, tr("Context Menu", "上下文菜单")),
                (ButtonAction::AppSwitcher, tr("App switcher", "应用切换")),
            ],
        ),
        (
            tr("Media", "媒体"),
            vec![
                (ButtonAction::VolumeUp, tr("Volume +", "音量+")),
                (ButtonAction::VolumeDown, tr("Volume -", "音量−")),
                (ButtonAction::VolumeMute, tr("Mute", "静音")),
                (ButtonAction::PlayPause, tr("Play/Pause", "播放/暂停")),
            ],
        ),
        (
            tr("Custom", "自定义"),
            vec![(
                ButtonAction::CustomShortcut,
                tr("Custom shortcuts", "自定义快捷键"),
            )],
        ),
    ]
});

static PAGES: LazyLock<[PageDefinition; 6]> = LazyLock::new(|| {
    [
        PageDefinition {
            id: "overview",
            title: tr("Overview", "概览"),
            icon: "view-grid-symbolic",
            description: tr(
                "View the underlying readiness status of OmaVoice on the current Omarchy system.",
                "查看 OmaVoice 在当前 Omarchy 系统上的基础就绪状态。",
            ),
            check_ids: &[
                "linux",
                "wayland-session",
                "hyprland-session",
                "gtk4",
                "libadwaita",
            ],
        },
        PageDefinition {
            id: "devices",
            title: tr("Devices", "设备"),
            icon: "bluetooth-symbolic",
            description: tr(
                "View Bluetooth, remote control input permissions, and ATVVoice operating conditions.Pairing is still done by the system Bluetooth interface.",
                "查看蓝牙、遥控器输入权限和 ATVVoice 运行条件。配对仍由系统蓝牙界面完成。",
            ),
            check_ids: &[
                "bluez",
                "input-group",
                "evdev-read",
                "atvvoice",
                "atvvoice-service",
                "atvvoice-source",
            ],
        },
        PageDefinition {
            id: "voice",
            title: tr("Voice", "语音"),
            icon: "audio-input-microphone-symbolic",
            description: tr(
                "View PipeWire, Handy, Floating Interface, and Text Write components without copying Handy's model or API settings.",
                "查看 PipeWire、Handy、悬浮界面和文字写入组件，不复制 Handy 的模型或 API 设置。",
            ),
            check_ids: &[
                "pipewire",
                "wireplumber",
                "gtk3-layer-shell",
                "handy",
                "handy-service",
                "statistics-service",
                "text-injector",
            ],
        },
        PageDefinition {
            id: "buttons",
            title: tr("Buttons", "按键"),
            icon: "input-keyboard-symbolic",
            description: tr(
                "Configure normal buttons; confirmation is required after saving before applying to the system.",
                "配置普通按键；保存后需确认才会应用到系统。",
            ),
            check_ids: &["uinput-write", "key-remapper"],
        },
        PageDefinition {
            id: "statistics",
            title: tr("Statistics", "统计"),
            icon: "view-list-symbolic",
            description: tr(
                "View anonymous usage summary; transcript files are off by default and saved locally only when explicitly turned on or imported.",
                "查看匿名使用汇总；语音正文档案默认关闭，只有明确开启或导入时才保存在本机。",
            ),
            check_ids: &[],
        },
        PageDefinition {
            id: "system",
            title: tr("System Diagnostics", "系统与诊断"),
            icon: "emblem-system-symbolic",
            description: tr(
                "View all read-only check results by development stage; this page does not install software, modify services, or write system configurations.",
                "按开发阶段查看全部只读检查结果；此页面不会安装软件、修改服务或写入系统配置。",
            ),
            check_ids: &[],
        },
    ]
});

fn main() {
    let application = adw::Application::builder().application_id(APP_ID).build();
    let ui = Rc::new(RefCell::new(None));
    let tray_runtime = Rc::new(RefCell::new(None));
    let (commands, command_receiver) = mpsc::channel();

    application.connect_startup({
        let tray_runtime = tray_runtime.clone();
        let commands = commands.clone();
        move |application| match (OmaVoiceTray {
            commands: commands.clone(),
        })
        .assume_sni_available(true)
        .spawn()
        {
            Ok(handle) => {
                *tray_runtime.borrow_mut() = Some(TrayRuntime {
                    handle,
                    _application_hold: application.hold(),
                });
            }
            Err(error) => eprintln!("OmaVoice tray unavailable: {error:?}"),
        }
    });

    application.connect_activate({
        let ui = ui.clone();
        move |application| show_settings(application, &ui)
    });

    glib::timeout_add_local(Duration::from_millis(50), {
        let application = application.downgrade();
        let ui = ui.clone();
        move || {
            let Some(application) = application.upgrade() else {
                return ControlFlow::Break;
            };
            while let Ok(command) = command_receiver.try_recv() {
                match command {
                    TrayCommand::OpenSettings => show_settings(&application, &ui),
                    TrayCommand::Refresh => refresh_ui(&ui),
                    TrayCommand::Quit => {
                        application.quit();
                        return ControlFlow::Break;
                    }
                }
            }
            ControlFlow::Continue
        }
    });

    application.run();

    if let Some(runtime) = tray_runtime.borrow_mut().take() {
        runtime.handle.shutdown().wait();
    }
}

fn show_settings(application: &adw::Application, ui: &SharedUi) {
    if ui.borrow().is_none() {
        *ui.borrow_mut() = Some(build_ui(application));
    }

    if let Some(ui) = ui.borrow().as_ref() {
        ui.window.present();
    }
}

fn refresh_ui(ui: &SharedUi) {
    if let Some(ui) = ui.borrow().as_ref() {
        refresh_pages(&ui.stack, &ui.profiles);
    }
}

fn build_ui(application: &adw::Application) -> SettingsUi {
    let styles = gtk::CssProvider::new();
    styles.load_from_string(SETTINGS_CSS);
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("GTK display is available while building the UI"),
        &styles,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let page_title = adw::WindowTitle::new(PAGES[0].title, "OmaVoice");
    let refresh_button = gtk::Button::with_label(tr("Retest", "重新检测"));
    refresh_button.set_tooltip_text(Some(tr(
        "Re-read the current system and component status",
        "重新读取当前系统与组件状态",
    )));
    let close_button = gtk::Button::builder()
        .icon_name("window-close-symbolic")
        .tooltip_text(tr(
            "Close the settings window and the background tray continues to run",
            "关闭设置窗口，后台托盘继续运行",
        ))
        .build();
    close_button.add_css_class("flat");

    let content_header = adw::HeaderBar::builder()
        .title_widget(&page_title)
        .show_end_title_buttons(false)
        .build();
    content_header.pack_end(&close_button);
    content_header.pack_end(&refresh_button);

    let stack = gtk::Stack::builder()
        .hexpand(true)
        .vexpand(true)
        .hhomogeneous(false)
        .vhomogeneous(false)
        .build();
    let profiles = Rc::new(RefCell::new(ProfileSettings::load()));
    refresh_pages(&stack, &profiles);

    let content_box = gtk::Box::new(Orientation::Vertical, 0);
    content_box.append(&content_header);
    content_box.append(&stack);
    let content_page = adw::NavigationPage::new(&content_box, PAGES[0].title);

    let sidebar_title = adw::WindowTitle::new("OmaVoice", "Omarchy Linux");
    let sidebar_header = adw::HeaderBar::builder()
        .title_widget(&sidebar_title)
        .show_end_title_buttons(false)
        .build();

    let navigation = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .css_classes(["navigation-sidebar"])
        .vexpand(true)
        .build();
    for definition in PAGES.iter().copied() {
        let row = adw::ActionRow::builder()
            .activatable(true)
            .use_markup(false)
            .title(definition.title)
            .build();
        row.add_prefix(&gtk::Image::from_icon_name(definition.icon));
        navigation.append(&row);
    }

    let privacy_note = gtk::Label::new(Some(tr(
        "Only user configurations that are explicitly confirmed are saved · The system is not automatically modified",
        "只保存明确确认的用户配置 · 不自动修改系统",
    )));
    privacy_note.set_halign(Align::Fill);
    privacy_note.set_xalign(0.0);
    privacy_note.set_wrap(true);
    privacy_note.set_max_width_chars(28);
    privacy_note.set_margin_start(18);
    privacy_note.set_margin_end(18);
    privacy_note.set_margin_top(12);
    privacy_note.set_margin_bottom(18);
    privacy_note.add_css_class("dim-label");

    let sidebar_box = gtk::Box::new(Orientation::Vertical, 0);
    sidebar_box.set_size_request(200, -1);
    sidebar_box.append(&sidebar_header);
    sidebar_box.append(&navigation);
    sidebar_box.append(&privacy_note);
    let sidebar_page =
        adw::NavigationPage::new(&sidebar_box, tr("Settings Navigation", "设置导航"));

    let split_view = adw::NavigationSplitView::builder()
        .sidebar(&sidebar_page)
        .content(&content_page)
        .min_sidebar_width(200.0)
        .max_sidebar_width(260.0)
        .build();

    navigation.connect_row_selected({
        let stack = stack.clone();
        let page_title = page_title.clone();
        let content_page = content_page.clone();
        let split_view = split_view.clone();
        let profiles = profiles.clone();
        move |_, row| {
            let Some(row) = row else { return };
            let Some(definition) = PAGES.get(row.index() as usize) else {
                return;
            };
            if matches!(definition.id, "devices" | "buttons" | "statistics") {
                rebuild_page(&stack, *definition, &profiles);
            }
            stack.set_visible_child_name(definition.id);
            page_title.set_title(definition.title);
            content_page.set_title(definition.title);
            split_view.set_show_content(true);
        }
    });

    refresh_button.connect_clicked({
        let stack = stack.clone();
        let profiles = profiles.clone();
        move |_| refresh_pages(&stack, &profiles)
    });

    if let Some(first_row) = navigation.row_at_index(0) {
        navigation.select_row(Some(&first_row));
    }

    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title(tr("OmaVoice Settings", "OmaVoice 设置"))
        .default_width(1000)
        .default_height(700)
        .content(&split_view)
        .build();
    window.set_size_request(800, 650);
    window.connect_close_request(|window| {
        window.set_visible(false);
        Propagation::Stop
    });
    close_button.connect_clicked({
        let window = window.clone();
        move |_| window.set_visible(false)
    });

    SettingsUi {
        window,
        stack,
        profiles,
    }
}

fn rebuild_page(stack: &gtk::Stack, definition: PageDefinition, profiles: &SharedProfiles) {
    if let Some(child) = stack.child_by_name(definition.id) {
        stack.remove(&child);
    }
    let report = evaluate(&collect_system_snapshot(), Phase::ZeroB);
    stack.add_named(
        &build_page(&report, definition, profiles),
        Some(definition.id),
    );
}

fn refresh_pages(stack: &gtk::Stack, profiles: &SharedProfiles) {
    let visible_page = stack
        .visible_child_name()
        .map(|name| name.to_string())
        .unwrap_or_else(|| PAGES[0].id.to_string());

    while let Some(child) = stack.first_child() {
        stack.remove(&child);
    }

    let report = evaluate(&collect_system_snapshot(), Phase::ZeroB);
    for definition in PAGES.iter().copied() {
        stack.add_named(
            &build_page(&report, definition, profiles),
            Some(definition.id),
        );
    }

    let page_exists = PAGES.iter().any(|page| page.id == visible_page);
    stack.set_visible_child_name(if page_exists {
        &visible_page
    } else {
        PAGES[0].id
    });
}

fn build_page(
    report: &DoctorReport,
    definition: PageDefinition,
    profiles: &SharedProfiles,
) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(definition.title)
        .description(definition.description)
        .build();

    if definition.id == "overview" {
        page.add(&build_summary_group(report));
    }
    if definition.id == "devices" {
        page.add(&build_profile_group(profiles));
        page.add(&build_bluetooth_group());
        page.add(&build_remote_group());
    }
    if definition.id == "voice" {
        page.add(&build_handy_group());
    }
    if definition.id == "buttons" {
        for group in build_button_mapping_groups(profiles) {
            page.add(&group);
        }
    }
    if definition.id == "statistics" {
        for group in build_statistics_groups() {
            page.add(&group);
        }
        return page;
    }
    if definition.id == "system" {
        page.add(&build_diagnostics_group(report));
    }

    let checks = checks_for_page(report, definition);
    let group = adw::PreferencesGroup::builder()
        .title(page_group_title(definition.id))
        .description(page_group_description(definition.id))
        .build();
    for check in checks {
        group.add(&build_check_row(check));
    }
    page.add(&group);

    page
}

fn build_statistics_groups() -> Vec<adw::PreferencesGroup> {
    let period = Rc::new(Cell::new(StatisticsPeriod::Today));
    let period_group = adw::PreferencesGroup::builder()
        .title(tr("Time Range", "时间范围"))
        .description(tr(
            "Statistics by local time zone; this week starts on Monday.",
            "按本机时区统计；本周从周一开始。",
        ))
        .build();
    let period_row = adw::ActionRow::builder()
        .title(tr("View Scope", "查看范围"))
        .build();
    let period_controls = gtk::Box::new(Orientation::Horizontal, 0);
    period_controls.add_css_class("linked");
    let today_button = gtk::ToggleButton::with_label(tr("Today", "今日"));
    let week_button = gtk::ToggleButton::with_label(tr("This week", "本周"));
    let all_button = gtk::ToggleButton::with_label(tr("All", "全部"));
    week_button.set_group(Some(&today_button));
    all_button.set_group(Some(&today_button));
    today_button.set_active(true);
    period_controls.append(&today_button);
    period_controls.append(&week_button);
    period_controls.append(&all_button);
    let refresh_button = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text(tr("Reload local stats", "重新读取本机统计"))
        .build();
    refresh_button.add_css_class("flat");
    let controls = gtk::Box::new(Orientation::Horizontal, 12);
    controls.set_valign(Align::Center);
    controls.append(&period_controls);
    controls.append(&refresh_button);
    period_row.add_suffix(&controls);
    period_group.add(&period_row);

    let summary_group = adw::PreferencesGroup::builder()
        .title(tr("Usage Summary", "使用汇总"))
        .description(tr("Keys only count the valid output of the remote control after keyd mapping; custom combinations do not count modifiers.", "按键只统计遥控器经 keyd 映射后的有效输出；自定义组合不统计修饰键。"))
        .build();
    let status = gtk::Label::new(Some(tr("Reading...", "正在读取…")));
    status.set_wrap(true);
    status.set_xalign(1.0);
    status.add_css_class("dim-label");
    let status_row = adw::ActionRow::builder()
        .title(tr("Data Status", "数据状态"))
        .subtitle(tr(
            "SQLite is only saved in the current user's XDG data directory",
            "SQLite 只保存在当前用户的 XDG 数据目录",
        ))
        .build();
    status_row.add_suffix(&status);
    summary_group.add(&status_row);

    let button_presses = statistics_value_label();
    let button_row = adw::ActionRow::builder()
        .title(tr("Number of keystrokes", "按键次数"))
        .build();
    button_row.add_suffix(&button_presses);
    summary_group.add(&button_row);
    let voice_sessions = statistics_value_label();
    let sessions_row = adw::ActionRow::builder()
        .title(tr("Voice Count", "语音次数"))
        .build();
    sessions_row.add_suffix(&voice_sessions);
    summary_group.add(&sessions_row);
    let voice_duration = statistics_value_label();
    let duration_row = adw::ActionRow::builder()
        .title(tr("Total Voice Duration", "语音总时长"))
        .build();
    duration_row.add_suffix(&voice_duration);
    summary_group.add(&duration_row);

    let button_group = adw::PreferencesGroup::builder()
        .title(tr("Output key ranking", "输出按键排行"))
        .description(tr("Show up to 10 items; the same output key cannot backfire which entity key is triggered.", "最多显示 10 项；相同输出键无法反推是哪个实体键触发。"))
        .build();
    let button_ranking = statistics_ranking_rows(&button_group);

    let voice_group = adw::PreferencesGroup::builder()
        .title(tr("Single voice duration ranking", "单次语音时长排行"))
        .description(tr(
            "Display up to 10 times; read only time and duration metadata for Handy WAVs.",
            "最多显示 10 次；只读取 Handy WAV 的时间和时长元数据。",
        ))
        .build();
    let voice_ranking = statistics_ranking_rows(&voice_group);

    let transcript_group = adw::PreferencesGroup::builder()
        .title(tr("Transcript archive (optional)", "文字档案（可选）"))
        .description(
            tr("Transcript text is sensitive data: closed by default, saved locally only, not uploaded, and not written to ordinary OmaVoice logs.", "正文属于敏感数据：默认关闭、只保存在本机，不上传，也不写入普通 OmaVoice 日志。"),
        )
        .build();
    let transcript_switch = adw::SwitchRow::builder()
        .title(tr("Save Voice Text to Local", "保存语音文字到本机"))
        .subtitle(tr("After opening, copy the existing and subsequent final text of Handy read-only; closing will not delete the saved records.", "开启后只读复制 Handy 已有及后续最终文字；关闭不会删除已经保存的记录。"))
        .build();
    transcript_group.add(&transcript_switch);

    let transcript_status = gtk::Label::new(Some(tr("Reading...", "正在读取…")));
    transcript_status.set_wrap(true);
    transcript_status.set_xalign(1.0);
    transcript_status.add_css_class("dim-label");
    let transcript_status_row = adw::ActionRow::builder()
        .title(tr("Archive status", "档案状态"))
        .subtitle(tr(
            "transcripts.db independently saved in the current user's XDG data directory",
            "独立保存在当前用户 XDG 数据目录的 transcripts.db",
        ))
        .build();
    transcript_status_row.add_suffix(&transcript_status);
    transcript_group.add(&transcript_status_row);

    let transcript_count = statistics_value_label();
    let transcript_count_row = adw::ActionRow::builder()
        .title(tr("Local transcripts", "本机文字记录"))
        .subtitle(tr(
            "Contains time to complete, time zone offset, voice duration and source when available",
            "包含完成时间、时区偏移、可用时的语音时长和来源",
        ))
        .build();
    transcript_count_row.add_suffix(&transcript_count);
    transcript_group.add(&transcript_count_row);

    let transcript_import_button = gtk::Button::with_label(tr("Import JSON…", "导入 JSON…"));
    let transcript_export_button = gtk::Button::with_label(tr("Export JSON…", "导出 JSON…"));
    let transcript_file_buttons = gtk::Box::new(Orientation::Horizontal, 6);
    transcript_file_buttons.set_valign(Align::Center);
    transcript_file_buttons.append(&transcript_import_button);
    transcript_file_buttons.append(&transcript_export_button);
    let transcript_file_row = adw::ActionRow::builder()
        .title(tr("Import &amp; Export", "导入与导出"))
        .subtitle(tr("Versioned JSON contains transcript text and time metadata; import will preview the number of additions and duplicates", "版本化 JSON 包含正文和时间元数据；导入会先预览新增与重复数量"))
        .build();
    transcript_file_row.add_suffix(&transcript_file_buttons);
    transcript_group.add(&transcript_file_row);

    let transcript_clear_button =
        gtk::Button::with_label(tr("Clear transcript archive…", "清空文字档案…"));
    transcript_clear_button.add_css_class("destructive-action");
    transcript_clear_button.set_valign(Align::Center);
    let transcript_clear_row = adw::ActionRow::builder()
        .title(tr("Clear local transcript archive", "清空本机文字档案"))
        .subtitle(tr("Delete only the transcripts saved by OmaVoice; do not delete anonymous statistics, Handy history, or recordings", "只删除 OmaVoice 保存的正文；不会删除匿名统计、Handy 历史或录音"))
        .build();
    transcript_clear_row.add_suffix(&transcript_clear_button);
    transcript_group.add(&transcript_clear_row);

    let management_group = adw::PreferencesGroup::builder()
        .title(tr("Anonymous Stats", "匿名统计数据"))
        .description(tr("Key Aggregation and Voice Duration will not be uploaded and will not be imported or exported with Key Profile.", "按键聚合和语音时长不会上传，也不会随按键 Profile 导入或导出。"))
        .build();
    let clear_button = gtk::Button::with_label(tr("Clear Stats...", "清空统计…"));
    clear_button.add_css_class("destructive-action");
    clear_button.set_valign(Align::Center);
    let clear_row = adw::ActionRow::builder()
        .title(tr("Clear all local statistics", "清空全部本机统计"))
        .subtitle(tr("Permanently delete key aggregation and speech duration metadata; Handy recording files are not deleted", "永久删除按键聚合和语音时长元数据；不会删除 Handy 录音文件"))
        .build();
    clear_row.add_suffix(&clear_button);
    management_group.add(&clear_row);

    let ui = StatisticsUi {
        period: period.clone(),
        status,
        button_presses,
        voice_sessions,
        voice_duration,
        button_ranking,
        voice_ranking,
        clear_button: clear_button.clone(),
        transcript_status,
        transcript_switch: transcript_switch.clone(),
        transcript_count,
        transcript_import_button: transcript_import_button.clone(),
        transcript_export_button: transcript_export_button.clone(),
        transcript_clear_button: transcript_clear_button.clone(),
        transcript_refreshing: Rc::new(Cell::new(false)),
    };
    for (button, selected_period) in [
        (today_button, StatisticsPeriod::Today),
        (week_button, StatisticsPeriod::Week),
        (all_button, StatisticsPeriod::All),
    ] {
        button.connect_toggled({
            let ui = ui.clone();
            move |button| {
                if button.is_active() {
                    ui.period.set(selected_period);
                    refresh_statistics_ui(&ui);
                }
            }
        });
    }
    refresh_button.connect_clicked({
        let ui = ui.clone();
        move |_| refresh_statistics_ui(&ui)
    });
    clear_button.connect_clicked({
        let ui = ui.clone();
        move |button| confirm_clear_statistics(button, &ui)
    });
    transcript_switch.connect_active_notify({
        let ui = ui.clone();
        move |row| {
            if !ui.transcript_refreshing.get() {
                update_transcript_archive_setting(&ui, row.is_active());
            }
        }
    });
    transcript_import_button.connect_clicked({
        let ui = ui.clone();
        move |button| choose_transcript_import(button, &ui)
    });
    transcript_export_button.connect_clicked({
        let ui = ui.clone();
        move |button| choose_transcript_export(button, &ui)
    });
    transcript_clear_button.connect_clicked({
        let ui = ui.clone();
        move |button| confirm_clear_transcripts(button, &ui)
    });
    refresh_statistics_ui(&ui);

    vec![
        period_group,
        summary_group,
        button_group,
        voice_group,
        transcript_group,
        management_group,
    ]
}

fn statistics_value_label() -> gtk::Label {
    let label = gtk::Label::new(Some("—"));
    label.set_valign(Align::Center);
    label.add_css_class("heading");
    label
}

fn statistics_ranking_rows(group: &adw::PreferencesGroup) -> Vec<(adw::ActionRow, gtk::Label)> {
    (0..10)
        .map(|_| {
            let value = gtk::Label::new(None);
            value.set_valign(Align::Center);
            value.add_css_class("dim-label");
            let row = adw::ActionRow::new();
            row.add_suffix(&value);
            row.set_visible(false);
            group.add(&row);
            (row, value)
        })
        .collect()
}

fn refresh_statistics_ui(ui: &StatisticsUi) {
    match load_statistics_summary(ui.period.get()) {
        Ok(summary) => {
            set_profile_status(
                &ui.status,
                tr("Read local statistics", "已读取本机统计"),
                "success",
            );
            ui.button_presses
                .set_text(&trf!("{} times", "{} 次", summary.button_presses));
            ui.voice_sessions
                .set_text(&trf!("{} times", "{} 次", summary.voice_sessions));
            ui.voice_duration
                .set_text(&format_statistics_duration(summary.voice_duration_ms));
            refresh_button_ranking(&ui.button_ranking, &summary);
            refresh_voice_ranking(&ui.voice_ranking, &summary);
            ui.clear_button.set_sensitive(true);
        }
        Err(error) => {
            set_profile_status(
                &ui.status,
                &trf!("Read failed: {error}", "读取失败：{error}"),
                "error",
            );
            ui.button_presses.set_text("—");
            ui.voice_sessions.set_text("—");
            ui.voice_duration.set_text("—");
            show_statistics_error_row(&ui.button_ranking);
            show_statistics_error_row(&ui.voice_ranking);
            ui.clear_button.set_sensitive(false);
        }
    }
    refresh_transcript_ui(ui);
}

fn load_statistics_summary(period: StatisticsPeriod) -> Result<StatisticsSummary, String> {
    let paths = StatisticsPaths::from_xdg_environment().map_err(|error| error.to_string())?;
    let database = StatisticsDatabase::open(&paths.database).map_err(|error| error.to_string())?;
    database
        .summary(period, Local::now().timestamp())
        .map_err(|error| error.to_string())
}

fn refresh_transcript_ui(ui: &StatisticsUi) {
    ui.transcript_refreshing.set(true);
    match load_transcript_state() {
        Ok((enabled, count)) => {
            ui.transcript_switch.set_active(enabled);
            ui.transcript_switch.set_sensitive(true);
            ui.transcript_count.set_text(&trf!("{count}", "{count} 条"));
            ui.transcript_import_button.set_sensitive(true);
            ui.transcript_export_button.set_sensitive(count > 0);
            ui.transcript_clear_button.set_sensitive(count > 0);
            set_profile_status(
                &ui.transcript_status,
                if enabled {
                    tr("Turned on, saved on machine only", "已开启，只保存在本机")
                } else {
                    tr("Closed by default", "默认关闭")
                },
                if enabled { "success" } else { "dim-label" },
            );
        }
        Err(error) => {
            ui.transcript_switch.set_sensitive(false);
            ui.transcript_count.set_text("—");
            ui.transcript_import_button.set_sensitive(false);
            ui.transcript_export_button.set_sensitive(false);
            ui.transcript_clear_button.set_sensitive(false);
            set_profile_status(
                &ui.transcript_status,
                &trf!(
                    "Text file not available: {error}",
                    "文字档案不可用：{error}"
                ),
                "error",
            );
        }
    }
    ui.transcript_refreshing.set(false);
}

fn load_transcript_state() -> Result<(bool, u64), String> {
    let paths = TranscriptPaths::from_xdg_environment().map_err(|error| error.to_string())?;
    let database = TranscriptDatabase::open(&paths.database).map_err(|error| error.to_string())?;
    Ok((
        database
            .archive_enabled()
            .map_err(|error| error.to_string())?,
        database.entry_count().map_err(|error| error.to_string())?,
    ))
}

fn update_transcript_archive_setting(ui: &StatisticsUi, enabled: bool) {
    ui.transcript_switch.set_sensitive(false);
    set_profile_status(
        &ui.transcript_status,
        if enabled {
            tr(
                "Turning on and syncing Handy local history…",
                "正在开启并同步 Handy 本机历史…",
            )
        } else {
            tr(
                "Turning off follow-up text collection...",
                "正在关闭后续文字采集…",
            )
        },
        "dim-label",
    );
    let result = (|| -> Result<(u64, Option<String>), String> {
        let paths = TranscriptPaths::from_xdg_environment().map_err(|error| error.to_string())?;
        let database =
            TranscriptDatabase::open(&paths.database).map_err(|error| error.to_string())?;
        database
            .set_archive_enabled(enabled)
            .map_err(|error| error.to_string())?;
        let sync_error = if enabled {
            database
                .import_handy_history(
                    &paths.handy_history,
                    &paths.statistics_database,
                    SystemTime::now(),
                )
                .err()
                .map(|error| error.to_string())
        } else {
            None
        };
        let count = database.entry_count().map_err(|error| error.to_string())?;
        Ok((count, sync_error))
    })();
    refresh_transcript_ui(ui);
    match result {
        Ok((count, None)) => {
            let message = if enabled {
                trf!(
                    "Turned on; currently {count} saved",
                    "已开启；当前保存 {count} 条"
                )
            } else {
                tr(
                    "Closed; existing records are still stored on the machine",
                    "已关闭；已有记录仍保存在本机",
                )
                .into()
            };
            set_profile_status(
                &ui.transcript_status,
                &message,
                if enabled { "success" } else { "dim-label" },
            );
        }
        Ok((_, Some(error))) => set_profile_status(
            &ui.transcript_status,
            &trf!(
                "Turned on, but Handy sync failed: {error}",
                "已开启，但 Handy 同步失败：{error}"
            ),
            "error",
        ),
        Err(error) => set_profile_status(
            &ui.transcript_status,
            &trf!("Failed to save switch: {error}", "保存开关失败：{error}"),
            "error",
        ),
    }
}

fn transcript_json_dialog(title: &str, initial_name: Option<&str>) -> gtk::FileDialog {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(tr(
        "OmaVoice Text Files JSON",
        "OmaVoice 文字档案 JSON",
    )));
    filter.add_mime_type("application/json");
    filter.add_pattern("*.json");
    let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let builder = gtk::FileDialog::builder()
        .title(title)
        .modal(true)
        .filters(&filters)
        .default_filter(&filter);
    match initial_name {
        Some(initial_name) => builder.initial_name(initial_name).build(),
        None => builder.build(),
    }
}

fn choose_transcript_import(button: &gtk::Button, ui: &StatisticsUi) {
    let parent = button
        .root()
        .and_then(|root| root.downcast::<gtk::Window>().ok());
    let dialog = transcript_json_dialog(
        tr("Import OmaVoice Text File", "导入 OmaVoice 文字档案"),
        None,
    );
    ui.transcript_import_button.set_sensitive(false);
    set_profile_status(
        &ui.transcript_status,
        tr("Please select a JSON file", "请选择 JSON 文件"),
        "dim-label",
    );
    let callback_parent = parent.clone();
    dialog.open(parent.as_ref(), None::<&gtk::gio::Cancellable>, {
        let ui = ui.clone();
        move |result| match result {
            Ok(file) => {
                let Some(path) = file.path() else {
                    refresh_transcript_ui(&ui);
                    set_profile_status(
                        &ui.transcript_status,
                        tr(
                            "Import failed: only local files are supported",
                            "导入失败：只支持本机文件",
                        ),
                        "error",
                    );
                    return;
                };
                preview_transcript_import(callback_parent, &ui, &path);
            }
            Err(error) if error.matches(gtk::gio::IOErrorEnum::Cancelled) => {
                refresh_transcript_ui(&ui);
            }
            Err(error) => {
                refresh_transcript_ui(&ui);
                set_profile_status(
                    &ui.transcript_status,
                    &trf!("Failed to select file: {error}", "选择文件失败：{error}"),
                    "error",
                );
            }
        }
    });
}

fn preview_transcript_import(parent: Option<gtk::Window>, ui: &StatisticsUi, path: &Path) {
    let result = (|| -> Result<(TranscriptArchive, omavoice_linux::transcripts::TranscriptImportPreview), String> {
        let archive = read_archive_file(path, SystemTime::now()).map_err(|error| error.to_string())?;
        let paths = TranscriptPaths::from_xdg_environment().map_err(|error| error.to_string())?;
        let database = TranscriptDatabase::open(&paths.database).map_err(|error| error.to_string())?;
        let preview = database
            .preview_import(&archive, SystemTime::now())
            .map_err(|error| error.to_string())?;
        Ok((archive, preview))
    })();
    let (archive, preview) = match result {
        Ok(result) => result,
        Err(error) => {
            refresh_transcript_ui(ui);
            set_profile_status(
                &ui.transcript_status,
                &trf!("Failed to import: {error}", "无法导入：{error}"),
                "error",
            );
            return;
        }
    };
    let heading = if preview.new_entries == 0 {
        tr("No transcripts to add", "没有可新增的文字记录")
    } else {
        tr("Import text file?", "导入文字档案？")
    };
    let body = trf!(
        "A total of {} files; {} will be added, skipping {} duplicate records.Transcript text is written only to local transcripts.db.",
        "文件共 {} 条；将新增 {} 条，跳过 {} 条重复记录。正文只写入本机 transcripts.db。",
        preview.total,
        preview.new_entries,
        preview.duplicates
    );
    let dialog = adw::MessageDialog::new(parent.as_ref(), Some(heading), Some(&body));
    dialog.add_responses(&[
        ("cancel", tr("Cancel", "取消")),
        ("import", tr("Confirm Import", "确认导入")),
    ]);
    dialog.set_close_response("cancel");
    dialog.set_default_response(Some("cancel"));
    dialog.set_response_enabled("import", preview.new_entries > 0);
    dialog.connect_response(Some("cancel"), {
        let ui = ui.clone();
        move |_, _| refresh_transcript_ui(&ui)
    });
    dialog.connect_response(Some("import"), {
        let ui = ui.clone();
        move |_, _| import_transcript_archive(&ui, &archive)
    });
    dialog.present();
}

fn import_transcript_archive(ui: &StatisticsUi, archive: &TranscriptArchive) {
    let result = TranscriptPaths::from_xdg_environment()
        .map_err(|error| error.to_string())
        .and_then(|paths| {
            TranscriptDatabase::open(&paths.database).map_err(|error| error.to_string())
        })
        .and_then(|database| {
            database
                .import_archive(archive, SystemTime::now())
                .map_err(|error| error.to_string())
        });
    refresh_transcript_ui(ui);
    match result {
        Ok(imported) => set_profile_status(
            &ui.transcript_status,
            &trf!(
                "{imported} imported; auto collect switch remains unchanged",
                "已导入 {imported} 条；自动采集开关保持不变"
            ),
            "success",
        ),
        Err(error) => set_profile_status(
            &ui.transcript_status,
            &trf!("Import failed: {error}", "导入失败：{error}"),
            "error",
        ),
    }
}

fn choose_transcript_export(button: &gtk::Button, ui: &StatisticsUi) {
    let parent = button
        .root()
        .and_then(|root| root.downcast::<gtk::Window>().ok());
    let initial_name = format!(
        "omavoice-transcripts-{}.json",
        Local::now().format("%Y-%m-%d")
    );
    let dialog = transcript_json_dialog(
        tr("Export OmaVoice Text File", "导出 OmaVoice 文字档案"),
        Some(&initial_name),
    );
    ui.transcript_export_button.set_sensitive(false);
    set_profile_status(
        &ui.transcript_status,
        tr("Please select an export location", "请选择导出位置"),
        "dim-label",
    );
    dialog.save(parent.as_ref(), None::<&gtk::gio::Cancellable>, {
        let ui = ui.clone();
        move |result| match result {
            Ok(file) => {
                let Some(path) = file.path() else {
                    refresh_transcript_ui(&ui);
                    set_profile_status(
                        &ui.transcript_status,
                        tr(
                            "Export failed: only local files are supported",
                            "导出失败：只支持本机文件",
                        ),
                        "error",
                    );
                    return;
                };
                export_transcript_archive(&ui, &path);
            }
            Err(error) if error.matches(gtk::gio::IOErrorEnum::Cancelled) => {
                refresh_transcript_ui(&ui);
            }
            Err(error) => {
                refresh_transcript_ui(&ui);
                set_profile_status(
                    &ui.transcript_status,
                    &trf!(
                        "Failed to select export location: {error}",
                        "选择导出位置失败：{error}"
                    ),
                    "error",
                );
            }
        }
    });
}

fn export_transcript_archive(ui: &StatisticsUi, path: &Path) {
    let result = (|| -> Result<usize, String> {
        let paths = TranscriptPaths::from_xdg_environment().map_err(|error| error.to_string())?;
        let database =
            TranscriptDatabase::open(&paths.database).map_err(|error| error.to_string())?;
        let archive = database
            .export_archive(SystemTime::now())
            .map_err(|error| error.to_string())?;
        let count = archive.entries.len();
        write_archive_file(path, &archive).map_err(|error| error.to_string())?;
        Ok(count)
    })();
    refresh_transcript_ui(ui);
    match result {
        Ok(count) => set_profile_status(
            &ui.transcript_status,
            &trf!(
                "{count} exported to selected JSON file",
                "已导出 {count} 条到所选 JSON 文件"
            ),
            "success",
        ),
        Err(error) => set_profile_status(
            &ui.transcript_status,
            &trf!("Export failed: {error}", "导出失败：{error}"),
            "error",
        ),
    }
}

fn confirm_clear_transcripts(button: &gtk::Button, ui: &StatisticsUi) {
    let parent = button
        .root()
        .and_then(|root| root.downcast::<gtk::Window>().ok());
    let dialog = adw::MessageDialog::new(
        parent.as_ref(),
        Some(tr("Clear all local text files?", "清空全部本机文字档案？")),
        Some(tr(
            "The transcripts saved by OmaVoice will be permanently deleted; anonymous statistics, Handy history, and recordings will not be deleted, and old records will not be re-imported in the background.",
            "OmaVoice 保存的正文将永久删除；匿名统计、Handy 历史和录音不会删除，旧记录也不会在后台重新导入。",
        )),
    );
    dialog.add_responses(&[
        ("cancel", tr("Cancel", "取消")),
        ("clear", tr("Confirm Empty", "确认清空")),
    ]);
    dialog.set_close_response("cancel");
    dialog.set_default_response(Some("cancel"));
    dialog.set_response_appearance("clear", adw::ResponseAppearance::Destructive);
    dialog.connect_response(Some("clear"), {
        let ui = ui.clone();
        move |_, _| clear_transcripts(&ui)
    });
    dialog.present();
}

fn clear_transcripts(ui: &StatisticsUi) {
    let result = TranscriptPaths::from_xdg_environment()
        .map_err(|error| error.to_string())
        .and_then(|paths| {
            TranscriptDatabase::open(&paths.database).map_err(|error| error.to_string())
        })
        .and_then(|database| database.clear().map_err(|error| error.to_string()));
    refresh_transcript_ui(ui);
    match result {
        Ok(()) => set_profile_status(
            &ui.transcript_status,
            tr(
                "Empty text archive; new voice after emptying is still handled by the current switch",
                "已清空文字档案；清空后的新语音仍按当前开关处理",
            ),
            "success",
        ),
        Err(error) => set_profile_status(
            &ui.transcript_status,
            &trf!("Emptying failed: {error}", "清空失败：{error}"),
            "error",
        ),
    }
}

fn refresh_button_ranking(rows: &[(adw::ActionRow, gtk::Label)], summary: &StatisticsSummary) {
    hide_statistics_rows(rows);
    if summary.button_counts.is_empty() {
        show_statistics_empty_row(
            rows,
            tr(
                "There is no key data in the current range",
                "当前范围暂无按键数据",
            ),
        );
        return;
    }
    for ((row, value), count) in rows.iter().zip(&summary.button_counts) {
        row.set_title(&statistics_key_label(&count.key));
        row.set_subtitle(&trf!("Output key: {}", "输出键：{}", count.key));
        value.set_text(&trf!("{} times", "{} 次", count.count));
        row.set_visible(true);
    }
}

fn refresh_voice_ranking(rows: &[(adw::ActionRow, gtk::Label)], summary: &StatisticsSummary) {
    hide_statistics_rows(rows);
    if summary.longest_voice_sessions.is_empty() {
        show_statistics_empty_row(
            rows,
            tr(
                "There is no voice data in the current range",
                "当前范围暂无语音数据",
            ),
        );
        return;
    }
    for (index, ((row, value), session)) in
        rows.iter().zip(&summary.longest_voice_sessions).enumerate()
    {
        row.set_title(&format_statistics_duration(session.duration_ms));
        row.set_subtitle(&format_statistics_timestamp(session.started_at));
        value.set_text(&trf!("No. {}", "第 {}", index + 1));
        row.set_visible(true);
    }
}

fn hide_statistics_rows(rows: &[(adw::ActionRow, gtk::Label)]) {
    for (row, value) in rows {
        row.set_visible(false);
        row.set_title("");
        row.set_subtitle("");
        value.set_text("");
    }
}

fn show_statistics_empty_row(rows: &[(adw::ActionRow, gtk::Label)], message: &str) {
    let Some((row, _)) = rows.first() else { return };
    row.set_title(message);
    row.set_subtitle(tr(
        "Refresh after the remote control button or voice action to view",
        "完成遥控器按键或语音操作后刷新即可查看",
    ));
    row.set_visible(true);
}

fn show_statistics_error_row(rows: &[(adw::ActionRow, gtk::Label)]) {
    hide_statistics_rows(rows);
    let Some((row, _)) = rows.first() else { return };
    row.set_title(tr("Stats Temporarily Unavailable", "统计暂时不可用"));
    row.set_subtitle(tr(
        "Please check XDG data directory and omavoice-statistics user service",
        "请检查 XDG 数据目录和 omavoice-statistics 用户服务",
    ));
    row.set_visible(true);
}

fn confirm_clear_statistics(button: &gtk::Button, ui: &StatisticsUi) {
    let parent = button
        .root()
        .and_then(|root| root.downcast::<gtk::Window>().ok());
    let dialog = adw::MessageDialog::new(
        parent.as_ref(),
        Some(tr("Clear all local statistics?", "清空全部本机统计？")),
        Some(tr(
            "Key aggregation and speech duration metadata will be permanently deleted; Handy recordings will not be deleted.",
            "按键聚合和语音时长元数据将永久删除；Handy 录音文件不会删除。",
        )),
    );
    dialog.add_responses(&[
        ("cancel", tr("Cancel", "取消")),
        ("clear", tr("Confirm Empty", "确认清空")),
    ]);
    dialog.set_close_response("cancel");
    dialog.set_default_response(Some("cancel"));
    dialog.set_response_appearance("clear", adw::ResponseAppearance::Destructive);
    dialog.connect_response(Some("clear"), {
        let ui = ui.clone();
        move |_, _| clear_statistics(&ui)
    });
    dialog.present();
}

fn clear_statistics(ui: &StatisticsUi) {
    let result = StatisticsPaths::from_xdg_environment()
        .map_err(|error| error.to_string())
        .and_then(|paths| {
            StatisticsDatabase::open(&paths.database).map_err(|error| error.to_string())
        })
        .and_then(|database| database.clear().map_err(|error| error.to_string()));
    match result {
        Ok(()) => {
            refresh_statistics_ui(ui);
            set_profile_status(
                &ui.status,
                tr("Local statistics cleared", "已清空本机统计"),
                "success",
            );
        }
        Err(error) => set_profile_status(
            &ui.status,
            &trf!("Emptying failed: {error}", "清空失败：{error}"),
            "error",
        ),
    }
}

fn statistics_key_label(key: &str) -> String {
    match key {
        "up" => tr("Up", "上").into(),
        "down" => tr("Down", "下").into(),
        "left" => tr("Left", "左").into(),
        "right" => tr("Right", "右").into(),
        "enter" => tr("OK/Enter", "确定 / Enter").into(),
        "esc" => "Esc".into(),
        "backspace" => tr("Backspace", "退格").into(),
        "home" => tr("Home", "主页 / Home").into(),
        "compose" => tr("Menu", "菜单").into(),
        "volumeup" => tr("Volume +", "音量+").into(),
        "volumedown" => tr("Volume -", "音量−").into(),
        "mute" => tr("Mute", "静音").into(),
        "playpause" => tr("Play/Pause", "播放 / 暂停").into(),
        _ if key.len() == 1
            || key.strip_prefix('f').is_some_and(|number| {
                !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
            }) =>
        {
            key.to_ascii_uppercase()
        }
        _ => key.replace('_', " "),
    }
}

fn format_statistics_duration(duration_ms: u64) -> String {
    let tenths = duration_ms.saturating_add(50) / 100;
    let total_seconds = tenths / 10;
    let tenth = tenths % 10;
    let hours = total_seconds / 3_600;
    let minutes = total_seconds % 3_600 / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        trf!(
            "{hours} hr {minutes} min {seconds} sec",
            "{hours} 小时 {minutes} 分 {seconds} 秒"
        )
    } else if minutes > 0 {
        trf!("{minutes} min {seconds} sec", "{minutes} 分 {seconds} 秒")
    } else {
        trf!("{total_seconds}.{tenth} sec", "{total_seconds}.{tenth} 秒")
    }
}

fn format_statistics_timestamp(timestamp: i64) -> String {
    Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|time| time.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| tr("Unknown time", "时间未知").into())
}

fn build_bluetooth_group() -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title(tr("System Bluetooth", "系统蓝牙"))
        .description(tr("Pairing, connection and scanning continue to be the responsibility of BlueZ with the desktop system interface, and OmaVoice does not take over the controller drive.", "配对、连接和扫描继续由 BlueZ 与桌面系统界面负责，OmaVoice 不接管控制器驱动。"))
        .build();
    let launcher = bluetooth_launcher();
    add_launcher_row(
        &group,
        tr("Bluetooth Device Management", "蓝牙设备管理"),
        if launcher == Some(OMARCHY_BLUETOOTH) {
            tr(
                "Open the same Bluetooth panel used by the Omarchy top bar.",
                "打开 Omarchy 顶栏使用的同一个蓝牙面板。",
            )
        } else if launcher.is_some() {
            tr(
                "Open the open source Bluetooth management interface installed on the current desktop.",
                "打开当前桌面已安装的开源蓝牙管理界面。",
            )
        } else {
            tr(
                "No openable graphical Bluetooth interface detected; bluetoothctl is still available.",
                "没有检测到可打开的图形蓝牙界面；仍可使用 bluetoothctl。",
            )
        },
        tr("Open Bluetooth panel", "打开蓝牙面板"),
        launcher,
    );
    group
}

fn build_handy_group() -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title(tr("Dictation backend", "听写后端"))
        .description(
            tr("Handy has model, API, language, history, and floating interface settings; OmaVoice does not copy or override its private configuration.", "Handy 拥有模型、API、语言、历史和悬浮界面设置；OmaVoice 不复制或改写其私有配置。"),
        )
        .build();
    let launcher = handy_launcher();
    add_launcher_row(
        &group,
        tr("Handy Settings", "Handy 设置"),
        if launcher.is_some() {
            tr(
                "Opens or recalls the Handy main window; it is handled by its single-instance mechanism when it is already running.",
                "打开或唤回 Handy 主窗口；已运行时由其单实例机制处理。",
            )
        } else {
            tr(
                "Handy is not installed; OmaVoice does not automatically install components without confirmation.",
                "Handy 尚未安装；OmaVoice 不会在没有确认时自动安装组件。",
            )
        },
        if launcher.is_some() {
            tr("Open Handy", "打开 Handy")
        } else {
            tr("Not Installed", "尚未安装")
        },
        launcher,
    );
    group
}

fn build_diagnostics_group(report: &DoctorReport) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title(tr("Diagnostic Report", "诊断报告"))
        .description(tr("Explicitly generate the current doctor JSON to XDG state directory; does not contain a Bluetooth address, temporary device node, transcript, or audio.", "显式生成当前 doctor JSON 到 XDG 状态目录；不包含蓝牙地址、临时设备节点、语音正文或音频。"))
        .build();
    let status = gtk::Label::new(Some(tr("Not yet generated", "尚未生成")));
    status.set_wrap(true);
    status.set_xalign(1.0);
    status.add_css_class("dim-label");
    let button = gtk::Button::with_label(tr("Generate and open directories", "生成并打开目录"));
    button.set_valign(Align::Center);
    button.connect_clicked({
        let report = report.clone();
        let status = status.clone();
        move |_| export_and_open_diagnostics(&report, &status)
    });
    let controls = gtk::Box::new(Orientation::Horizontal, 12);
    controls.set_valign(Align::Center);
    controls.append(&status);
    controls.append(&button);
    let row = adw::ActionRow::builder()
        .title(tr("OmaVoice capability report", "OmaVoice 能力报告"))
        .subtitle(tr("XDG Status Directory/omavoice/diagnostics/omavoice-doctor.json · Schema 1 · Permissions 0600", "XDG 状态目录/omavoice/diagnostics/omavoice-doctor.json · Schema 1 · 权限 0600"))
        .build();
    row.add_suffix(&controls);
    group.add(&row);

    let log_directory = log_directory_from_environment()
        .ok()
        .filter(|directory| directory.is_dir());
    let log_status = gtk::Label::new(None);
    log_status.set_wrap(true);
    log_status.set_xalign(1.0);
    let log_button = gtk::Button::with_label(if log_directory.is_some() {
        tr("Open Log Directory", "打开日志目录")
    } else {
        tr("No running logs yet", "尚无运行日志")
    });
    log_button.set_sensitive(log_directory.is_some() && command_exists("xdg-open"));
    log_button.set_valign(Align::Center);
    if let Some(directory) = log_directory {
        log_button.connect_clicked({
            let log_status = log_status.clone();
            move |_| {
                let mut command = Command::new("xdg-open");
                command.arg(&directory);
                match spawn_and_reap(&mut command) {
                    Ok(()) => set_profile_status(&log_status, tr("Opened", "已打开"), "success"),
                    Err(error) => set_profile_status(
                        &log_status,
                        &trf!("Open failed: {error}", "打开失败：{error}"),
                        "error",
                    ),
                }
            }
        });
    }
    let log_controls = gtk::Box::new(Orientation::Horizontal, 12);
    log_controls.set_valign(Align::Center);
    log_controls.append(&log_status);
    log_controls.append(&log_button);
    let log_row = adw::ActionRow::builder()
        .title(tr("OmaVoice Run Log", "OmaVoice 运行日志"))
        .subtitle(tr("XDG status directory/omavoice/logs · Only open after the runtime component actually generates a log", "XDG 状态目录/omavoice/logs · 仅在运行时组件实际产生日志后开放"))
        .build();
    log_row.add_suffix(&log_controls);
    group.add(&log_row);
    group
}

fn add_launcher_row(
    group: &adw::PreferencesGroup,
    title: &str,
    subtitle: &str,
    button_label: &str,
    launcher: Option<LaunchSpec>,
) {
    let status = gtk::Label::new(None);
    status.set_wrap(true);
    status.set_xalign(1.0);
    let button = gtk::Button::with_label(button_label);
    button.set_sensitive(launcher.is_some());
    button.set_valign(Align::Center);
    if let Some(launcher) = launcher {
        button.connect_clicked({
            let status = status.clone();
            move |_| match launch(launcher) {
                Ok(()) => set_profile_status(
                    &status,
                    tr("Open Request Issued", "已发出打开请求"),
                    "success",
                ),
                Err(error) => set_profile_status(
                    &status,
                    &trf!("Open failed: {error}", "打开失败：{error}"),
                    "error",
                ),
            }
        });
    }
    let controls = gtk::Box::new(Orientation::Horizontal, 12);
    controls.set_valign(Align::Center);
    controls.append(&status);
    controls.append(&button);
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .build();
    row.add_suffix(&controls);
    group.add(&row);
}

fn bluetooth_launcher() -> Option<LaunchSpec> {
    if omarchy_bluetooth_available() {
        return Some(OMARCHY_BLUETOOTH);
    }
    [BLUEMAN, OVERSKRIDE, GNOME_BLUETOOTH, KDE_BLUETOOTH]
        .into_iter()
        .find(|launcher| command_exists(launcher.program))
}

fn omarchy_bluetooth_available() -> bool {
    if !command_exists(OMARCHY_BLUETOOTH.program) || !Path::new(OMARCHY_SHELL_PATH).is_dir() {
        return false;
    }
    Command::new(OMARCHY_BLUETOOTH.program)
        .args(["ipc", "-p", OMARCHY_SHELL_PATH, "show"])
        .output()
        .is_ok_and(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout).contains("target omarchy.bluetooth")
        })
}

fn handy_launcher() -> Option<LaunchSpec> {
    [OMAVOICE_HANDY, HANDY_LOWERCASE, HANDY_TITLECASE]
        .into_iter()
        .find(|launcher| command_exists(launcher.program))
}

fn launch(launcher: LaunchSpec) -> Result<(), String> {
    let mut command = Command::new(launcher.program);
    command.args(launcher.arguments);
    spawn_and_reap(&mut command)
}

fn spawn_and_reap(command: &mut Command) -> Result<(), String> {
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn export_and_open_diagnostics(report: &DoctorReport, status: &gtk::Label) {
    let result = diagnostic_directory_from_environment().and_then(|directory| {
        export_diagnostic_report(&directory, report)?;
        Ok(directory)
    });
    match result {
        Ok(directory) => {
            if command_exists("xdg-open") {
                let mut command = Command::new("xdg-open");
                command.arg(directory);
                match spawn_and_reap(&mut command) {
                    Ok(()) => set_profile_status(
                        status,
                        tr("Report generated and folder opened", "已生成并打开目录"),
                        "success",
                    ),
                    Err(error) => set_profile_status(
                        status,
                        &trf!(
                            "Report generated, directory open failed: {error}",
                            "报告已生成，目录打开失败：{error}"
                        ),
                        "error",
                    ),
                }
            } else {
                set_profile_status(
                    status,
                    tr(
                        "Report generated; system does not have xdg-open",
                        "报告已生成；系统没有 xdg-open",
                    ),
                    "success",
                );
            }
        }
        Err(error) => set_profile_status(
            status,
            &trf!("Build failed: {error}", "生成失败：{error}"),
            "error",
        ),
    }
}

fn build_profile_group(settings: &SharedProfiles) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title(tr("Device Profile", "设备 Profile"))
        .description(tr("Saves stable device matches, control sources, voice sources, and transfer types; does not save Bluetooth addresses, eventNs, or digital PipeWire node IDs.", "保存稳定设备匹配、控制来源、语音来源和传输类型；不会保存蓝牙地址、eventN 或数字 PipeWire node ID。"))
        .build();

    let snapshot = settings.borrow();
    let Some(config) = snapshot.config.as_ref() else {
        let row = adw::ActionRow::builder()
            .title(tr(
                "Local configuration could not be read",
                "本机配置无法读取",
            ))
            .subtitle(snapshot.error.as_deref().unwrap_or(tr(
                "Unknown configuration error; original file not overwritten",
                "未知配置错误；原文件未被覆盖",
            )))
            .build();
        row.add_prefix(&gtk::Image::from_icon_name("dialog-error-symbolic"));
        group.add(&row);
        return group;
    };
    let Some(profile) = config.selected_profile().cloned() else {
        let row = adw::ActionRow::builder()
            .title(tr(
                "No editable device profiles",
                "没有可编辑的设备 Profile",
            ))
            .subtitle(tr(
                "Please fix selected_profile_id first; the original profile was not overwritten.",
                "请先修复 selected_profile_id；原配置文件未被覆盖。",
            ))
            .build();
        row.add_prefix(&gtk::Image::from_icon_name("dialog-error-symbolic"));
        group.add(&row);
        return group;
    };
    let initial_status = if snapshot.dirty {
        tr("There are unsaved changes", "有未保存更改")
    } else if snapshot.persisted {
        tr("Loaded from local configuration", "已从本机配置加载")
    } else {
        tr(
            "Default Profile has not been saved",
            "默认 Profile 尚未保存",
        )
    };
    drop(snapshot);

    let status = gtk::Label::new(Some(initial_status));
    status.set_wrap(true);
    status.set_xalign(1.0);
    status.add_css_class("dim-label");
    let save_button = gtk::Button::with_label(tr("Save Profile", "保存 Profile"));
    save_button.add_css_class("suggested-action");
    save_button.set_valign(Align::Center);
    save_button.connect_clicked({
        let settings = settings.clone();
        let status = status.clone();
        move |_| save_profile(&settings, &status)
    });
    let status_controls = gtk::Box::new(Orientation::Horizontal, 12);
    status_controls.set_valign(Align::Center);
    status_controls.append(&status);
    status_controls.append(&save_button);
    let status_row = adw::ActionRow::builder()
        .title(tr("Local Configuration", "本机配置"))
        .subtitle(tr(
            "Write-Only XDG User Configuration · Schema 2 · Atomic Write · File Permissions 0600",
            "只写 XDG 用户配置 · Schema 2 · 原子写入 · 文件权限 0600",
        ))
        .build();
    status_row.add_suffix(&status_controls);
    group.add(&status_row);

    let name_row = adw::EntryRow::builder()
        .title(tr("Profile Name", "Profile 名称"))
        .text(&profile.display_name)
        .build();
    name_row.connect_changed({
        let settings = settings.clone();
        let status = status.clone();
        move |row| {
            update_selected_profile(&settings, &status, |profile| {
                profile.display_name = row.text().to_string();
            });
        }
    });
    group.add(&name_row);

    let matcher = adw::ActionRow::builder()
        .title(tr("Stabilize device matching", "稳定设备匹配"))
        .subtitle(matcher_summary(&profile))
        .build();
    group.add(&matcher);

    let control = adw::ActionRow::builder()
        .title(tr("Control Source", "控制来源"))
        .build();
    add_choice_buttons(
        &control,
        &[
            ("evdev", ControlSource::Evdev),
            (tr("Do not use", "不使用"), ControlSource::Disabled),
        ],
        profile.control_source,
        {
            let settings = settings.clone();
            let status = status.clone();
            move |choice| {
                update_selected_profile(&settings, &status, |profile| {
                    profile.control_source = choice;
                });
            }
        },
    );
    group.add(&control);

    let voice = adw::ActionRow::builder()
        .title(tr("Voice Source", "语音来源"))
        .build();
    add_choice_buttons(
        &voice,
        &[
            ("ATVVoice", VoiceSource::Atvv),
            ("PipeWire", VoiceSource::PipeWire),
            (tr("Do not use", "不使用"), VoiceSource::Disabled),
        ],
        profile.voice_source,
        {
            let settings = settings.clone();
            let status = status.clone();
            move |choice| {
                update_selected_profile(&settings, &status, |profile| {
                    profile.voice_source = choice;
                });
            }
        },
    );
    group.add(&voice);

    let transport = adw::ActionRow::builder()
        .title(tr("Transmission type", "传输类型"))
        .build();
    add_choice_buttons(
        &transport,
        &[
            ("BLE", Transport::Ble),
            (
                tr("Classic Bluetooth", "经典蓝牙"),
                Transport::BluetoothClassic,
            ),
            ("USB", Transport::Usb),
            ("2.4G", Transport::Receiver2_4Ghz),
        ],
        profile.transport,
        {
            let settings = settings.clone();
            let status = status.clone();
            move |choice| {
                update_selected_profile(&settings, &status, |profile| {
                    profile.transport = choice;
                });
            }
        },
    );
    group.add(&transport);

    let ptt = adw::ActionRow::builder()
        .title(tr("Voice Trigger Key", "语音触发键"))
        .subtitle(tr(
            "Fixed use of F20 before phase 3 access control; does not emulate macOS Fn.",
            "阶段 3 门禁前固定使用 F20；不模拟 macOS Fn。",
        ))
        .build();
    ptt.add_suffix(&gtk::Label::new(Some(&profile.ptt_key)));
    group.add(&ptt);

    group
}

fn build_button_mapping_groups(settings: &SharedProfiles) -> Vec<adw::PreferencesGroup> {
    let selector_group = adw::PreferencesGroup::builder()
        .title(tr("Remote control buttons", "遥控器按键"))
        .description(tr("Select the keys and actions; save only the Profile, or review the preview and authorize it to be applied to the system.", "选择按键和动作；可仅保存 Profile，也可审阅预览后授权应用到系统。"))
        .build();

    let snapshot = settings.borrow();
    let Some(config) = snapshot.config.as_ref() else {
        let row = adw::ActionRow::builder()
            .title(tr(
                "Local configuration could not be read",
                "本机配置无法读取",
            ))
            .subtitle(snapshot.error.as_deref().unwrap_or(tr(
                "Unknown configuration error; original file not overwritten",
                "未知配置错误；原文件未被覆盖",
            )))
            .build();
        row.add_prefix(&gtk::Image::from_icon_name("dialog-error-symbolic"));
        selector_group.add(&row);
        return vec![selector_group];
    };
    let Some(profile) = config.selected_profile().cloned() else {
        let row = adw::ActionRow::builder()
            .title(tr(
                "No editable device profiles",
                "没有可编辑的设备 Profile",
            ))
            .subtitle(tr(
                "Please fix selected_profile_id first; the original profile was not overwritten.",
                "请先修复 selected_profile_id；原配置文件未被覆盖。",
            ))
            .build();
        row.add_prefix(&gtk::Image::from_icon_name("dialog-error-symbolic"));
        selector_group.add(&row);
        return vec![selector_group];
    };
    let initial_status = if snapshot.dirty {
        tr("Unsaved", "未保存")
    } else if snapshot.persisted {
        tr("Loaded from local configuration", "已从本机配置加载")
    } else {
        tr("Default mapping has not been saved", "默认映射尚未保存")
    };
    drop(snapshot);

    let status = gtk::Label::new(Some(initial_status));
    status.set_wrap(true);
    status.set_xalign(1.0);
    status.add_css_class("dim-label");
    let save_button = gtk::Button::with_label(tr("Save only", "仅保存"));
    save_button.set_valign(Align::Center);
    save_button.connect_clicked({
        let settings = settings.clone();
        let status = status.clone();
        move |_| {
            save_profile_with_message(
                &settings,
                &status,
                tr("Saved; not yet applied to system", "已保存；尚未应用到系统"),
            )
        }
    });
    let apply_button = gtk::Button::with_label(tr("Save and apply", "保存并应用"));
    apply_button.add_css_class("suggested-action");
    apply_button.set_valign(Align::Center);
    apply_button.connect_clicked({
        let settings = settings.clone();
        let status = status.clone();
        let apply_button = apply_button.clone();
        move |_| start_keyd_apply(&settings, &status, &apply_button)
    });
    let status_controls = gtk::Box::new(Orientation::Horizontal, 12);
    status_controls.set_valign(Align::Center);
    status_controls.append(&status);
    status_controls.append(&save_button);
    status_controls.append(&apply_button);
    let status_row = adw::ActionRow::builder()
        .title(tr("Current Device Profile", "当前设备 Profile"))
        .subtitle(trf!(
            "{} · System app requests admin authorization",
            "{} · 系统应用会请求管理员授权",
            profile.display_name
        ))
        .build();
    status_row.add_suffix(&status_controls);
    selector_group.add(&status_row);

    let preview_group = adw::PreferencesGroup::builder()
        .title(tr("keyd configuration preview", "keyd 配置预览"))
        .description(
            tr("Final content before application; the fixed system assistant will check, backup, replace atoms and reload again, failing to roll back automatically.", "应用前的最终内容；固定系统助手会再次校验、备份、原子替换并 reload，失败自动回滚。"),
        )
        .build();
    let preview_status = gtk::Label::new(None);
    preview_status.set_xalign(1.0);
    let preview_summary = adw::ActionRow::builder()
        .title(tr("Generation Status", "生成状态"))
        .subtitle(tr(
            "Generating from current device profile",
            "正在根据当前设备 Profile 生成",
        ))
        .build();
    preview_summary.add_suffix(&preview_status);
    preview_group.add(&preview_summary);
    let preview_buffer = gtk::TextBuffer::new(None);
    let preview_view = gtk::TextView::with_buffer(&preview_buffer);
    preview_view.set_editable(false);
    preview_view.set_cursor_visible(false);
    preview_view.set_monospace(true);
    preview_view.set_wrap_mode(gtk::WrapMode::None);
    preview_view.set_left_margin(12);
    preview_view.set_right_margin(12);
    preview_view.set_top_margin(12);
    preview_view.set_bottom_margin(12);
    let preview_scroll = gtk::ScrolledWindow::builder()
        .min_content_height(260)
        .hexpand(true)
        .child(&preview_view)
        .build();
    preview_scroll.add_css_class("card");
    preview_group.add(&preview_scroll);
    let preview_ui = KeydPreviewUi {
        buffer: preview_buffer,
        status: preview_status,
        summary: preview_summary,
        apply_button,
    };
    refresh_keyd_preview(settings, &preview_ui);

    let selected_button = Rc::new(Cell::new(RemoteButton::Ok));
    let action_group = adw::PreferencesGroup::builder()
        .title(tr("OK key action", "确定键动作"))
        .description(tr("Power preserves the system menu while keeping the original key; other Power actions are still not applicable.", "Power 保持原键时保留系统菜单；其他 Power 动作仍不可应用。"))
        .build();

    let shortcut_group = adw::PreferencesGroup::builder()
        .title(tr("Custom shortcuts", "自定义快捷键"))
        .description(
            tr("The key combination is sent by the keyd kernel input layer; only Ctrl/Alt/Shift/Super and a whitelist primary key are accepted, no commands or scripts are executed.", "组合键由 keyd 在内核输入层发送；只接受 Ctrl / Alt / Shift / Super 与一个白名单主键，不执行命令或脚本。"),
        )
        .build();
    let modifier_row = adw::ActionRow::builder()
        .title(tr("Modifier key", "修饰键"))
        .subtitle(tr(
            "Multiple choice; cancel all when no modifier key is required",
            "可多选；不需要修饰键时全部取消",
        ))
        .build();
    let modifier_buttons = gtk::Box::new(Orientation::Horizontal, 0);
    modifier_buttons.add_css_class("linked");
    modifier_buttons.set_valign(Align::Center);
    let shortcut_control = gtk::ToggleButton::with_label("Ctrl");
    let shortcut_alt = gtk::ToggleButton::with_label("Alt");
    let shortcut_shift = gtk::ToggleButton::with_label("Shift");
    let shortcut_super = gtk::ToggleButton::with_label("Super");
    for button in [
        &shortcut_control,
        &shortcut_alt,
        &shortcut_shift,
        &shortcut_super,
    ] {
        modifier_buttons.append(button);
    }
    modifier_row.add_suffix(&modifier_buttons);
    shortcut_group.add(&modifier_row);

    let shortcut_key = adw::EntryRow::builder()
        .title(tr("Primary Key", "主键"))
        .build();
    shortcut_key.set_tooltip_text(Some(tr(
        "e.g. P, Space, Enter, Esc, Delete, Page Up,/, F1-F24 (except F20)",
        "例如 P、Space、Enter、Esc、Delete、Page Up、/、F1–F24（F20 除外）",
    )));
    shortcut_group.add(&shortcut_key);

    let shortcut_status = gtk::Label::new(None);
    shortcut_status.set_xalign(1.0);
    let shortcut_save = gtk::Button::with_label(tr("Use this shortcut", "使用此快捷键"));
    shortcut_save.add_css_class("suggested-action");
    shortcut_save.set_valign(Align::Center);
    let shortcut_controls = gtk::Box::new(Orientation::Horizontal, 12);
    shortcut_controls.set_valign(Align::Center);
    shortcut_controls.append(&shortcut_status);
    shortcut_controls.append(&shortcut_save);
    let shortcut_summary = adw::ActionRow::builder()
        .title(tr("Current Portfolio", "当前组合"))
        .subtitle(tr("Not set", "尚未设置"))
        .build();
    shortcut_summary.add_suffix(&shortcut_controls);
    shortcut_group.add(&shortcut_summary);
    let shortcut_ui = ShortcutEditorUi {
        group: shortcut_group,
        control: shortcut_control,
        alt: shortcut_alt,
        shift: shortcut_shift,
        super_key: shortcut_super,
        key: shortcut_key,
        summary: shortcut_summary,
        status: shortcut_status,
    };
    refresh_shortcut_editor(settings, selected_button.get(), &shortcut_ui);
    shortcut_save.connect_clicked({
        let settings = settings.clone();
        let status = status.clone();
        let selected_button = selected_button.clone();
        let preview_ui = preview_ui.clone();
        let shortcut_ui = shortcut_ui.clone();
        move |_| {
            save_custom_shortcut(
                &settings,
                &status,
                &preview_ui,
                selected_button.get(),
                &shortcut_ui,
            )
        }
    });

    let mut first_action_button: Option<gtk::ToggleButton> = None;
    let mut action_buttons = Vec::new();
    for (category, actions) in BUTTON_ACTION_ROWS.iter() {
        let row = adw::ActionRow::builder().title(*category).build();
        let buttons = gtk::Box::new(Orientation::Horizontal, 0);
        buttons.add_css_class("linked");
        buttons.set_valign(Align::Center);
        for (action, label) in actions.iter().copied() {
            let button = gtk::ToggleButton::with_label(label);
            if let Some(first) = first_action_button.as_ref() {
                button.set_group(Some(first));
            } else {
                first_action_button = Some(button.clone());
            }
            button.set_active(action == profile.button_action(selected_button.get()));
            button.connect_toggled({
                let settings = settings.clone();
                let status = status.clone();
                let selected_button = selected_button.clone();
                let preview_ui = preview_ui.clone();
                let shortcut_ui = shortcut_ui.clone();
                move |button| {
                    if button.is_active() {
                        update_button_mapping(
                            &settings,
                            &status,
                            &preview_ui,
                            selected_button.get(),
                            action,
                        );
                        refresh_shortcut_editor(&settings, selected_button.get(), &shortcut_ui);
                    }
                }
            });
            buttons.append(&button);
            action_buttons.push((action, button));
        }
        row.add_suffix(&buttons);
        action_group.add(&row);
    }
    let action_buttons = Rc::new(action_buttons);

    let mut first_remote_button: Option<gtk::ToggleButton> = None;
    for (category, remote_buttons) in REMOTE_BUTTON_ROWS.iter() {
        let row = adw::ActionRow::builder().title(*category).build();
        let buttons = gtk::Box::new(Orientation::Horizontal, 0);
        buttons.add_css_class("linked");
        buttons.set_valign(Align::Center);
        for (remote_button, label) in remote_buttons.iter().copied() {
            let button = gtk::ToggleButton::with_label(label);
            if let Some(first) = first_remote_button.as_ref() {
                button.set_group(Some(first));
            } else {
                first_remote_button = Some(button.clone());
            }
            button.set_active(remote_button == selected_button.get());
            button.connect_toggled({
                let action_buttons = action_buttons.clone();
                let action_group = action_group.clone();
                let selected_button = selected_button.clone();
                let settings = settings.clone();
                let shortcut_ui = shortcut_ui.clone();
                move |button| {
                    if !button.is_active() {
                        return;
                    }
                    selected_button.set(remote_button);
                    action_group.set_title(&trf!("{label} key action", "{label}键动作"));
                    let action = settings
                        .borrow()
                        .config
                        .as_ref()
                        .and_then(LinuxConfig::selected_profile)
                        .map(|profile| profile.button_action(remote_button))
                        .unwrap_or_else(|| remote_button.default_action());
                    for (candidate, action_button) in action_buttons.iter() {
                        action_button.set_active(*candidate == action);
                    }
                    refresh_shortcut_editor(&settings, remote_button, &shortcut_ui);
                }
            });
            buttons.append(&button);
        }
        row.add_suffix(&buttons);
        selector_group.add(&row);
    }

    let mic_row = adw::ActionRow::builder()
        .title(tr("Mic Voice Button", "Mic 语音键"))
        .subtitle(tr("Fixed to speak: RC003 F5 keyd F20 → → Handy; does not participate in normal key action configuration.", "固定为按住说话：RC003 F5 → keyd F20 → Handy；不参与普通按键动作配置。"))
        .build();
    mic_row.add_suffix(&status_label(tr("Fixed", "固定"), true, false));
    selector_group.add(&mic_row);

    vec![
        selector_group,
        action_group,
        shortcut_ui.group.clone(),
        preview_group,
    ]
}

fn refresh_shortcut_editor(
    settings: &SharedProfiles,
    button: RemoteButton,
    shortcut_ui: &ShortcutEditorUi,
) {
    let settings = settings.borrow();
    let Some(profile) = settings
        .config
        .as_ref()
        .and_then(LinuxConfig::selected_profile)
    else {
        shortcut_ui.group.set_visible(false);
        return;
    };
    shortcut_ui
        .group
        .set_visible(profile.button_action(button) == ButtonAction::CustomShortcut);

    if let Some(shortcut) = profile.button_shortcut(button) {
        shortcut_ui.control.set_active(shortcut.control);
        shortcut_ui.alt.set_active(shortcut.alt);
        shortcut_ui.shift.set_active(shortcut.shift);
        shortcut_ui.super_key.set_active(shortcut.super_key);
        shortcut_ui.key.set_text(&shortcut.key);
        shortcut_ui.summary.set_subtitle(&shortcut.display_name());
        set_profile_status(&shortcut_ui.status, tr("Configured", "已配置"), "success");
    } else {
        shortcut_ui.control.set_active(false);
        shortcut_ui.alt.set_active(false);
        shortcut_ui.shift.set_active(false);
        shortcut_ui.super_key.set_active(false);
        shortcut_ui.key.set_text("");
        shortcut_ui.summary.set_subtitle(tr(
            "Select the modifier key and enter a primary key",
            "选择修饰键并输入一个主键",
        ));
        set_profile_status(
            &shortcut_ui.status,
            tr("Waiting for setup", "等待设置"),
            "dim-label",
        );
    }
}

fn save_custom_shortcut(
    settings: &SharedProfiles,
    status: &gtk::Label,
    preview_ui: &KeydPreviewUi,
    button: RemoteButton,
    shortcut_ui: &ShortcutEditorUi,
) {
    let shortcut = match KeyboardShortcut::from_input(
        shortcut_ui.control.is_active(),
        shortcut_ui.alt.is_active(),
        shortcut_ui.shift.is_active(),
        shortcut_ui.super_key.is_active(),
        shortcut_ui.key.text().as_str(),
    ) {
        Ok(shortcut) => shortcut,
        Err(error) => {
            shortcut_ui.summary.set_subtitle(&error);
            set_profile_status(&shortcut_ui.status, tr("Unavailable", "无法使用"), "error");
            return;
        }
    };

    let mut state = settings.borrow_mut();
    let Some(profile) = state
        .config
        .as_mut()
        .and_then(LinuxConfig::selected_profile_mut)
    else {
        return;
    };
    let changed = profile.button_shortcut(button) != Some(&shortcut);
    if changed {
        profile.button_shortcuts.insert(button, shortcut);
        state.dirty = true;
    }
    drop(state);

    if changed {
        set_profile_status(status, tr("Unsaved", "未保存"), "dim-label");
    }
    refresh_shortcut_editor(settings, button, shortcut_ui);
    refresh_keyd_preview(settings, preview_ui);
}

fn update_button_mapping(
    settings: &SharedProfiles,
    status: &gtk::Label,
    preview_ui: &KeydPreviewUi,
    button: RemoteButton,
    action: ButtonAction,
) {
    let mut state = settings.borrow_mut();
    let Some(profile) = state
        .config
        .as_mut()
        .and_then(LinuxConfig::selected_profile_mut)
    else {
        return;
    };
    if profile.button_action(button) == action {
        return;
    }
    profile.button_mappings.insert(button, action);
    state.dirty = true;
    drop(state);
    set_profile_status(status, tr("Unsaved", "未保存"), "dim-label");
    refresh_keyd_preview(settings, preview_ui);
}

fn refresh_keyd_preview(settings: &SharedProfiles, preview_ui: &KeydPreviewUi) {
    let settings = settings.borrow();
    let Some(profile) = settings
        .config
        .as_ref()
        .and_then(LinuxConfig::selected_profile)
    else {
        preview_ui
            .buffer
            .set_text("# Preview unavailable: no selected device Profile\n");
        preview_ui.summary.set_subtitle(tr(
            "There are currently no device profiles to generate",
            "当前没有可生成的设备 Profile",
        ));
        set_profile_status(
            &preview_ui.status,
            tr("Unable to generate", "无法生成"),
            "error",
        );
        preview_ui.apply_button.set_sensitive(false);
        return;
    };

    match render_rc003_keyd_preview(profile) {
        Ok(preview) => {
            preview_ui.buffer.set_text(&preview.config);
            if preview.ready_to_apply() {
                preview_ui
                    .summary
                    .set_subtitle(tr("All verified key bits can be represented by keyd; user confirmation is still required to apply", "所有已验证键位都能由 keyd 表达；仍需用户确认后才能应用"));
                set_profile_status(
                    &preview_ui.status,
                    tr("Ready for confirmation", "可供确认"),
                    "success",
                );
                preview_ui.apply_button.set_sensitive(true);
            } else {
                preview_ui.summary.set_subtitle(&preview.notices.join("；"));
                set_profile_status(
                    &preview_ui.status,
                    &trf!(
                        "Not applicable ({} items)",
                        "不可应用（{} 项）",
                        preview.notices.len()
                    ),
                    "error",
                );
                preview_ui.apply_button.set_sensitive(false);
            }
        }
        Err(error) => {
            preview_ui
                .buffer
                .set_text(&format!("# Preview unavailable: {error}\n"));
            preview_ui.summary.set_subtitle(&error.to_string());
            set_profile_status(
                &preview_ui.status,
                tr("Unable to generate", "无法生成"),
                "error",
            );
            preview_ui.apply_button.set_sensitive(false);
        }
    }
}

fn start_keyd_apply(settings: &SharedProfiles, status: &gtk::Label, apply_button: &gtk::Button) {
    let preview = {
        let settings = settings.borrow();
        settings
            .config
            .as_ref()
            .and_then(LinuxConfig::selected_profile)
            .ok_or_else(|| {
                tr(
                    "There are currently no device profiles to apply",
                    "当前没有可应用的设备 Profile",
                )
                .to_string()
            })
            .and_then(|profile| {
                render_rc003_keyd_preview(profile).map_err(|error| error.to_string())
            })
    };
    let preview = match preview {
        Ok(preview) if preview.ready_to_apply() => preview,
        Ok(preview) => {
            set_profile_status(
                status,
                &trf!(
                    "Unable to apply: {}",
                    "无法应用：{}",
                    preview.notices.join("；")
                ),
                "error",
            );
            return;
        }
        Err(error) => {
            set_profile_status(
                status,
                &trf!("Could not apply: {error}", "无法应用：{error}"),
                "error",
            );
            return;
        }
    };

    if let Err(error) = persist_profile(settings) {
        set_profile_status(
            status,
            &trf!("Save failed: {error}", "保存失败：{error}"),
            "error",
        );
        return;
    }
    if !Path::new(KEYD_HELPER_PATH).is_file() {
        set_profile_status(
            status,
            tr(
                "The system application component is not installed; please run Linux/install-keyd-helper.sh first",
                "系统应用组件尚未安装；请先运行 Linux/install-keyd-helper.sh",
            ),
            "error",
        );
        return;
    }

    apply_button.set_sensitive(false);
    set_profile_status(
        status,
        tr("Waiting for admin authorization...", "等待管理员授权…"),
        "dim-label",
    );
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(run_keyd_helper(&preview.config));
    });

    glib::timeout_add_local(Duration::from_millis(100), {
        let settings = settings.clone();
        let status = status.clone();
        let apply_button = apply_button.clone();
        move || match receiver.try_recv() {
            Ok(Ok(message)) => {
                set_profile_status(&status, &message, "success");
                apply_button.set_sensitive(keyd_preview_ready(&settings));
                ControlFlow::Break
            }
            Ok(Err(error)) => {
                set_profile_status(&status, &error, "error");
                apply_button.set_sensitive(keyd_preview_ready(&settings));
                ControlFlow::Break
            }
            Err(TryRecvError::Empty) => ControlFlow::Continue,
            Err(TryRecvError::Disconnected) => {
                set_profile_status(
                    &status,
                    tr(
                        "System application process ended unexpectedly",
                        "系统应用进程意外结束",
                    ),
                    "error",
                );
                apply_button.set_sensitive(keyd_preview_ready(&settings));
                ControlFlow::Break
            }
        }
    });
}

fn keyd_preview_ready(settings: &SharedProfiles) -> bool {
    settings
        .borrow()
        .config
        .as_ref()
        .and_then(LinuxConfig::selected_profile)
        .and_then(|profile| render_rc003_keyd_preview(profile).ok())
        .is_some_and(|preview| preview.ready_to_apply())
}

fn run_keyd_helper(config: &str) -> Result<String, String> {
    let mut child = Command::new(PKEXEC_PATH)
        .arg(KEYD_HELPER_PATH)
        .arg(KEYD_HELPER_PROTOCOL)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            trf!(
                "Could not start system authorization: {error}",
                "无法启动系统授权：{error}"
            )
        })?;
    let write_result = child
        .stdin
        .take()
        .ok_or_else(|| {
            tr(
                "Unable to pass configuration to system application components",
                "无法向系统应用组件传递配置",
            )
            .to_string()
        })
        .and_then(|mut input| {
            input.write_all(config.as_bytes()).map_err(|error| {
                trf!(
                    "Failed to pass keyd configuration: {error}",
                    "传递 keyd 配置失败：{error}"
                )
            })
        });
    let output = child.wait_with_output().map_err(|error| {
        trf!(
            "Failed to wait for system to apply result: {error}",
            "等待系统应用结果失败：{error}"
        )
    })?;
    write_result?;

    if output.status.success() {
        let message = bounded_process_message(&output.stdout);
        Ok(if message.is_empty() {
            tr("Applied to system", "已应用到系统").into()
        } else {
            message
        })
    } else if output.status.code() == Some(126) {
        Err(tr(
            "Administrator authorization canceled; system configuration unchanged",
            "已取消管理员授权；系统配置未改变",
        )
        .into())
    } else {
        let message = bounded_process_message(if output.stderr.is_empty() {
            &output.stdout
        } else {
            &output.stderr
        });
        Err(if message.is_empty() {
            trf!(
                "System application failed: {}",
                "系统应用失败：{}",
                output.status
            )
        } else {
            message
        })
    }
}

fn bounded_process_message(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .trim()
        .chars()
        .take(512)
        .collect()
}

fn matcher_summary(profile: &DeviceProfile) -> String {
    let vendor = profile
        .matcher
        .vendor_id
        .map(|value| format!("VID {value:04X}"))
        .unwrap_or_else(|| tr("Vid not specified", "VID 未指定").into());
    let product = profile
        .matcher
        .product_id
        .map(|value| format!("PID {value:04X}"))
        .unwrap_or_else(|| tr("PID not specified", "PID 未指定").into());
    let name = profile
        .matcher
        .device_name
        .as_deref()
        .unwrap_or(tr("Name not specified", "名称未指定"));
    format!("{vendor} · {product} · {name}")
}

fn add_choice_buttons<T: Copy + Eq + 'static>(
    row: &adw::ActionRow,
    choices: &[(&str, T)],
    selected: T,
    on_selected: impl Fn(T) + 'static,
) {
    let buttons = gtk::Box::new(Orientation::Horizontal, 0);
    buttons.add_css_class("linked");
    buttons.set_valign(Align::Center);
    let callback = Rc::new(on_selected);
    let mut first: Option<gtk::ToggleButton> = None;
    for (label, value) in choices.iter().copied() {
        let button = gtk::ToggleButton::with_label(label);
        if let Some(first) = first.as_ref() {
            button.set_group(Some(first));
        } else {
            first = Some(button.clone());
        }
        button.set_active(value == selected);
        button.connect_toggled({
            let callback = callback.clone();
            move |button| {
                if button.is_active() {
                    callback(value);
                }
            }
        });
        buttons.append(&button);
    }
    row.add_suffix(&buttons);
}

fn update_selected_profile(
    settings: &SharedProfiles,
    status: &gtk::Label,
    update: impl FnOnce(&mut DeviceProfile),
) {
    let mut settings = settings.borrow_mut();
    let Some(profile) = settings
        .config
        .as_mut()
        .and_then(LinuxConfig::selected_profile_mut)
    else {
        return;
    };
    update(profile);
    settings.dirty = true;
    set_profile_status(
        status,
        tr("There are unsaved changes", "有未保存更改"),
        "dim-label",
    );
}

fn save_profile(settings: &SharedProfiles, status: &gtk::Label) {
    save_profile_with_message(settings, status, tr("Saved", "已保存"));
}

fn save_profile_with_message(
    settings: &SharedProfiles,
    status: &gtk::Label,
    success_message: &str,
) {
    match persist_profile(settings) {
        Ok(()) => set_profile_status(status, success_message, "success"),
        Err(error) => set_profile_status(
            status,
            &trf!("Save failed: {error}", "保存失败：{error}"),
            "error",
        ),
    }
}

fn persist_profile(settings: &SharedProfiles) -> Result<(), String> {
    let (store, config) = {
        let settings = settings.borrow();
        (settings.store.clone(), settings.config.clone())
    };
    let (store, config) = match (store, config) {
        (Some(store), Some(config)) => (store, config),
        _ => return Err(tr("Local configuration not available", "本机配置不可用").into()),
    };
    store.save(&config).map_err(|error| error.to_string())?;

    let mut settings = settings.borrow_mut();
    settings.persisted = true;
    settings.dirty = false;
    settings.error = None;
    Ok(())
}

fn set_profile_status(label: &gtk::Label, text: &str, css_class: &str) {
    for class in ["dim-label", "success", "error"] {
        label.remove_css_class(class);
    }
    label.set_text(text);
    label.add_css_class(css_class);
}

fn build_remote_group() -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title(tr("Xiaomi Bluetooth Voice Remote Control", "小米蓝牙语音遥控器"))
        .description(tr("RC003 Physical diagram; the device identity is based on the real-time information of BlueZ, and the Bluetooth address or temporary eventN is not saved.", "RC003 实物图；设备身份以 BlueZ 实时信息为准，不保存蓝牙地址或临时 eventN。"))
        .build();

    let bytes = glib::Bytes::from_static(REMOTE_IMAGE);
    match gtk::gdk::Texture::from_bytes(&bytes) {
        Ok(texture) => {
            let picture = gtk::Picture::builder()
                .paintable(&texture)
                .alternative_text(tr(
                    "Xiaomi RC003 Bluetooth Voice Remote Front",
                    "小米 RC003 蓝牙语音遥控器正面",
                ))
                .content_fit(gtk::ContentFit::Contain)
                .can_shrink(true)
                .halign(Align::Center)
                .height_request(340)
                .build();
            group.add(&picture);
        }
        Err(error) => {
            let row = adw::ActionRow::builder()
                .title(tr("Remote control image not available", "遥控器图片不可用"))
                .subtitle(error.to_string())
                .build();
            group.add(&row);
        }
    }

    group
}

fn build_summary_group(report: &DoctorReport) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title(tr("Phase 0B Readiness", "阶段 0B 就绪状态"))
        .description(tr(
            "The same test results are used here with omavoice-doctor --phase 0b.",
            "这里和 omavoice-doctor --phase 0b 使用同一份检测结果。",
        ))
        .build();

    let title = if report.ready {
        tr(
            "Setup hub base conditions are ready",
            "设置中心基础条件已就绪",
        )
    } else {
        tr(
            "There are still blocking conditions in the setting center",
            "设置中心仍有阻塞条件",
        )
    };
    let subtitle = trf!(
        "Missing {} item (s) via {} item (s), where {} item (s) are currently blocked.",
        "通过 {} 项，缺失 {} 项，其中当前阻塞 {} 项。",
        report.summary.passed,
        report.summary.missing,
        report.summary.blocking
    );
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .build();
    let status = status_label(
        if report.ready {
            tr("Can continue", "可以继续")
        } else {
            tr("Needs processing", "需要处理")
        },
        report.ready,
        !report.ready,
    );
    row.add_suffix(&status);
    group.add(&row);

    group
}

fn build_check_row(check: &CheckResult) -> adw::ActionRow {
    let subtitle = match (check.status, check.remediation) {
        (CheckStatus::Missing, Some(remediation)) => {
            trf!(
                "{}\\ nRecommended: {remediation}",
                "{}\n建议：{remediation}",
                check.detail
            )
        }
        _ => check.detail.clone(),
    };
    let row = adw::ActionRow::builder()
        .title(check.label)
        .subtitle(subtitle)
        .subtitle_lines(3)
        .build();

    let passed = check.status == CheckStatus::Passed;
    let icon_name = if passed {
        "emblem-ok-symbolic"
    } else {
        "dialog-warning-symbolic"
    };
    row.add_prefix(&gtk::Image::from_icon_name(icon_name));

    let status = match (check.status, check.blocking) {
        (CheckStatus::Passed, _) => status_label(tr("Already have", "已具备"), true, false),
        (CheckStatus::Missing, true) => status_label(tr("Current block", "当前阻塞"), false, true),
        (CheckStatus::Missing, false) => status_label(tr("Needed Later", "稍后需要"), false, false),
    };
    row.add_suffix(&status);
    row
}

fn status_label(text: &str, success: bool, error: bool) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_valign(Align::Center);
    if success {
        label.add_css_class("success");
    } else if error {
        label.add_css_class("error");
    } else {
        label.add_css_class("dim-label");
    }
    label
}

fn checks_for_page(report: &DoctorReport, definition: PageDefinition) -> Vec<&CheckResult> {
    if definition.id == "system" {
        return report.checks.iter().collect();
    }

    definition
        .check_ids
        .iter()
        .filter_map(|id| report.checks.iter().find(|check| check.id == *id))
        .collect()
}

fn page_group_title(page_id: &str) -> &'static str {
    match page_id {
        "overview" => tr("Local base environment", "本机基础环境"),
        "devices" => tr("Remote Controls &amp; Input Devices", "遥控器与输入设备"),
        "voice" => tr("Global Voice Input", "全局语音输入"),
        "buttons" => tr("Key Mapping Criteria", "按键映射条件"),
        "system" => tr("Full Ability Check", "全部能力检查"),
        _ => tr("Status", "状态"),
    }
}

fn page_group_description(page_id: &str) -> &'static str {
    match page_id {
        "overview" => tr(
            "These components determine whether the setup center will function properly in the current Wayland session.",
            "这些组件决定设置中心能否在当前 Wayland 会话正常运行。",
        ),
        "devices" => tr(
            "Show only real-time capabilities and don't save Bluetooth addresses, temporary eventN, or connection status.",
            "只展示实时能力，不保存蓝牙地址、临时 eventN 或连接状态。",
        ),
        "voice" => tr(
            "ATVVoice provides the sound source of the remote control, and Handy is responsible for dictation, text writing, and bottom suspension interaction.",
            "ATVVoice 提供遥控器音源，Handy 负责听写、文字写入与底部悬浮交互。",
        ),
        "buttons" => tr(
            "The first version prefers to reuse keyd; actions that cannot be expressed by off-the-shelf tools are only added by OmaVoice.",
            "首版优先复用 keyd；现成工具不能表达的动作才由 OmaVoice 增加薄适配。",
        ),
        "system" => tr(
            "Missing components required for later stages will not incorrectly block the current stage.",
            "缺少以后阶段才需要的组件不会错误阻塞当前阶段。",
        ),
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omavoice_linux::SystemSnapshot;
    use std::collections::HashSet;

    #[test]
    fn settings_navigation_has_stable_unique_pages() {
        assert_eq!(
            PAGES.map(|page| page.id),
            [
                "overview",
                "devices",
                "voice",
                "buttons",
                "statistics",
                "system"
            ]
        );
        assert_eq!(
            PAGES
                .iter()
                .map(|page| page.id)
                .collect::<HashSet<_>>()
                .len(),
            PAGES.len()
        );
        assert!(PAGES.iter().all(|page| !page.title.is_empty()));
        assert!(PAGES.iter().all(|page| !page.icon.is_empty()));
        let statistics = PAGES.iter().find(|page| page.id == "statistics").unwrap();
        assert!(
            statistics
                .description
                .contains(tr("off by default", "默认关闭"))
        );
        assert!(
            statistics
                .description
                .contains(tr("explicitly turned on or imported", "明确开启或导入"))
        );
    }

    #[test]
    fn button_mapping_choices_cover_every_semantic_button_and_action_once() {
        let buttons = REMOTE_BUTTON_ROWS
            .iter()
            .flat_map(|(_, buttons)| buttons.iter().map(|(button, _)| *button))
            .collect::<Vec<_>>();
        assert_eq!(buttons.len(), omavoice_linux::config::REMOTE_BUTTONS.len());
        for expected in omavoice_linux::config::REMOTE_BUTTONS {
            assert_eq!(
                buttons
                    .iter()
                    .filter(|candidate| **candidate == expected)
                    .count(),
                1
            );
        }

        let actions = BUTTON_ACTION_ROWS
            .iter()
            .flat_map(|(_, actions)| actions.iter().map(|(action, _)| *action))
            .collect::<Vec<_>>();
        let expected_actions = [
            ButtonAction::Disabled,
            ButtonAction::PassThrough,
            ButtonAction::Escape,
            ButtonAction::Enter,
            ButtonAction::Backspace,
            ButtonAction::ArrowUp,
            ButtonAction::ArrowDown,
            ButtonAction::ArrowLeft,
            ButtonAction::ArrowRight,
            ButtonAction::ShowDesktop,
            ButtonAction::ContextMenu,
            ButtonAction::AppSwitcher,
            ButtonAction::VolumeUp,
            ButtonAction::VolumeDown,
            ButtonAction::VolumeMute,
            ButtonAction::PlayPause,
            ButtonAction::CustomShortcut,
        ];
        assert_eq!(actions.len(), expected_actions.len());
        for expected in expected_actions {
            assert_eq!(
                actions
                    .iter()
                    .filter(|candidate| **candidate == expected)
                    .count(),
                1
            );
        }
    }

    #[test]
    fn tray_exposes_remote_icon_and_expected_menu() {
        let (commands, _) = mpsc::channel();
        let tray = OmaVoiceTray { commands };
        let icons = ksni::Tray::icon_pixmap(&tray);
        let menu = ksni::Tray::menu(&tray);
        let labels = menu
            .iter()
            .filter_map(|item| match item {
                ksni::MenuItem::Standard(item) => Some(item.label.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(ksni::Tray::icon_name(&tray), "");
        assert_eq!(icons.len(), 1);
        assert_eq!((icons[0].width, icons[0].height), (64, 64));
        assert_eq!(icons[0].data.len(), 64 * 64 * 4);
        assert_eq!(
            labels,
            [
                tr("Open Settings", "打开设置"),
                tr("Omarchy Linux Preview", "Omarchy Linux 预览"),
                tr("Retest", "重新检测"),
                tr("Quit OmaVoice", "退出 OmaVoice")
            ]
        );
    }

    #[test]
    fn tray_activation_requests_the_settings_window() {
        let (commands, receiver) = mpsc::channel();
        let mut tray = OmaVoiceTray { commands };

        ksni::Tray::activate(&mut tray, 0, 0);

        assert_eq!(receiver.try_recv(), Ok(TrayCommand::OpenSettings));
    }

    #[test]
    fn embedded_remote_image_is_the_rc003_asset() {
        assert_eq!(&REMOTE_IMAGE[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(
            u32::from_be_bytes(REMOTE_IMAGE[16..20].try_into().unwrap()),
            1024
        );
        assert_eq!(
            u32::from_be_bytes(REMOTE_IMAGE[20..24].try_into().unwrap()),
            1536
        );
    }

    #[test]
    fn settings_css_keeps_chinese_text_at_twelve_points() {
        assert!(SETTINGS_CSS.contains("font-size: 12pt"));
    }

    #[test]
    fn statistics_values_have_stable_user_facing_labels() {
        assert_eq!(statistics_key_label("up"), tr("Up", "上"));
        assert_eq!(statistics_key_label("volumeup"), tr("Volume +", "音量+"));
        assert_eq!(statistics_key_label("p"), "P");
        assert_eq!(statistics_key_label("f12"), "F12");
        assert_eq!(statistics_key_label("play_pause"), "play pause");

        assert_eq!(format_statistics_duration(0), tr("0.0 sec", "0.0 秒"));
        assert_eq!(format_statistics_duration(5_520), tr("5.5 sec", "5.5 秒"));
        assert_eq!(
            format_statistics_duration(65_000),
            tr("1 min 5 sec", "1 分 5 秒")
        );
        assert_eq!(
            format_statistics_duration(3_665_000),
            tr("1 hr 1 min 5 sec", "1 小时 1 分 5 秒")
        );
        assert_eq!(
            format_statistics_timestamp(i64::MAX),
            tr("Unknown time", "时间未知")
        );
    }

    #[test]
    fn component_launchers_do_not_use_a_shell_or_mutate_system_configuration() {
        assert_eq!(OMARCHY_BLUETOOTH.program, "quickshell");
        assert_eq!(
            OMARCHY_BLUETOOTH.arguments,
            [
                "ipc",
                "-p",
                OMARCHY_SHELL_PATH,
                "call",
                "omarchy.bluetooth",
                "open"
            ]
        );
        assert!(
            [BLUEMAN, OVERSKRIDE, GNOME_BLUETOOTH, KDE_BLUETOOTH]
                .iter()
                .all(|launcher| launcher.program != "sh" && launcher.program != "sudo")
        );
        assert!(OMAVOICE_HANDY.arguments.is_empty());
        assert!(HANDY_LOWERCASE.arguments.is_empty());
        assert_eq!(PKEXEC_PATH, "/usr/bin/pkexec");
        assert_eq!(KEYD_HELPER_PATH, "/usr/lib/omavoice/omavoice-keyd-helper");
        assert_eq!(KEYD_HELPER_PROTOCOL, "apply-v1");
        assert_ne!(KEYD_HELPER_PATH, "sh");
    }

    #[test]
    fn privileged_helper_output_is_bounded_before_display() {
        assert_eq!(bounded_process_message(b" Done\n"), "Done");
        assert_eq!(bounded_process_message(&vec![b'x'; 600]).len(), 512);
    }

    #[test]
    fn focused_pages_cover_every_doctor_check_once() {
        let report = evaluate(&SystemSnapshot::default(), Phase::ZeroB);
        let focused_pages = PAGES.iter().filter(|page| page.id != "system");
        let ids = focused_pages
            .flat_map(|page| page.check_ids.iter().copied())
            .collect::<Vec<_>>();

        assert_eq!(ids.len(), report.checks.len());
        assert_eq!(ids.iter().copied().collect::<HashSet<_>>().len(), ids.len());
        assert!(report.checks.iter().all(|check| ids.contains(&check.id)));
    }

    #[test]
    fn system_page_exposes_the_complete_report() {
        let report = evaluate(&SystemSnapshot::default(), Phase::ZeroB);
        let system = PAGES
            .iter()
            .find(|page| page.id == "system")
            .copied()
            .unwrap();

        assert_eq!(checks_for_page(&report, system).len(), report.checks.len());
    }
}
