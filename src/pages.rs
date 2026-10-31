//! Strony centrum ustawień: model wierszy, akcje i odczyt danych systemowych.
//!
//! Rysowanie nie zna typów kontrolek — [`ui`](crate::ui) dostaje gotowe grupy i
//! wiersze, a kliknięcia rozwiązuje przez [`Action`]. Dzięki temu strony można
//! testować bez okna.

use std::path::{Path, PathBuf};

use libluna_core::{
    Accent, GreeterSettings, IconTheme, LockWallpaper, OutputMode, ThemePreference,
};
use libluna_i18n::{tr, tr_args};
use libluna_widgets::IconName;
use luna::equalizer::{Preset, BAND_LABELS, MAX_GAIN_DB};
use luna::network::Security;
use luna::notifications::Placement;

use crate::app::SettingsApp;

/// Strona centrum ustawień.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Appearance,
    Login,
    LockScreen,
    Notifications,
    Language,
    Workspaces,
    Users,
    Accessibility,
    DefaultApps,
    Network,
    Bluetooth,
    Input,
    Shortcuts,
    Displays,
    Sound,
    Power,
    Printers,
    System,
    About,
}

impl Page {
    pub const ALL: [Page; 19] = [
        Page::Appearance,
        Page::Login,
        Page::LockScreen,
        Page::Notifications,
        Page::Language,
        Page::Workspaces,
        Page::Users,
        Page::Accessibility,
        Page::DefaultApps,
        Page::Network,
        Page::Bluetooth,
        Page::Input,
        Page::Shortcuts,
        Page::Displays,
        Page::Sound,
        Page::Power,
        Page::Printers,
        Page::System,
        Page::About,
    ];

    /// Nazwa strony dla wiersza poleceń (`luna-settings --page network`).
    pub fn slug(self) -> &'static str {
        match self {
            Page::Appearance => "appearance",
            Page::Login => "login",
            Page::LockScreen => "lock-screen",
            Page::Notifications => "notifications",
            Page::Language => "language",
            Page::Workspaces => "workspaces",
            Page::Users => "users",
            Page::Accessibility => "accessibility",
            Page::DefaultApps => "default-apps",
            Page::Network => "network",
            Page::Bluetooth => "bluetooth",
            Page::Input => "input",
            Page::Shortcuts => "shortcuts",
            Page::Displays => "displays",
            Page::Sound => "sound",
            Page::Power => "power",
            Page::Printers => "printers",
            Page::System => "system",
            Page::About => "about",
        }
    }

    /// Znajduje stronę po nazwie (np. z argumentu `--page`).
    pub fn by_name(name: &str) -> Option<Self> {
        let name = name.trim().to_lowercase();
        Self::ALL.into_iter().find(|page| page.slug() == name)
    }

    /// Klucz tłumaczenia tytułu.
    pub fn title_key(self) -> &'static str {
        match self {
            Page::Appearance => "Appearance and Icons",
            Page::Login => "Login Screen",
            Page::LockScreen => "Lock Screen",
            Page::Notifications => "Notifications",
            Page::Language => "Language and Region",
            Page::Workspaces => "Workspaces",
            Page::Users => "Users",
            Page::Accessibility => "Accessibility",
            Page::DefaultApps => "Default Applications",
            Page::Network => "Network",
            Page::Bluetooth => "Bluetooth",
            Page::Input => "Keyboard and Mouse",
            Page::Shortcuts => "Keyboard Shortcuts",
            Page::Displays => "Displays",
            Page::Sound => "Sound",
            Page::Power => "Power",
            Page::Printers => "Printers",
            Page::System => "System",
            Page::About => "About Luna",
        }
    }

    /// Klucz tłumaczenia opisu strony.
    pub fn subtitle_key(self) -> &'static str {
        match self {
            Page::Appearance => "Theme, colors and icons in one place",
            Page::Login => "Wallpaper and colors of the sign-in screen",
            Page::LockScreen => "Look of your lock screen (no administrator needed)",
            Page::Notifications => "Control notifications and history",
            Page::Language => "Language, formats and time zone",
            Page::Workspaces => "Number, names and switching of workspaces",
            Page::Users => "Local accounts on this computer",
            Page::Accessibility => "Text size, contrast and motion",
            Page::DefaultApps => "Programs used to open files and links",
            Page::Network => "Wi-Fi, wired and VPN connections (NetworkManager)",
            Page::Bluetooth => "Adapter, scanning and pairing (BlueZ)",
            Page::Input => "Key repeat, scrolling and touchpad",
            Page::Shortcuts => "Desktop shortcuts with Super, Ctrl or Alt",
            Page::Displays => "Displays, resolution and color profiles",
            Page::Sound => "Volume, devices and equalizer",
            Page::Power => "Battery, power profiles and sleep",
            Page::Printers => "Printers and print queues (CUPS)",
            Page::System => "System and environment information",
            Page::About => "About the Luna desktop",
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            Page::Appearance => IconName::Brightness,
            Page::Login => IconName::User,
            Page::LockScreen => IconName::Star,
            Page::Notifications => IconName::Info,
            Page::Language => IconName::Globe,
            Page::Workspaces => IconName::Grid,
            Page::Users => IconName::User,
            Page::Accessibility => IconName::User,
            Page::DefaultApps => IconName::App,
            Page::Network => IconName::Wifi,
            Page::Bluetooth => IconName::Bluetooth,
            Page::Input => IconName::Sliders,
            Page::Shortcuts => IconName::Edit,
            Page::Displays => IconName::Monitor,
            Page::Sound => IconName::Volume,
            Page::Power => IconName::Battery,
            Page::Printers => IconName::Printer,
            Page::System => IconName::Info,
            Page::About => IconName::Moon,
        }
    }
}

/// Akcja wywoływana przez kontrolkę.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    SetPreference(ThemePreference),
    SetAccent(Accent),
    SetStyle(usize),
    SetLanguage(usize),
    /// Otwiera wyszukiwany wybór (język, locale, strefa, ikony).
    OpenPicker(PickerKind),
    SetIconTheme(IconTheme),
    /// Zewnętrzny zestaw ikon freedesktop (`None` = tylko wektorowe).
    SetExternalIconSet(Option<String>),
    SetVolume(u8),
    ToggleMute,
    /// Wybiera domyślne wyjście audio (`wpctl set-default`).
    SetSink(u32),
    /// Wybiera domyślne wejście audio.
    SetSource(u32),
    // — Bezczynność: wygaszanie, blokada, usypianie —
    /// Ekran gaśnie po `n` sekundach bezczynności (0 = nigdy).
    SetIdleScreenOff(u32),
    /// Sesja blokuje się po `n` sekundach bezczynności (0 = nigdy).
    SetIdleLock(u32),
    /// Komputer usypia po `n` sekundach bezczynności (0 = nigdy).
    SetIdleSuspend(u32),
    /// Czy wygaszenie ekranu od razu blokuje sesję.
    ToggleLockOnBlank,
    // — Konta —
    /// Otwiera formularz dodawania konta.
    OpenAddAccount,
    /// Otwiera formularz zmiany hasła wskazanego konta.
    OpenPasswordChange(String),
    // — Korektor (PipeWire + LADSPA) —
    ToggleEqualizer,
    SetEqPreset(Preset),
    SetEqPreamp(i8),
    SetEqBand(usize, i8),
    SetBrightness(u8),
    TogglePowerSaver,
    ToggleDnd,
    /// Tapeta ekranu logowania (indeks w [`SettingsApp::wallpapers`]).
    SetGreeterWallpaper(usize),
    /// Ekran logowania bez tapety — gradient akcentu.
    ClearGreeterWallpaper,
    SetGreeterTheme(ThemePreference),
    SetGreeterAccent(Accent),
    ToggleGreeterClock,
    /// Zapisuje szkic ekranu logowania (formularz hasła administratora).
    ApplyGreeter,
    /// Użyj tapety pulpitu (jeśli jest ustawiona).
    UseDesktopWallpaper,
    // — Ekran blokady (per użytkownik, bez roota) —
    SetLockTheme(ThemePreference),
    SetLockAccent(Accent),
    ToggleLockClock,
    SetLockWallpaper(LockWallpaper),
    // — Powiadomienia / przestrzenie —
    SetNotificationPlacement(Placement),
    /// Wysyła testowe powiadomienie do daemona `luna-shell`.
    TestNotification,
    SetWorkspaceCount(usize),
    EditWorkspaceName(usize),
    /// Włącza/wyłącza przełączanie przestrzeni przez dolne rogi ekranu.
    ToggleWorkspaceCorner,
    /// Włącza/wyłącza podgląd (Super+Tab) w lewym górnym rogu.
    ToggleOverviewCorner,
    /// Otwiera wybór tapety ekranu logowania z dysku (własny piksel).
    OpenWallpaperPicker,
    /// Otwiera wybór tapety **pulpitu** z dysku.
    OpenDesktopWallpaperPicker,
    /// Ustawia tapetę pulpitu z konkretnej ścieżki.
    SetDesktopWallpaperPath(PathBuf),
    /// Przywraca wbudowaną tapetę pulpitu.
    ClearDesktopWallpaper,
    /// Ustawia tapetę ekranu logowania z konkretnej ścieżki (wybór z dysku).
    SetGreeterWallpaperPath(PathBuf),
    /// Ustawia strefę systemową przez `pkexec timedatectl` (wymaga autoryzacji).
    ApplySystemTimezone,
    /// Włącza/wyłącza synchronizację czasu (NTP) w systemie.
    ToggleNtp,
    // — Sieć (NetworkManager przez D-Bus) —
    ToggleWifi,
    RefreshWifi,
    /// Połącz z siecią Wi-Fi na wskazanym urządzeniu.
    ConnectWifi {
        device: String,
        ssid: String,
        security: Security,
        hidden: bool,
    },
    ConnectEthernet(String),
    DisconnectDevice(String),
    ForgetWifi(String),
    /// Usuwa dowolny zapisany profil (także sieć poza zasięgiem).
    ForgetProfile(String),
    /// Włącza/wyłącza automatyczne łączenie z zapisanym profilem.
    SetAutoconnect {
        profile: String,
        enabled: bool,
    },
    /// Otwiera formularz IPv4 dla urządzenia przewodowego.
    OpenWiredEditor(String),
    /// Otwiera formularz nazwy sieci ukrytej.
    OpenHiddenNetwork,
    // — Ekrany —
    /// Wybiera monitor, którego dotyczą ustawienia poniżej.
    SelectMonitor(usize),
    /// Ustawia siłę ciepłej korekty (night light) 0–100.
    SetNightLight(u8),
    /// Układa monitory poziomo i zapisuje pozycje.
    AutoArrangeMonitors,
    // — Wejście (klawiatura i mysz) —
    SetRepeatDelay(u32),
    SetRepeatRate(u32),
    ToggleNaturalScroll,
    // — Bluetooth (BlueZ przez D-Bus) —
    SetBluetoothPowered {
        adapter: String,
        powered: bool,
    },
    SetBluetoothDiscovery {
        adapter: String,
        on: bool,
    },
    BluetoothPair(String),
    BluetoothConnect(String),
    BluetoothDisconnect(String),
    BluetoothRemove {
        adapter: String,
        device: String,
    },
    RefreshBluetooth,
    // — Profile zasilania (power-profiles-daemon) —
    SetPowerProfile(String),
    // — VPN (NetworkManager) —
    VpnConnect(String),
    VpnDisconnect(String),
    VpnRemove(String),
    OpenVpnImport,
    // — Drukarki (CUPS) —
    PrinterDefault(String),
    PrinterEnabled {
        name: String,
        enabled: bool,
    },
    PrinterRemove(String),
    OpenPrinterAdd,
    // — Sesje (logind) —
    SwitchSession(String),
    // — Kalibracja kolorów (colord) —
    OpenColorPicker(String),
    OpenColorImport,
    SetColorProfile(String),
    // — Dostępność —
    SetTextScale(u8),
    ToggleHighContrast,
    ToggleReduceMotion,
    // — Domyślne aplikacje (xdg-mime) —
    OpenDefaultAppPicker(usize),
    SetDefaultApp(String),
    // — Skróty klawiszowe —
    RecordShortcut(libluna_core::ShortcutAction),
    ResetShortcuts,
    /// Klik w numer wersji na stronie „O Lunie” (easter egg).
    AboutVersionClick,
}

/// Rodzaj wyszukiwanego wyboru.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    Language,
    Locale,
    Timezone,
    IconSet,
    /// Rozdzielczość wybranego monitora.
    Resolution,
    /// Odświeżanie dla wybranej rozdzielczości.
    Refresh,
    /// Profil kolorów dla wybranego urządzenia colord (`SettingsApp::color_target`).
    ColorProfile,
    /// Domyślna aplikacja dla kategorii (`SettingsApp::default_app_target`).
    DefaultApp,
}

/// Do czego należy wybrany akcent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccentTarget {
    /// Akcent całego środowiska (zapis w `theme.conf` użytkownika).
    Desktop,
    /// Akcent ekranu logowania (`/etc/luna/greeter.conf`).
    Greeter,
    /// Akcent ekranu blokady (`~/.config/luna/lock.conf`).
    Lock,
}

/// Który suwak przeciągamy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliderKind {
    Volume,
    Brightness,
    NightLight,
    /// Przedwzmacniacz korektora (0–100 % → −12…+12 dB).
    EqPreamp,
    /// Pasmo korektora o wskazanym indeksie.
    EqBand(usize),
}

impl SliderKind {
    /// Suwak 0–100 % → wzmocnienie w dB (zakres ±[`MAX_GAIN_DB`]).
    pub fn eq_db(percent: u8) -> i8 {
        let db = (percent as f32 / 100.0) * 2.0 * MAX_GAIN_DB - MAX_GAIN_DB;
        db.round() as i8
    }

    /// dB → pozycja suwaka 0–1.
    pub fn eq_position(db: f32) -> f64 {
        ((db + MAX_GAIN_DB) / (2.0 * MAX_GAIN_DB)).clamp(0.0, 1.0) as f64
    }
}

/// Kontrolka w wierszu ustawień.
#[derive(Debug, Clone)]
pub enum Control {
    /// Tylko tytuł/podtytuł (wiersz informacyjny).
    None,
    /// Wartość po prawej stronie.
    Value(String),
    Switch(bool),
    Slider {
        kind: SliderKind,
        value: f64,
    },
    /// Segmentowy przełącznik; `actions[i]` odpowiada opcji `i`.
    Segmented {
        options: Vec<String>,
        selected: usize,
        actions: Vec<Action>,
    },
    /// Paleta akcentów (indeks w [`Accent::ALL`]).
    Accents {
        selected: usize,
        target: AccentTarget,
    },
    /// Pojedynczy wybór („ptaszek”) — np. wybór tapety ekranu logowania.
    Check(bool),
    /// Pole wyboru otwierające wyszukiwany picker (splitbutton z chevronem).
    Select {
        value: String,
    },
    /// Pasek podglądu ikon (styl ikon).
    IconStrip(Vec<IconName>),
    /// Podgląd motywu: karta w barwach akcentu z kilkoma ikonami.
    ThemePreview {
        accent: Accent,
        icons: Vec<IconName>,
    },
    /// Miniatura tapety z zaznaczeniem.
    Wallpaper {
        path: PathBuf,
        selected: bool,
    },
    Button {
        label: String,
        suggested: bool,
    },
    /// Kilka przycisków w jednym wierszu (np. akcje urządzenia).
    Buttons {
        labels: Vec<String>,
        actions: Vec<Action>,
        /// Indeks przycisku sugerowanego (`usize::MAX` = żaden).
        suggested: usize,
    },
}

/// Monitor na grafice układu (współrzędne logiczne z Selene).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorTile {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub enabled: bool,
    pub selected: bool,
}

/// Wiersz ustawień.
#[derive(Debug, Clone)]
pub struct Row {
    pub title: String,
    pub subtitle: Option<String>,
    pub control: Control,
    pub action: Action,
}

/// Grupa ustawień.
#[derive(Debug, Clone)]
pub struct Group {
    pub title: String,
    pub rows: Vec<Row>,
    /// Grupa techniczna („Configuration”/„Information”) — pokazywana w panelu Info,
    /// a nie w głównej treści strony.
    pub info: bool,
}

fn row(title: impl Into<String>, control: Control, action: Action) -> Row {
    Row {
        title: title.into(),
        subtitle: None,
        control,
        action,
    }
}

fn row_sub(
    title: impl Into<String>,
    subtitle: impl Into<String>,
    control: Control,
    action: Action,
) -> Row {
    Row {
        title: title.into(),
        subtitle: Some(subtitle.into()),
        control,
        action,
    }
}

fn value_row(title: impl Into<String>, value: impl Into<String>) -> Row {
    row(title, Control::Value(value.into()), Action::None)
}

fn info_row(title: impl Into<String>, subtitle: impl Into<String>) -> Row {
    row_sub(title, subtitle, Control::None, Action::None)
}

fn group(title: impl Into<String>, rows: Vec<Row>) -> Group {
    Group {
        title: title.into(),
        rows,
        info: false,
    }
}

fn info_group(title: impl Into<String>, rows: Vec<Row>) -> Group {
    Group {
        title: title.into(),
        rows,
        info: true,
    }
}

/// Wiersz z kilkoma przyciskami (akcje urządzenia, drukarki).
fn row_buttons(
    title: impl Into<String>,
    subtitle: impl Into<String>,
    labels: Vec<String>,
    actions: Vec<Action>,
) -> Row {
    row_sub(
        title,
        subtitle,
        Control::Buttons {
            labels,
            actions,
            suggested: usize::MAX,
        },
        Action::None,
    )
}

/// Buduje grupy i wiersze dla aktywnej strony.
pub fn groups(app: &SettingsApp) -> Vec<Group> {
    match app.page {
        Page::Appearance => appearance(app),
        Page::Notifications => notifications(app),
        Page::Language => language(app),
        Page::Login => login(app),
        Page::LockScreen => lock_screen(app),
        Page::Workspaces => workspaces(app),
        Page::Users => users(app),
        Page::Accessibility => accessibility(app),
        Page::DefaultApps => default_apps(app),
        Page::Network => network(app),
        Page::Bluetooth => bluetooth(app),
        Page::Input => input(app),
        Page::Shortcuts => shortcuts(app),
        Page::Displays => displays(app),
        Page::Sound => sound(app),
        Page::Power => power(app),
        Page::Printers => printers(app),
        Page::System => system(),
        Page::About => about(app),
    }
}

/// Grupy widoczne w głównej treści strony (bez technicznych szczegółów).
pub fn content_groups(app: &SettingsApp) -> Vec<Group> {
    groups(app)
        .into_iter()
        .filter(|group| !group.info)
        .collect()
}

/// Grupy techniczne dla panelu Info (konfiguracja, ścieżki, backendy).
pub fn info_groups(app: &SettingsApp) -> Vec<Group> {
    groups(app).into_iter().filter(|group| group.info).collect()
}

fn appearance(app: &SettingsApp) -> Vec<Group> {
    let mode = vec![tr("Light"), tr("Dark"), tr("Auto")];
    let mode_actions = vec![
        Action::SetPreference(ThemePreference::Light),
        Action::SetPreference(ThemePreference::Dark),
        Action::SetPreference(ThemePreference::System),
    ];
    let mode_selected = match app.theme_pref {
        ThemePreference::Light => 0,
        ThemePreference::Dark => 1,
        ThemePreference::System => 2,
    };

    let mut theme_rows = vec![
        row_sub(
            tr("Theme mode"),
            tr("System colors are used when Auto is selected"),
            Control::Segmented {
                options: mode,
                selected: mode_selected,
                actions: mode_actions,
            },
            Action::None,
        ),
        row(
            tr("Accent color"),
            Control::Accents {
                selected: Accent::ALL
                    .iter()
                    .position(|a| *a == app.accent)
                    .unwrap_or(0),
                target: AccentTarget::Desktop,
            },
            Action::None,
        ),
    ];

    if !app.styles.is_empty() {
        let mut options = vec![tr("Default")];
        let mut actions = vec![Action::SetStyle(0)];
        let mut selected = 0;
        for (i, style) in app.styles.iter().enumerate() {
            options.push(style.name.clone());
            actions.push(Action::SetStyle(i + 1));
            if app.style.as_deref() == Some(style.id.as_str()) {
                selected = i + 1;
            }
        }
        theme_rows.push(row(
            tr("Style"),
            Control::Segmented {
                options,
                selected,
                actions,
            },
            Action::None,
        ));
    }

    theme_rows.push(row_sub(
        tr("Preview"),
        tr("A live sample of the selected colors and icons"),
        Control::ThemePreview {
            accent: app.accent,
            icons: vec![
                IconName::Files,
                IconName::Terminal,
                IconName::Settings,
                IconName::Globe,
                IconName::Music,
                IconName::Image,
            ],
        },
        Action::None,
    ));

    let selected = IconTheme::ALL
        .iter()
        .position(|theme| *theme == app.icons.theme)
        .unwrap_or(0);
    let options = vec![tr("Luna (pastel)"), tr("Monochrome")];
    let actions = IconTheme::ALL
        .iter()
        .map(|theme| Action::SetIconTheme(*theme))
        .collect();
    let preview = vec![
        IconName::Files,
        IconName::Terminal,
        IconName::Settings,
        IconName::Globe,
        IconName::Music,
        IconName::Image,
        IconName::Wifi,
        IconName::Moon,
    ];
    let external = match &app.icons.external {
        Some(name) => libluna_core::icons::find_icon_set(name)
            .map(|set| libluna_core::icons::icon_set_display_name(&set))
            .unwrap_or_else(|| name.clone()),
        None => tr("Vector icons (Luna)"),
    };
    let dirs = libluna_core::icons::icon_search_dirs()
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(" · ");

    let current_wallpaper = libluna_render::wallpaper::configured_path();
    let mut wallpaper_rows = vec![row_sub(
        tr("Desktop wallpaper"),
        current_wallpaper
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| tr("Built-in graphic")),
        Control::Button {
            label: tr("Browse…"),
            suggested: false,
        },
        Action::OpenDesktopWallpaperPicker,
    )];
    for path in app.wallpapers.iter().take(6) {
        wallpaper_rows.push(row_sub(
            wallpaper_label(path),
            path.parent()
                .map(|parent| parent.display().to_string())
                .unwrap_or_default(),
            Control::Wallpaper {
                path: path.clone(),
                selected: current_wallpaper.as_deref() == Some(path.as_path()),
            },
            Action::SetDesktopWallpaperPath(path.clone()),
        ));
    }
    if current_wallpaper.is_some() {
        wallpaper_rows.push(row(
            tr("Built-in wallpaper"),
            Control::Button {
                label: tr("Restore"),
                suggested: false,
            },
            Action::ClearDesktopWallpaper,
        ));
    }

    vec![
        group(tr("Theme"), theme_rows),
        group(tr("Wallpaper"), wallpaper_rows),
        group(
            tr("Icons"),
            vec![
                row_sub(
                    tr("Icon style"),
                    tr("Pastel illustrations or monochrome icons"),
                    Control::Segmented {
                        options,
                        selected,
                        actions,
                    },
                    Action::None,
                ),
                row_sub(
                    tr("External icon set"),
                    tr("Global for the whole environment; installed in the icon directories below"),
                    Control::Select { value: external },
                    Action::OpenPicker(PickerKind::IconSet),
                ),
                row(tr("Preview"), Control::IconStrip(preview), Action::None),
            ],
        ),
        info_group(
            tr("Information"),
            vec![
                info_row(
                    tr("Icons are vector-based"),
                    tr("The style changes colors; an external set replaces application and place icons"),
                ),
                value_row(tr("Installed icon sets"), app.icon_sets.len().to_string()),
                info_row(tr("Icon directories"), dirs),
            ],
        ),
    ]
}

fn notifications(app: &SettingsApp) -> Vec<Group> {
    let placements = vec![
        "↖".to_owned(),
        "↗".to_owned(),
        "↙".to_owned(),
        "↘".to_owned(),
    ];
    let placement_actions = Placement::ALL
        .iter()
        .map(|p| Action::SetNotificationPlacement(*p))
        .collect();
    let placement_selected = Placement::ALL
        .iter()
        .position(|p| *p == app.notification_placement)
        .unwrap_or(0);

    vec![
        group(
            tr("Notifications"),
            vec![
                row_sub(
                    tr("Notification position"),
                    format!(
                        "{}  ↖ ↗ ↙ ↘",
                        tr("Corner of the screen where toasts appear")
                    ),
                    Control::Segmented {
                        options: placements,
                        selected: placement_selected,
                        actions: placement_actions,
                    },
                    Action::None,
                ),
                row_sub(
                    tr("Do Not Disturb"),
                    tr("Hide toasts, keep them in the notification center"),
                    Control::Switch(app.dnd),
                    Action::ToggleDnd,
                ),
                info_row(
                    tr("History"),
                    tr("Click the clock on the panel to open the notification center"),
                ),
                row_sub(
                    tr("Test notification"),
                    tr("Sends a sample notification through the running daemon"),
                    Control::Button {
                        label: tr("Send test"),
                        suggested: false,
                    },
                    Action::TestNotification,
                ),
                value_row(
                    tr("Notification daemon"),
                    if app.notification_daemon.is_empty() {
                        tr("Checking…")
                    } else {
                        app.notification_daemon.clone()
                    },
                ),
                info_row(
                    tr("Supported features"),
                    tr("Actions, replaces_id, CloseNotification, markup, icons, persistent notifications"),
                ),
            ],
        ),
        info_group(
            tr("Information"),
            vec![
                info_row(
                    tr("Provided by luna-shell over D-Bus"),
                    "org.freedesktop.Notifications",
                ),
                info_row(
                    tr("Status is checked live"),
                    tr("If no daemon owns the name, restart luna-shell — it retries every 5 seconds"),
                ),
            ],
        ),
    ]
}

fn language(app: &SettingsApp) -> Vec<Group> {
    let language_name = app
        .languages
        .iter()
        .find(|(code, _)| *code == app.language)
        .map(|(code, name)| format!("{name} ({code})"))
        .unwrap_or_else(|| app.language.clone());

    let locale = app.region.effective_locale();
    let locale_value = if app.region.locale.is_some() {
        locale
    } else {
        format!("{}: {locale}", tr("System"))
    };
    let timezone = app.region.effective_timezone();
    let timezone_value = if app.region.timezone.is_some() {
        timezone.clone()
    } else {
        format!("{}: {timezone}", tr("System"))
    };

    vec![
        group(
            tr("Language and Region"),
            vec![
                row_sub(
                    tr("Interface language"),
                    tr("Applies immediately to all Luna apps"),
                    Control::Select {
                        value: language_name,
                    },
                    Action::OpenPicker(PickerKind::Language),
                ),
                row_sub(
                    tr("Region and formats"),
                    tr("Date, time and number formats"),
                    Control::Select { value: locale_value },
                    Action::OpenPicker(PickerKind::Locale),
                ),
                row_sub(
                    tr("Time zone"),
                    tr("Used by the Luna clock (per session)"),
                    Control::Select {
                        value: timezone_value,
                    },
                    Action::OpenPicker(PickerKind::Timezone),
                ),
                row_sub(
                    tr("Automatic time"),
                    tr("Synchronize the clock over the network (NTP)"),
                    Control::Switch(app.ntp.unwrap_or(false)),
                    Action::ToggleNtp,
                ),
                row_sub(
                    tr("Apply time zone to the system"),
                    tr("Requires administrator approval (polkit)"),
                    Control::Button {
                        label: tr("Apply"),
                        suggested: false,
                    },
                    Action::ApplySystemTimezone,
                ),
            ],
        ),
        info_group(
            tr("Configuration"),
            vec![
                value_row(
                    tr("Saved to ~/.config/luna/locale.conf"),
                    libluna_i18n::locale_config_path()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "—".to_owned()),
                ),
                value_row(
                    tr("Region configuration"),
                    libluna_core::RegionSettings::config_path()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "—".to_owned()),
                ),
                info_row(
                    tr("Lists are detected from the system"),
                    tr("Interface language uses Luna catalogs; locale and time zone come from the OS"),
                ),
                info_row(
                    tr("Other applications"),
                    tr("Luna apps switch immediately; other programs pick up the language after restart"),
                ),
            ],
        ),
    ]
}

fn lock_screen(app: &SettingsApp) -> Vec<Group> {
    let mode = vec![tr("Light"), tr("Dark"), tr("Auto")];
    let mode_actions = vec![
        Action::SetLockTheme(ThemePreference::Light),
        Action::SetLockTheme(ThemePreference::Dark),
        Action::SetLockTheme(ThemePreference::System),
    ];
    let mode_selected = match app.lock.theme {
        ThemePreference::Light => 0,
        ThemePreference::Dark => 1,
        ThemePreference::System => 2,
    };
    let accent = app.lock.accent.unwrap_or(Accent::Blue);
    let accent_selected = Accent::ALL
        .iter()
        .position(|candidate| *candidate == accent)
        .unwrap_or(0);

    vec![
        group(
            tr("Appearance"),
            vec![
                row_sub(
                    tr("Theme mode"),
                    tr("Colors of the lock screen card and background"),
                    Control::Segmented {
                        options: mode,
                        selected: mode_selected,
                        actions: mode_actions,
                    },
                    Action::None,
                ),
                row(
                    tr("Accent color"),
                    Control::Accents {
                        selected: accent_selected,
                        target: AccentTarget::Lock,
                    },
                    Action::None,
                ),
                row(
                    tr("Show clock"),
                    Control::Switch(app.lock.show_clock),
                    Action::ToggleLockClock,
                ),
            ],
        ),
        group(
            tr("Background"),
            vec![
                row_sub(
                    tr("Desktop wallpaper"),
                    tr("The same wallpaper as the desktop, dimmed"),
                    Control::Check(app.lock.wallpaper == LockWallpaper::Desktop),
                    Action::SetLockWallpaper(LockWallpaper::Desktop),
                ),
                row_sub(
                    tr("Solid color"),
                    tr("No image — a quiet, dark background"),
                    Control::Check(app.lock.wallpaper == LockWallpaper::None),
                    Action::SetLockWallpaper(LockWallpaper::None),
                ),
            ],
        ),
        group(
            tr("Automatic lock"),
            vec![idle_row(
                tr("Lock after inactivity"),
                tr("The lock screen appears when there is no keyboard or mouse input"),
                app.idle.lock_secs,
                Action::SetIdleLock,
            )],
        ),
        info_group(
            tr("Configuration"),
            vec![
                value_row(
                    tr("Lock screen configuration"),
                    libluna_core::LockSettings::config_path()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "—".to_owned()),
                ),
                info_row(
                    tr("No administrator approval required"),
                    tr("The lock screen runs in your session and uses your user settings"),
                ),
            ],
        ),
    ]
}

fn workspaces(app: &SettingsApp) -> Vec<Group> {
    let options: Vec<String> = (1..=libluna_core::MAX_WORKSPACES)
        .map(|n| n.to_string())
        .collect();
    let actions: Vec<Action> = (1..=libluna_core::MAX_WORKSPACES)
        .map(Action::SetWorkspaceCount)
        .collect();
    let selected = app
        .workspace_layout
        .count()
        .saturating_sub(1)
        .min(options.len() - 1);

    let mut name_rows = Vec::new();
    for index in 0..app.workspace_layout.count() {
        name_rows.push(row(
            tr_args("Workspace {n}", &[("n", &(index + 1).to_string())]),
            Control::Value(app.workspace_layout.name(index)),
            Action::EditWorkspaceName(index),
        ));
    }

    vec![
        group(
            tr("Workspaces"),
            vec![
                row_sub(
                    tr("Number of workspaces"),
                    tr("The panel and Super+Tab update immediately"),
                    Control::Segmented {
                        options,
                        selected,
                        actions,
                    },
                    Action::None,
                ),
                row_sub(
                    tr("Switch at screen corners"),
                    tr("Bottom corners change the current workspace"),
                    Control::Switch(app.workspace_layout.corner_switch()),
                    Action::ToggleWorkspaceCorner,
                ),
                row_sub(
                    tr("Overview at the top-left corner"),
                    tr("Shows the same view as Super+Tab"),
                    Control::Switch(app.workspace_layout.overview_corner()),
                    Action::ToggleOverviewCorner,
                ),
            ],
        ),
        group(tr("Names"), name_rows),
        info_group(
            tr("Information"),
            vec![info_row(
                tr("Names and count update immediately"),
                tr("Click a workspace to rename it; the panel refreshes through luna-control"),
            )],
        ),
    ]
}

fn login(app: &SettingsApp) -> Vec<Group> {
    let mode = vec![tr("Light"), tr("Dark"), tr("Auto")];
    let mode_actions = vec![
        Action::SetGreeterTheme(ThemePreference::Light),
        Action::SetGreeterTheme(ThemePreference::Dark),
        Action::SetGreeterTheme(ThemePreference::System),
    ];
    let mode_selected = match app.greeter.theme {
        ThemePreference::Light => 0,
        ThemePreference::Dark => 1,
        ThemePreference::System => 2,
    };
    let accent = app.greeter.accent.unwrap_or(Accent::Blue);
    let accent_selected = Accent::ALL
        .iter()
        .position(|candidate| *candidate == accent)
        .unwrap_or(0);

    let appearance = vec![
        row_sub(
            tr("Theme mode"),
            tr("Colors of the sign-in screen"),
            Control::Segmented {
                options: mode,
                selected: mode_selected,
                actions: mode_actions,
            },
            Action::None,
        ),
        row(
            tr("Accent color"),
            Control::Accents {
                selected: accent_selected,
                target: AccentTarget::Greeter,
            },
            Action::None,
        ),
        row(
            tr("Show clock"),
            Control::Switch(app.greeter.show_clock),
            Action::ToggleGreeterClock,
        ),
    ];

    // Tapety: gradient + wykryte pliki PNG. Greeter dekoduje wyłącznie PNG.
    let mut wallpaper_rows = vec![row_sub(
        tr("Gradient"),
        tr("No image — a subtle accent glow"),
        Control::Check(app.greeter.wallpaper.is_none()),
        Action::ClearGreeterWallpaper,
    )];

    if let Some(desktop) = libluna_render::wallpaper::configured_path() {
        wallpaper_rows.push(row_sub(
            tr("Desktop wallpaper"),
            desktop.display().to_string(),
            Control::Button {
                label: tr("Use"),
                suggested: false,
            },
            Action::UseDesktopWallpaper,
        ));
    }

    wallpaper_rows.push(row_sub(
        tr("Choose from disk"),
        tr("Pick any PNG image from your computer"),
        Control::Button {
            label: tr("Browse…"),
            suggested: false,
        },
        Action::OpenWallpaperPicker,
    ));

    if app.wallpapers.is_empty() {
        wallpaper_rows.push(info_row(
            tr("No wallpapers found"),
            "/usr/share/wallpapers · ~/.local/share/backgrounds",
        ));
    } else {
        for (index, path) in app.wallpapers.iter().enumerate() {
            let selected = app.greeter.wallpaper.as_deref() == Some(path.as_path());
            wallpaper_rows.push(row_sub(
                wallpaper_label(path),
                path.parent()
                    .map(|parent| parent.display().to_string())
                    .unwrap_or_default(),
                Control::Wallpaper {
                    path: path.clone(),
                    selected,
                },
                Action::SetGreeterWallpaper(index),
            ));
        }
    }

    if let Some(current) = &app.greeter.wallpaper {
        if !app.wallpapers.iter().any(|path| path == current) {
            wallpaper_rows.push(value_row(
                tr("Current wallpaper"),
                current.display().to_string(),
            ));
        }
    }

    let apply = vec![
        row_sub(
            tr("Apply login screen changes"),
            if app.greeter_dirty {
                tr("You have unsaved changes — administrator approval is required")
            } else {
                tr("Saves the current selection with administrator approval")
            },
            Control::Button {
                label: tr("Apply changes"),
                suggested: true,
            },
            Action::ApplyGreeter,
        ),
        info_row(
            tr("Changes require administrator approval"),
            tr("The sign-in screen runs as a separate user"),
        ),
    ];

    vec![
        group(tr("Appearance"), appearance),
        group(tr("Apply changes"), apply),
        group(tr("Wallpaper"), wallpaper_rows),
        info_group(
            tr("Configuration"),
            vec![value_row(
                tr("Configuration file"),
                GreeterSettings::config_path().display().to_string(),
            )],
        ),
    ]
}

/// Czytelna nazwa tapety: `katalog/plik.png`.
fn wallpaper_label(path: &Path) -> String {
    let file = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("wallpaper.png");
    match path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
    {
        Some(dir) => format!("{dir}/{file}"),
        None => file.to_owned(),
    }
}

/// Maksymalna liczba proponowanych tapet (żeby strona pozostała użyteczna).
const MAX_WALLPAPERS: usize = 40;

/// Tapety systemowe i użytkownika w formacie PNG (tylko taki greeter dekoduje).
pub fn wallpapers() -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/usr/share/wallpapers"),
        PathBuf::from("/usr/share/backgrounds"),
        PathBuf::from("/usr/local/share/wallpapers"),
        PathBuf::from("/var/lib/luna/wallpapers"),
    ];
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    if let Some(data) = data_home {
        roots.push(data.join("backgrounds"));
        roots.push(data.join("wallpapers"));
    }

    let mut found = Vec::new();
    for root in roots {
        scan_png(&root, 0, &mut found);
    }
    found.sort();
    found.dedup();
    found.truncate(MAX_WALLPAPERS);
    found
}

fn scan_png(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 3 || out.len() >= MAX_WALLPAPERS {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if out.len() >= MAX_WALLPAPERS {
            return;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            scan_png(&path, depth + 1, out);
        } else if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
        {
            out.push(path);
        }
    }
}

fn network(app: &SettingsApp) -> Vec<Group> {
    let snapshot = &app.network;
    let mut status_rows = vec![
        value_row(
            tr("Backend"),
            if snapshot.available {
                "NetworkManager (D-Bus)".to_owned()
            } else {
                tr("Unavailable")
            },
        ),
        value_row(tr("Connectivity"), tr(snapshot.connectivity.label())),
    ];
    if let Some(error) = &snapshot.error {
        status_rows.push(info_row(tr("Network error"), error.clone()));
    }

    let mut wired_rows = Vec::new();
    for device in snapshot.ethernet() {
        let state = tr(device.state.label());
        let subtitle = match &device.ip4 {
            Some(ip) => format!("{} · {} · {ip}", device.interface, state),
            None => format!("{} · {}", device.interface, state),
        };
        let title = if device.driver.is_empty() {
            device.interface.clone()
        } else {
            format!("{} ({})", device.interface, device.driver)
        };
        let connected = device.state.is_connected();
        wired_rows.push(row_sub(
            title,
            subtitle,
            Control::Button {
                label: if connected {
                    tr("Disconnect")
                } else {
                    tr("Connect")
                },
                suggested: false,
            },
            if connected {
                Action::DisconnectDevice(device.path.clone())
            } else {
                Action::ConnectEthernet(device.path.clone())
            },
        ));
        let wired = device.wired.clone().unwrap_or_default();
        let summary = if wired.is_dhcp() {
            tr("Automatic (DHCP)")
        } else if wired.address.is_empty() {
            tr("Manual")
        } else {
            wired.address.clone()
        };
        wired_rows.push(row_sub(
            tr("IPv4 configuration"),
            summary,
            Control::Button {
                label: tr("Edit…"),
                suggested: false,
            },
            Action::OpenWiredEditor(device.path.clone()),
        ));
        if !wired.is_dhcp() {
            if !wired.gateway.is_empty() {
                wired_rows.push(value_row(tr("Gateway"), wired.gateway.clone()));
            }
            if !wired.dns.is_empty() {
                wired_rows.push(value_row(tr("DNS"), wired.dns.join(", ")));
            }
        }
        if device.metered {
            wired_rows.push(info_row(
                tr("Metered connection"),
                tr("The provider may limit the amount of transferred data"),
            ));
        }
    }
    if wired_rows.is_empty() {
        wired_rows.push(info_row(
            tr("No wired device"),
            tr("Ethernet adapter not detected"),
        ));
    }

    let mut wifi_rows = vec![row_sub(
        tr("Wi-Fi"),
        tr("Wireless networks are managed by NetworkManager"),
        Control::Switch(snapshot.wifi_enabled),
        Action::ToggleWifi,
    )];
    if snapshot.wifi_enabled {
        if snapshot.wifi.is_empty() {
            wifi_rows.push(info_row(
                tr("No networks found"),
                tr("Use Refresh to scan again"),
            ));
        } else {
            for net in &snapshot.wifi {
                let mut subtitle = format!(
                    "{}% · {} · {}",
                    net.signal,
                    tr(net.security.label()),
                    net.device_name
                );
                if net.known {
                    subtitle.push_str(" · ");
                    subtitle.push_str(&tr("Saved"));
                }
                let supported = net.security.is_supported();
                let control = if supported {
                    Control::Button {
                        label: if net.active {
                            tr("Disconnect")
                        } else {
                            tr("Connect")
                        },
                        suggested: false,
                    }
                } else {
                    Control::Value(tr("Unsupported"))
                };
                let action = if !supported {
                    Action::None
                } else if net.active {
                    Action::DisconnectDevice(net.device.clone())
                } else {
                    Action::ConnectWifi {
                        device: net.device.clone(),
                        ssid: net.ssid.clone(),
                        security: net.security,
                        hidden: false,
                    }
                };
                wifi_rows.push(row_sub(net.ssid.clone(), subtitle, control, action));
            }
        }
        if let Some(active) = snapshot.active_wifi() {
            if let Some(known) = snapshot
                .known_wifi
                .iter()
                .find(|known| known.ssid == active.ssid)
            {
                wifi_rows.push(row_sub(
                    tr("Connect automatically"),
                    active.ssid.clone(),
                    Control::Switch(known.autoconnect),
                    Action::SetAutoconnect {
                        profile: known.profile.clone(),
                        enabled: !known.autoconnect,
                    },
                ));
            }
            wifi_rows.push(row_sub(
                tr("Forget this network"),
                active.ssid.clone(),
                Control::Button {
                    label: tr("Forget"),
                    suggested: false,
                },
                Action::ForgetWifi(active.ssid.clone()),
            ));
        }
        wifi_rows.push(row_sub(
            tr("Hidden network"),
            tr("Join a network that does not broadcast its name"),
            Control::Button {
                label: tr("Join…"),
                suggested: false,
            },
            Action::OpenHiddenNetwork,
        ));
        wifi_rows.push(row(
            tr("Scan again"),
            Control::Button {
                label: tr("Refresh"),
                suggested: false,
            },
            Action::RefreshWifi,
        ));
    } else {
        wifi_rows.push(info_row(
            tr("Wi-Fi is off"),
            tr("Turn on the switch above to scan for networks"),
        ));
    }

    let active_ssid = snapshot.active_wifi().map(|net| net.ssid.as_str());
    let saved_rows: Vec<Row> = snapshot
        .known_wifi
        .iter()
        .filter(|known| Some(known.ssid.as_str()) != active_ssid)
        .map(|known| {
            row_sub(
                known.ssid.clone(),
                if known.autoconnect {
                    tr("Saved · connects automatically")
                } else {
                    tr("Saved")
                },
                Control::Button {
                    label: tr("Forget"),
                    suggested: false,
                },
                Action::ForgetProfile(known.profile.clone()),
            )
        })
        .collect();

    let mut groups = vec![
        group(tr("Status"), status_rows),
        group(tr("Wired"), wired_rows),
        group(tr("Wi-Fi"), wifi_rows),
    ];
    if !saved_rows.is_empty() {
        groups.push(group(tr("Saved networks"), saved_rows));
    }
    groups.push(vpn_group(app));
    groups.push(info_group(
        tr("Configuration"),
        vec![
            value_row(tr("Backend"), "NetworkManager (D-Bus)"),
            info_row(
                tr("Passwords are sent to NetworkManager only"),
                tr("Luna Settings does not store Wi-Fi passwords"),
            ),
            info_row(
                tr("Editing saved connections"),
                tr("Changing a saved profile may require administrator authorization"),
            ),
            info_row(
                tr("Not supported yet"),
                tr("WEP and 802.1X (enterprise) networks"),
            ),
        ],
    ));
    groups
}

/// Grupa VPN: istniejące profile NetworkManagera + import pliku.
fn vpn_group(app: &SettingsApp) -> Group {
    let mut rows = Vec::new();
    if !app.vpn.available && app.vpn.profiles.is_empty() {
        rows.push(info_row(
            tr("No VPN profiles"),
            app.vpn
                .error
                .clone()
                .unwrap_or_else(|| tr("NetworkManager did not report any VPN support")),
        ));
        return group(tr("VPN"), rows);
    }
    for profile in &app.vpn.profiles {
        let state = if profile.active {
            tr("Connected")
        } else {
            tr("Not connected")
        };
        let subtitle = format!("{} · {state}", vpn_kind_label(&profile.kind));
        let (labels, actions) = if profile.active {
            (
                vec![tr("Disconnect").to_owned(), tr("Remove").to_owned()],
                vec![
                    Action::VpnDisconnect(profile.uuid.clone()),
                    Action::VpnRemove(profile.uuid.clone()),
                ],
            )
        } else {
            (
                vec![tr("Connect").to_owned(), tr("Remove").to_owned()],
                vec![
                    Action::VpnConnect(profile.uuid.clone()),
                    Action::VpnRemove(profile.uuid.clone()),
                ],
            )
        };
        rows.push(row_buttons(profile.name.clone(), subtitle, labels, actions));
    }
    rows.push(row_sub(
        tr("Import VPN profile"),
        tr("OpenVPN (.ovpn) or WireGuard (.conf) — imported by NetworkManager"),
        Control::Button {
            label: tr("Import…"),
            suggested: false,
        },
        Action::OpenVpnImport,
    ));
    group(tr("VPN"), rows)
}

fn vpn_kind_label(kind: &str) -> String {
    match kind {
        "wireguard" => "WireGuard".to_owned(),
        "vpn" => "OpenVPN / VPN".to_owned(),
        other => other.to_owned(),
    }
}

fn input(app: &SettingsApp) -> Vec<Group> {
    let delays = [200_u32, 400, 600, 800, 1000];
    let delay_labels: Vec<String> = delays.iter().map(|delay| format!("{delay} ms")).collect();
    let delay_selected = delays
        .iter()
        .enumerate()
        .min_by_key(|(_, delay)| delay.abs_diff(app.input.repeat_delay()))
        .map(|(index, _)| index)
        .unwrap_or(2);
    let rates = [15_u32, 25, 35, 50];
    let rate_labels: Vec<String> = rates.iter().map(|rate| format!("{rate}/s")).collect();
    let rate_selected = rates
        .iter()
        .enumerate()
        .min_by_key(|(_, rate)| rate.abs_diff(app.input.repeat_rate()))
        .map(|(index, _)| index)
        .unwrap_or(1);

    vec![
        group(
            tr("Keyboard"),
            vec![
                row_sub(
                    tr("Key repeat delay"),
                    tr("How long a key must be held before it repeats"),
                    Control::Segmented {
                        options: delay_labels,
                        selected: delay_selected,
                        actions: delays.iter().copied().map(Action::SetRepeatDelay).collect(),
                    },
                    Action::None,
                ),
                row_sub(
                    tr("Key repeat speed"),
                    tr("Repeats per second while a key is held"),
                    Control::Segmented {
                        options: rate_labels,
                        selected: rate_selected,
                        actions: rates.iter().copied().map(Action::SetRepeatRate).collect(),
                    },
                    Action::None,
                ),
            ],
        ),
        group(
            tr("Mouse and touchpad"),
            vec![row_sub(
                tr("Natural scrolling"),
                tr("Move content in the direction of your fingers"),
                Control::Switch(app.input.natural_scroll()),
                Action::ToggleNaturalScroll,
            )],
        ),
        group(
            tr("In development"),
            vec![
                info_row(
                    tr("Tap to click"),
                    tr("Tap-to-click and touchpad gestures are planned after the beta"),
                ),
                info_row(
                    tr("Pointer speed"),
                    tr("Pointer acceleration is not configurable yet"),
                ),
            ],
        ),
        info_group(
            tr("Configuration"),
            vec![
                value_row(
                    tr("Input configuration"),
                    libluna_core::InputSettings::config_path()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "—".to_owned()),
                ),
                info_row(
                    tr("Changes apply immediately"),
                    tr("Selene receives repeat and scrolling settings through luna-control"),
                ),
            ],
        ),
    ]
}

fn displays(app: &SettingsApp) -> Vec<Group> {
    if app.monitors.is_empty() {
        return vec![group(
            tr("Monitor"),
            vec![
                info_row(
                    tr("No displays detected"),
                    tr("Selene did not report any output (nested mode shows one synthetic screen)"),
                ),
                info_row(tr("Diagnostics"), "$XDG_RUNTIME_DIR/luna-monitors"),
            ],
        )];
    }

    let selected = app.selected_monitor.min(app.monitors.len() - 1);
    let monitor_rows = app
        .monitors
        .iter()
        .enumerate()
        .map(|(index, monitor)| {
            row_sub(
                monitor.name.clone(),
                monitor_subtitle(monitor),
                Control::Check(index == selected),
                Action::SelectMonitor(index),
            )
        })
        .collect::<Vec<_>>();

    let layout_rows = vec![
        info_row(
            tr("Monitor arrangement"),
            tr("Drag screens in the picture above; positions snap to edges and are saved immediately"),
        ),
        row(
            tr("Auto arrange"),
            Control::Button {
                label: tr("Arrange"),
                suggested: false,
            },
            Action::AutoArrangeMonitors,
        ),
    ];

    let monitor = &app.monitors[selected];
    let saved = app.displays.mode_for(&monitor.name);
    let chosen = saved.or_else(|| monitor.current_mode());
    let resolution = chosen
        .map(|mode| format!("{} × {}", mode.width, mode.height))
        .unwrap_or_else(|| tr("Auto (native)"));
    let refresh = chosen
        .map(|mode| {
            if mode.refresh > 0 {
                format!("{} Hz", mode.refresh)
            } else {
                "—".to_owned()
            }
        })
        .unwrap_or_else(|| tr("Auto"));
    let mode_rows = vec![
        row_sub(
            tr("Resolution"),
            if saved.is_some() {
                tr("Applies immediately and is remembered after logout")
            } else {
                tr("Automatic mode from the monitor")
            },
            Control::Select { value: resolution },
            Action::OpenPicker(PickerKind::Resolution),
        ),
        row(
            tr("Refresh rate"),
            Control::Select { value: refresh },
            Action::OpenPicker(PickerKind::Refresh),
        ),
    ];

    let color_rows = vec![row_sub(
        tr("Screen color"),
        tr("Night light — warmer tones in the evening"),
        Control::Slider {
            kind: SliderKind::NightLight,
            value: app.displays.night_light as f64 / 100.0,
        },
        Action::SetNightLight(0),
    )];

    let brightness = match app.brightness {
        Some(v) => row(
            tr("Screen brightness"),
            Control::Slider {
                kind: SliderKind::Brightness,
                value: v as f64 / 100.0,
            },
            Action::SetBrightness(0),
        ),
        None => value_row(tr("Brightness"), tr("Unavailable")),
    };

    vec![
        group(tr("Layout"), layout_rows),
        group(tr("Monitors"), monitor_rows),
        group(tr("Mode"), mode_rows),
        group(tr("Color"), color_rows),
        color_profiles_group(app),
        group(tr("Brightness"), vec![brightness]),
        info_group(
            tr("Configuration"),
            vec![
                info_row(
                    tr("Resolution and refresh rate"),
                    tr("Selene switches the mode live and remembers it in ~/.config/luna/displays.conf"),
                ),
                info_row(
                    tr("Night light"),
                    tr("Uses the gamma table of the graphics card; applies immediately"),
                ),
                info_row(
                    tr("Monitor arrangement"),
                    tr("Saved in ~/.config/luna/displays.conf; Selene applies it live"),
                ),
                info_row(
                    tr("Color profiles"),
                    tr("ICC profiles are managed by colord; assigning one may require administrator authorization"),
                ),
            ],
        ),
    ]
}

/// Kalibracja kolorów: urządzenia i profile colord.
fn color_profiles_group(app: &SettingsApp) -> Group {
    let displays: Vec<_> = app
        .colors
        .devices
        .iter()
        .filter(|device| device.kind == "display")
        .collect();
    if displays.is_empty() {
        let subtitle = app
            .colors
            .error
            .clone()
            .unwrap_or_else(|| tr("colord did not report any display"));
        return group(
            tr("Color profiles"),
            vec![info_row(tr("Color profiles unavailable"), subtitle)],
        );
    }
    let mut rows = Vec::new();
    for device in displays {
        let current = app
            .colors
            .profiles
            .iter()
            .find(|profile| device.profiles.contains(&profile.id))
            .map(|profile| profile.title.clone())
            .unwrap_or_else(|| tr("None"));
        rows.push(row_sub(
            device.model.clone(),
            tr("Calibration profile for this display"),
            Control::Select { value: current },
            Action::OpenColorPicker(device.id.clone()),
        ));
    }
    rows.push(row_sub(
        tr("Import color profile"),
        tr("ICC/ICM file — imported into colord"),
        Control::Button {
            label: tr("Import…"),
            suggested: false,
        },
        Action::OpenColorImport,
    ));
    group(tr("Color profiles"), rows)
}

/// Krótki opis monitora do listy (rozmiar, Hz, liczba trybów, pozycja).
fn monitor_subtitle(monitor: &Monitor) -> String {
    let mut parts = Vec::new();
    if monitor.width > 0 && monitor.height > 0 {
        parts.push(format!("{} × {}", monitor.width, monitor.height));
    }
    if monitor.refresh > 0 {
        parts.push(format!("{} Hz", monitor.refresh));
    }
    if !monitor.enabled {
        parts.push(tr("Disabled"));
    }
    if monitor.modes.len() > 1 {
        parts.push(format!("{} {}", monitor.modes.len(), tr("modes")));
    }
    if monitor.x != 0 || monitor.y != 0 {
        parts.push(format!("{},{}", monitor.x, monitor.y));
    }
    parts.join(" · ")
}

/// Etykieta progu bezczynności („Nigdy”, „5 min”, „1 h”).
/// Krótkie etykiety mieszczą się w jednym wierszu (7 progów obok siebie).
fn idle_label(secs: u32) -> String {
    match secs {
        0 => "—".to_owned(),
        3600 => "1h".to_owned(),
        secs if secs % 60 == 0 => format!("{}m", secs / 60),
        secs => format!("{secs}s"),
    }
}

/// Wiersz z wyborem progu bezczynności; `action` buduje akcję dla sekund.
fn idle_row(
    title: impl Into<String>,
    subtitle: impl Into<String>,
    current: u32,
    action: impl Fn(u32) -> Action,
) -> Row {
    let options: Vec<String> = libluna_core::idle::CHOICES
        .iter()
        .map(|secs| idle_label(*secs))
        .collect();
    let selected = libluna_core::idle::CHOICES
        .iter()
        .position(|secs| *secs == current)
        .unwrap_or(0);
    let actions = libluna_core::idle::CHOICES
        .iter()
        .map(|secs| action(*secs))
        .collect();
    row_sub(
        title,
        subtitle,
        Control::Segmented {
            options,
            selected,
            actions,
        },
        Action::None,
    )
}

fn sound(app: &SettingsApp) -> Vec<Group> {
    let (volume, muted) = app.volume.unwrap_or((0, false));
    let sink = app.audio_sink.clone().unwrap_or_else(|| tr("Unavailable"));
    let volume_row = if app.volume.is_some() {
        row_sub(
            tr("Volume"),
            sink,
            Control::Slider {
                kind: SliderKind::Volume,
                value: volume as f64 / 100.0,
            },
            Action::SetVolume(0),
        )
    } else {
        row_sub(
            tr("Volume"),
            sink,
            Control::Value(tr("Unavailable")),
            Action::None,
        )
    };

    // ── Wyjście: lista sprzętowych urządzeń ──
    let mut output_rows = vec![
        volume_row,
        row(tr("Mute"), Control::Switch(muted), Action::ToggleMute),
    ];
    if app.audio.sinks.is_empty() {
        output_rows.push(value_row(
            tr("Output device"),
            tr("No audio device detected"),
        ));
    } else {
        for device in app.audio.hardware_sinks() {
            // Gdy gra korektor, domyślnym wyjściem PipeWire jest jego węzeł —
            // „aktywne” jest wtedy sprzętowe wyjście zapamiętane przez korektor.
            let active = if app.eq.enabled {
                app.eq_output_id == Some(device.id)
            } else {
                device.is_default
            };
            output_rows.push(row(
                device.description.clone(),
                Control::Check(active),
                Action::SetSink(device.id),
            ));
        }
    }

    // ── Wejście: mikrofony i inne źródła ──
    let source_rows = if app.audio.sources.is_empty() {
        vec![value_row(
            tr("Input device"),
            tr("No audio device detected"),
        )]
    } else {
        app.audio
            .sources
            .iter()
            .map(|device| {
                row(
                    device.description.clone(),
                    Control::Check(device.is_default),
                    Action::SetSource(device.id),
                )
            })
            .collect()
    };

    // ── Korektor ──
    let mut eq_rows = vec![row_sub(
        tr("Equalizer"),
        if !app.eq_available {
            tr("The luna-eq plugin is not installed")
        } else if app.eq.enabled {
            tr("Output goes through the equalizer")
        } else {
            tr("Adds a 10-band equalizer to the output")
        },
        Control::Switch(app.eq.enabled),
        Action::ToggleEqualizer,
    )];
    if app.eq.enabled && app.eq_available {
        let detected = app.eq.detected_preset();
        let mut options: Vec<String> = Preset::ALL
            .iter()
            .map(|preset| match preset {
                Preset::Flat => tr("Flat"),
                Preset::Bass => tr("Bass"),
                Preset::Treble => tr("Treble"),
                Preset::Vocal => tr("Vocal"),
                Preset::Rock => tr("Rock"),
                Preset::Custom => tr("Custom"),
            })
            .collect();
        let mut actions: Vec<Action> = Preset::ALL
            .iter()
            .map(|preset| Action::SetEqPreset(*preset))
            .collect();
        let selected = match Preset::ALL.iter().position(|p| *p == detected) {
            Some(index) => index,
            None => {
                options.push(tr("Custom"));
                actions.push(Action::SetEqPreset(Preset::Custom));
                options.len() - 1
            }
        };
        eq_rows.push(row(
            tr("Preset"),
            Control::Segmented {
                options,
                selected,
                actions,
            },
            Action::None,
        ));
        eq_rows.push(row_sub(
            tr("Preamp"),
            format!("{:+.1} dB", app.eq.preamp),
            Control::Slider {
                kind: SliderKind::EqPreamp,
                value: SliderKind::eq_position(app.eq.preamp),
            },
            Action::None,
        ));
        for (index, (label, gain)) in BAND_LABELS.iter().zip(app.eq.bands.iter()).enumerate() {
            eq_rows.push(row_sub(
                (*label).to_owned(),
                format!("{gain:+.1} dB"),
                Control::Slider {
                    kind: SliderKind::EqBand(index),
                    value: SliderKind::eq_position(*gain),
                },
                Action::None,
            ));
        }
        if app.eq.max_boost() > 0.0 && app.eq.preamp >= 0.0 {
            eq_rows.push(row_sub(
                tr("Clipping protection"),
                tr("Set the preamp a few dB below the highest boost"),
                Control::None,
                Action::None,
            ));
        }
        if let Some(err) = &app.eq_error {
            eq_rows.push(info_row(tr("Equalizer problem"), err.clone()));
        }
    }

    vec![
        group(tr("Sound"), output_rows),
        group(tr("Input"), source_rows),
        group(tr("Equalizer"), eq_rows),
    ]
}

fn users(app: &SettingsApp) -> Vec<Group> {
    let mut rows = Vec::new();
    if app.accounts.is_empty() {
        rows.push(value_row(
            tr("Accounts"),
            tr("No local accounts were found"),
        ));
    }
    for account in &app.accounts {
        let role = if account.is_admin() {
            tr("Administrator")
        } else {
            tr("Standard user")
        };
        let mut subtitle = role;
        if account.current {
            subtitle.push_str(" · ");
            subtitle.push_str(&tr("Current account"));
        }
        rows.push(row_sub(
            account.display_name().to_owned(),
            subtitle,
            Control::Button {
                label: tr("Change password"),
                suggested: false,
            },
            Action::OpenPasswordChange(account.login.clone()),
        ));
    }

    let current = app.accounts.iter().find(|account| account.current);
    let details = match current {
        Some(account) => vec![
            value_row(tr("Login"), account.login.clone()),
            value_row(tr("Full name"), account.display_name().to_owned()),
            value_row(
                tr("Account type"),
                if account.is_admin() {
                    tr("Administrator")
                } else {
                    tr("Standard user")
                },
            ),
            value_row(tr("Home folder"), account.home.clone()),
            value_row(tr("Shell"), account.shell.clone()),
        ],
        None => vec![value_row(tr("Current account"), tr("Unavailable"))],
    };

    let helper_ready = crate::accounts_admin::helper_path().is_some();
    vec![
        group(tr("Accounts"), rows),
        sessions_group(app),
        group(
            tr("Manage accounts"),
            vec![row_sub(
                tr("Add account"),
                if helper_ready {
                    tr("Creates a home folder and asks for the administrator password")
                } else {
                    tr("The luna-accounts helper is missing — reinstall Luna")
                },
                Control::Button {
                    label: tr("Add account"),
                    suggested: true,
                },
                Action::OpenAddAccount,
            )],
        ),
        group(tr("Current account"), details),
        info_group(
            tr("Account management"),
            vec![
                info_row(
                    tr("How changes are applied"),
                    tr("Passwords are sent to the luna-accounts helper through sudo (never as arguments)"),
                ),
                info_row(
                    tr("Administrator rights"),
                    tr("Membership in the wheel, sudo or admin group"),
                ),
            ],
        ),
    ]
}

/// Inne sesje zalogowanych użytkowników (przełączanie przez logind).
fn sessions_group(app: &SettingsApp) -> Group {
    let mut rows = Vec::new();
    if !app.sessions.available && app.sessions.sessions.is_empty() {
        rows.push(info_row(
            tr("Sessions unavailable"),
            app.sessions
                .error
                .clone()
                .unwrap_or_else(|| tr("logind did not report any sessions")),
        ));
    }
    for session in &app.sessions.sessions {
        let kind = match session.kind.as_str() {
            "wayland" => "Wayland",
            "x11" => "X11",
            "tty" => "TTY",
            other => other,
        };
        let subtitle = format!("{kind} · {}", seat_label(&session.seat));
        if session.current {
            rows.push(row_sub(
                session.user.clone(),
                format!("{} · {}", subtitle, tr("Current session")),
                Control::Value(tr("This session").to_owned()),
                Action::None,
            ));
        } else {
            rows.push(row_buttons(
                session.user.clone(),
                subtitle,
                vec![tr("Switch").to_owned()],
                vec![Action::SwitchSession(session.id.clone())],
            ));
        }
    }
    if rows.is_empty() {
        rows.push(info_row(
            tr("Other sessions"),
            tr("Only your session is active"),
        ));
    }
    group(tr("Sessions"), rows)
}

fn seat_label(seat: &str) -> String {
    if seat.is_empty() {
        tr("None").to_owned()
    } else {
        seat.to_owned()
    }
}

fn bluetooth(app: &SettingsApp) -> Vec<Group> {
    let snapshot = &app.bluetooth;
    let mut groups = Vec::new();
    if snapshot.adapters.is_empty() {
        groups.push(group(
            tr("Bluetooth"),
            vec![info_row(
                tr("No Bluetooth adapter found"),
                snapshot
                    .error
                    .clone()
                    .unwrap_or_else(|| tr("Make sure the bluetooth service is running")),
            )],
        ));
    } else {
        let adapter_rows: Vec<Row> = snapshot
            .adapters
            .iter()
            .map(|adapter| {
                row_sub(
                    adapter.name.clone(),
                    adapter.address.clone(),
                    Control::Switch(adapter.powered),
                    Action::SetBluetoothPowered {
                        adapter: adapter.path.clone(),
                        powered: !adapter.powered,
                    },
                )
            })
            .collect();
        groups.push(group(tr("Adapter"), adapter_rows));

        let adapter = &snapshot.adapters[0];
        groups.push(group(
            tr("Discovery"),
            vec![row_sub(
                tr("Scan for devices"),
                if !adapter.powered {
                    tr("Turn on the adapter to scan")
                } else if adapter.discovering {
                    tr("Scanning…")
                } else {
                    tr("Start a scan to find nearby devices")
                },
                Control::Switch(adapter.discovering),
                Action::SetBluetoothDiscovery {
                    adapter: adapter.path.clone(),
                    on: !adapter.discovering,
                },
            )],
        ));
    }

    let mut device_rows = Vec::new();
    for device in &snapshot.devices {
        let state = if device.connected {
            tr("Connected")
        } else if device.paired {
            tr("Paired")
        } else {
            tr("Available")
        };
        let mut subtitle = device.address.clone();
        if let Some(rssi) = device.rssi {
            subtitle.push_str(&format!(" · {rssi} dBm"));
        }
        subtitle.push_str(" · ");
        subtitle.push_str(&state);

        let (labels, actions) = if device.connected {
            (
                vec![tr("Disconnect").to_owned()],
                vec![Action::BluetoothDisconnect(device.path.clone())],
            )
        } else if device.paired {
            (
                vec![tr("Connect").to_owned(), tr("Remove").to_owned()],
                vec![
                    Action::BluetoothConnect(device.path.clone()),
                    Action::BluetoothRemove {
                        adapter: device.adapter.clone(),
                        device: device.path.clone(),
                    },
                ],
            )
        } else {
            (
                vec![tr("Pair").to_owned()],
                vec![Action::BluetoothPair(device.path.clone())],
            )
        };
        device_rows.push(row_buttons(device.name.clone(), subtitle, labels, actions));
    }
    if device_rows.is_empty() {
        device_rows.push(info_row(
            tr("Devices"),
            tr("No devices yet — start scanning"),
        ));
    }
    groups.push(group(tr("Devices"), device_rows));
    groups.push(info_group(
        tr("Bluetooth"),
        vec![
            value_row(tr("Backend"), "BlueZ (D-Bus)"),
            info_row(
                tr("Pairing"),
                tr("Devices that require a PIN or passkey are not supported yet"),
            ),
        ],
    ));
    groups
}

fn printers(app: &SettingsApp) -> Vec<Group> {
    let snapshot = &app.printers;
    let mut rows = Vec::new();
    for printer in &snapshot.printers {
        let mut subtitle = printer_state(printer);
        if let Some(uri) = &printer.uri {
            subtitle.push_str(" · ");
            subtitle.push_str(uri);
        }
        let mut labels = Vec::new();
        let mut actions = Vec::new();
        if !printer.is_default {
            labels.push(tr("Set default").to_owned());
            actions.push(Action::PrinterDefault(printer.name.clone()));
        }
        labels.push(if printer.enabled {
            tr("Disable").to_owned()
        } else {
            tr("Enable").to_owned()
        });
        actions.push(Action::PrinterEnabled {
            name: printer.name.clone(),
            enabled: !printer.enabled,
        });
        labels.push(tr("Remove").to_owned());
        actions.push(Action::PrinterRemove(printer.name.clone()));
        rows.push(row_buttons(printer.name.clone(), subtitle, labels, actions));
    }
    if rows.is_empty() {
        rows.push(info_row(
            tr("No printers found"),
            snapshot
                .error
                .clone()
                .unwrap_or_else(|| tr("Make sure CUPS is running")),
        ));
    }
    vec![
        group(tr("Printers"), rows),
        group(
            tr("Add printer"),
            vec![row_sub(
                tr("Network printer (driverless)"),
                tr("IPP address, e.g. ipp://printer.local:631/ipp/print"),
                Control::Button {
                    label: tr("Add printer…"),
                    suggested: true,
                },
                Action::OpenPrinterAdd,
            )],
        ),
        info_group(
            tr("Printing"),
            vec![
                value_row(tr("Backend"), "CUPS (lpstat / lpadmin)"),
                info_row(
                    tr("Administrator authorization"),
                    tr("Adding, removing and changing printers asks polkit for permission"),
                ),
            ],
        ),
    ]
}

/// Stan drukarki z `lpstat` sprowadzony do krótkiej etykiety.
fn printer_state(printer: &luna::printers::Printer) -> String {
    let info = printer.info.to_ascii_lowercase();
    if !printer.enabled || info.contains("disabled") {
        tr("Disabled")
    } else if info.contains("now printing") {
        tr("Printing")
    } else if info.contains("is idle") {
        tr("Idle")
    } else {
        printer.info.clone()
    }
}

fn accessibility(app: &SettingsApp) -> Vec<Group> {
    let options: Vec<String> = libluna_core::TEXT_SCALE_CHOICES
        .iter()
        .map(|percent| format!("{percent} %"))
        .collect();
    let actions: Vec<Action> = libluna_core::TEXT_SCALE_CHOICES
        .iter()
        .map(|percent| Action::SetTextScale(*percent))
        .collect();
    let selected = libluna_core::TEXT_SCALE_CHOICES
        .iter()
        .position(|percent| *percent == app.accessibility.text_scale)
        .unwrap_or(0);
    vec![
        group(
            tr("Text size"),
            vec![row_sub(
                tr("Text size"),
                tr("Applies to all Luna applications and the shell"),
                Control::Segmented {
                    options,
                    selected,
                    actions,
                },
                Action::None,
            )],
        ),
        group(
            tr("Contrast"),
            vec![row_sub(
                tr("High contrast"),
                tr("Stronger borders and black-and-white text"),
                Control::Switch(app.accessibility.high_contrast),
                Action::ToggleHighContrast,
            )],
        ),
        group(
            tr("Motion"),
            vec![row_sub(
                tr("Reduce motion"),
                tr("Disables toast and transition animations"),
                Control::Switch(app.accessibility.reduce_motion),
                Action::ToggleReduceMotion,
            )],
        ),
        info_group(
            tr("Accessibility"),
            vec![
                info_row(tr("Configuration"), "~/.config/luna/accessibility.conf"),
                info_row(
                    tr("Magnifier and on-screen keyboard"),
                    tr("Planned — not part of the beta yet"),
                ),
            ],
        ),
    ]
}

fn default_apps(app: &SettingsApp) -> Vec<Group> {
    let mut rows = Vec::new();
    if !app.defaults.available && app.defaults.apps.is_empty() {
        rows.push(info_row(
            tr("Default applications unavailable"),
            app.defaults
                .error
                .clone()
                .unwrap_or_else(|| tr("xdg-mime is not available")),
        ));
    } else {
        for (index, category) in luna::default_apps::CATEGORIES.iter().enumerate() {
            let value = app
                .defaults
                .current_name(index)
                .map(str::to_owned)
                .unwrap_or_else(|| tr("Not set"));
            rows.push(row_sub(
                tr(category.key),
                tr("Application used for this kind of file or link"),
                Control::Select { value },
                Action::OpenDefaultAppPicker(index),
            ));
        }
    }
    vec![
        group(tr("Default applications"), rows),
        info_group(
            tr("Default applications"),
            vec![
                info_row(
                    tr("How it works"),
                    tr("Changes are written to mimeapps.list with xdg-mime"),
                ),
                info_row(tr("Configuration"), "~/.config/mimeapps.list"),
            ],
        ),
    ]
}

fn shortcuts(app: &SettingsApp) -> Vec<Group> {
    let mut rows = Vec::new();
    for action in libluna_core::ShortcutAction::ALL {
        let label = shortcut_action_label(action);
        let value = app
            .shortcuts
            .binding(action)
            .map(|binding| binding.format())
            .unwrap_or_else(|| tr("Not set"));
        let subtitle = match action {
            libluna_core::ShortcutAction::Workspace(_) => {
                tr("Hold Shift while pressing to move the window")
            }
            _ => tr("Click to record a new combination"),
        };
        rows.push(row_sub(
            label,
            subtitle,
            Control::Button {
                label: value,
                suggested: false,
            },
            Action::RecordShortcut(action),
        ));
    }
    vec![
        group(tr("Shortcuts"), rows),
        group(
            tr("Reset"),
            vec![row_sub(
                tr("Restore defaults"),
                tr("All shortcuts return to the built-in values"),
                Control::Button {
                    label: tr("Reset to defaults"),
                    suggested: false,
                },
                Action::ResetShortcuts,
            )],
        ),
        info_group(
            tr("Shortcuts"),
            vec![
                info_row(tr("Configuration"), "~/.config/luna/shortcuts.conf"),
                info_row(
                    tr("Fixed shortcuts"),
                    tr("Alt+Tab, Alt+F4 and media keys are always handled by the compositor"),
                ),
            ],
        ),
    ]
}

/// Nazwa akcji skrótu (z numerem przestrzeni).
pub fn shortcut_action_label(action: libluna_core::ShortcutAction) -> String {
    match action {
        libluna_core::ShortcutAction::Workspace(index) => {
            tr_args("Workspace {n}", &[("n", &(index + 1).to_string())])
        }
        other => tr(other.label_key()),
    }
}

fn power(app: &SettingsApp) -> Vec<Group> {
    let battery_rows = match app.battery {
        Some(b) => vec![
            value_row(tr("Level"), format!("{}%", b.percent)),
            value_row(
                tr("Status"),
                if b.charging {
                    tr("Charging")
                } else {
                    tr("Discharging")
                },
            ),
            value_row(
                tr("Remaining time"),
                match b.minutes {
                    Some(m) if m > 0 => format!("{} h {} min", m / 60, m % 60),
                    _ => "—".to_owned(),
                },
            ),
        ],
        None => vec![value_row(tr("Battery"), tr("No battery (desktop)"))],
    };

    let mut groups = vec![group(tr("Battery"), battery_rows)];
    if app.power_profiles.available {
        groups.push(power_profile_group(app));
    } else {
        let reason =
            tr("Install power-profiles-daemon to choose Power saver, Balanced or Performance");
        groups.push(group(
            tr("Power profile"),
            vec![
                info_row(tr("Power profiles are unavailable"), reason),
                row(
                    tr("Use the power-saver profile"),
                    Control::Switch(app.power_saver),
                    Action::TogglePowerSaver,
                )
                .with_subtitle(tr(
                    "Without the daemon this only limits the compositor frame rate",
                )),
            ],
        ));
    }
    groups.push(group(
        tr("Screen and sleep"),
        vec![
            idle_row(
                tr("Turn off screen after"),
                tr("The screen goes dark until you move the mouse or press a key"),
                app.idle.screen_off_secs,
                Action::SetIdleScreenOff,
            ),
            row_sub(
                tr("Lock when the screen turns off"),
                tr("Requires the lock screen (works with luna-lock)"),
                Control::Switch(app.idle.lock_on_blank),
                Action::ToggleLockOnBlank,
            ),
            idle_row(
                tr("Sleep after"),
                tr("Suspends the computer using logind"),
                app.idle.suspend_secs,
                Action::SetIdleSuspend,
            ),
        ],
    ));
    groups
}

fn power_profile_group(app: &SettingsApp) -> Group {
    let profiles = &app.power_profiles;
    let options: Vec<String> = profiles
        .choices
        .iter()
        .map(|name| power_profile_label(name))
        .collect();
    let actions: Vec<Action> = profiles
        .choices
        .iter()
        .map(|name| Action::SetPowerProfile(name.clone()))
        .collect();
    let selected = profiles
        .choices
        .iter()
        .position(|name| name == &profiles.active)
        .unwrap_or(0);
    group(
        tr("Power profile"),
        vec![row_sub(
            tr("Profile"),
            tr("Managed by power-profiles-daemon; applies immediately"),
            Control::Segmented {
                options,
                selected,
                actions,
            },
            Action::None,
        )],
    )
}

fn power_profile_label(name: &str) -> String {
    match name {
        "power-saver" => tr("Power saver").to_owned(),
        "balanced" => tr("Balanced").to_owned(),
        "performance" => tr("Performance").to_owned(),
        other => other.to_owned(),
    }
}

fn system() -> Vec<Group> {
    vec![
        group(
            tr("Device"),
            vec![
                value_row(tr("Hostname"), hostname()),
                value_row(tr("Operating system"), os_pretty_name()),
                value_row(tr("Kernel"), kernel_release()),
                value_row(tr("Architecture"), std::env::consts::ARCH),
            ],
        ),
        group(
            tr("Environment"),
            vec![
                value_row(tr("User"), user_name()),
                value_row(tr("Compositor"), "Selene"),
                value_row(tr("Shell"), "luna-shell"),
                value_row(tr("Toolkit"), "libLuna"),
                value_row(tr("Luna version"), env!("CARGO_PKG_VERSION")),
            ],
        ),
    ]
}

fn about(app: &SettingsApp) -> Vec<Group> {
    let mut groups = vec![
        group(
            tr("About Luna"),
            vec![
                row(
                    tr("Version"),
                    Control::Value(env!("CARGO_PKG_VERSION").to_owned()),
                    Action::AboutVersionClick,
                ),
                value_row(tr("Desktop environment"), "Luna"),
                value_row(tr("Compositor"), "Selene"),
                value_row(tr("Toolkit"), "libLuna"),
                value_row(tr("License"), "GPL-3.0-or-later"),
            ],
        ),
        info_group(
            tr("Information"),
            vec![
                info_row(
                    tr("Luna is a lightweight desktop environment written in Rust."),
                    "Wayland · libLuna",
                ),
                info_row(
                    tr("Built with libLuna — no GTK, no Qt."),
                    "github.com/mijsys/libLuna",
                ),
            ],
        ),
    ];
    if app.easter_egg {
        groups.insert(
            1,
            group(
                tr("Secret"),
                vec![
                    info_row(
                        tr("Moon egg"),
                        tr("Mokosz watches over your desktop. Five clicks well spent."),
                    ),
                    value_row(tr("Secret code"), "LUNA-5-MOKOSZ"),
                ],
            ),
        );
    }
    groups
}

impl Row {
    fn with_subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }
}

/// Monitor wykryty przez Selene (migawka `luna-monitors`) albo w `/sys/class/drm`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
    pub name: String,
    pub connected: bool,
    pub enabled: bool,
    pub width: u32,
    pub height: u32,
    pub refresh: u32,
    /// Pozycja logiczna w układzie (od Selene; 0 przy odczycie z `/sys`).
    pub x: i32,
    pub y: i32,
    /// Tryby zgłoszone przez modeset (puste = brak danych o odświeżaniu).
    pub modes: Vec<OutputMode>,
}

impl Monitor {
    pub fn current_mode(&self) -> Option<OutputMode> {
        (self.width > 0 && self.height > 0).then(|| {
            OutputMode::new(
                self.width.min(u16::MAX as u32) as u16,
                self.height.min(u16::MAX as u32) as u16,
                self.refresh.min(u16::MAX as u32) as u16,
            )
        })
    }

    /// Rozdzielczości bez powtórzeń, malejąco (największa pierwsza).
    pub fn resolutions(&self) -> Vec<(u16, u16)> {
        let mut out: Vec<(u16, u16)> = self
            .modes
            .iter()
            .map(|mode| mode.resolution())
            .chain(self.current_mode().map(|mode| mode.resolution()))
            .collect();
        out.sort_by(|a, b| {
            (u32::from(b.0) * u32::from(b.1))
                .cmp(&(u32::from(a.0) * u32::from(a.1)))
                .then_with(|| a.0.cmp(&b.0))
        });
        out.dedup();
        out
    }

    /// Odświeżania dostępne dla rozdzielczości, malejąco.
    pub fn refresh_rates(&self, resolution: (u16, u16)) -> Vec<u16> {
        let mut out: Vec<u16> = self
            .modes
            .iter()
            .filter(|mode| mode.resolution() == resolution)
            .map(|mode| mode.refresh)
            .filter(|refresh| *refresh > 0)
            .collect();
        if let Some(current) = self.current_mode() {
            if current.resolution() == resolution && current.refresh > 0 {
                out.push(current.refresh);
            }
        }
        out.sort_unstable_by(|a, b| b.cmp(a));
        out.dedup();
        out
    }
}

/// Lista monitorów: migawka działającej Selene jest rozstrzygająca, `/sys/class/drm`
/// to tylko zapas (gdy kompozytor nie działa albo pisze w trybie nested).
///
/// Wcześniej obie listy były scalane — a gdy nazwy złączy się różniły (np. przez
/// błąd w mapowaniu typów DRM) Ustawienia pokazywały „duchowy” trzeci monitor.
pub fn monitors() -> Vec<Monitor> {
    let mut out = choose_monitors(runtime_monitors(), sysfs_monitors);
    // Bez migawki Selene nie znamy pozycji — układamy poziomo, żeby prostokąty
    // na grafice nie leżały jeden na drugim.
    if out.len() > 1 && out.iter().all(|monitor| monitor.x == 0 && monitor.y == 0) {
        let mut x = 0;
        for monitor in &mut out {
            monitor.x = x;
            monitor.y = 0;
            x += monitor.width.max(1) as i32;
        }
    }
    out.sort_by(|a, b| a.x.cmp(&b.x).then_with(|| a.name.cmp(&b.name)));
    out
}

fn choose_monitors(runtime: Vec<Monitor>, sysfs: impl FnOnce() -> Vec<Monitor>) -> Vec<Monitor> {
    if runtime.is_empty() {
        sysfs()
    } else {
        runtime
    }
}

/// Czyta `$XDG_RUNTIME_DIR/luna-monitors` (Selene zapisuje go po modesecie).
fn runtime_monitors() -> Vec<Monitor> {
    let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") else {
        return Vec::new();
    };
    let path = PathBuf::from(runtime).join("luna-monitors");
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    parse_runtime_outputs(&text)
}

/// Format: `nazwa|szer|wys|hz|x|y|skala|tryby`, tryby rozdzielone przecinkami.
pub(crate) fn parse_runtime_outputs(text: &str) -> Vec<Monitor> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('|').collect();
        if fields.len() < 8 {
            continue;
        }
        let name = fields[0].trim();
        if name.is_empty() {
            continue;
        }
        let width = fields[1].trim().parse().unwrap_or(0);
        let height = fields[2].trim().parse().unwrap_or(0);
        let refresh = fields[3].trim().parse().unwrap_or(0);
        let x = fields[4].trim().parse().unwrap_or(0);
        let y = fields[5].trim().parse().unwrap_or(0);
        let mut modes: Vec<OutputMode> = fields[7]
            .split(',')
            .filter_map(|mode| OutputMode::parse(mode.trim()))
            .collect();
        modes.sort_by(|a, b| {
            (u32::from(b.width) * u32::from(b.height))
                .cmp(&(u32::from(a.width) * u32::from(a.height)))
                .then_with(|| b.refresh.cmp(&a.refresh))
        });
        modes.dedup();
        out.push(Monitor {
            name: name.to_owned(),
            connected: true,
            enabled: true,
            width,
            height,
            refresh,
            x,
            y,
            modes,
        });
    }
    out
}

/// Złącza z `/sys/class/drm` (działa też bez działającej Selene).
fn sysfs_monitors() -> Vec<Monitor> {
    let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let dir_name = entry.file_name().to_string_lossy().into_owned();
        // Złącza mają postać `card0-HDMI-A-1`; pomijamy `card0`, `renderD128` itd.
        let Some((_, connector)) = dir_name.split_once('-') else {
            continue;
        };
        if connector.is_empty() {
            continue;
        }
        let path = entry.path();
        let status = read_trim(path.join("status")).unwrap_or_default();
        let connected = status == "connected";
        if !connected {
            continue;
        }
        let enabled = read_trim(path.join("enabled")).as_deref() == Some("enabled");
        let mut modes: Vec<OutputMode> = std::fs::read_to_string(path.join("modes"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| OutputMode::parse(line.trim()))
            .collect();
        modes.sort_by(|a, b| {
            (u32::from(b.width) * u32::from(b.height))
                .cmp(&(u32::from(a.width) * u32::from(a.height)))
                .then_with(|| b.refresh.cmp(&a.refresh))
        });
        modes.dedup();
        if modes.is_empty() {
            // Podłączone, ale bez EDID/trybów (np. martwy port w docku) — to nie monitor.
            continue;
        }
        // Nawet gdy kompozytor nie ma trybu ustawionego, znamy geometrię z EDID —
        // grafika układu ma wtedy sensowne proporcje.
        let current = modes.first().copied();
        out.push(Monitor {
            name: connector.to_owned(),
            connected,
            enabled,
            width: current.map(|mode| u32::from(mode.width)).unwrap_or(0),
            height: current.map(|mode| u32::from(mode.height)).unwrap_or(0),
            refresh: current.map(|mode| u32::from(mode.refresh)).unwrap_or(0),
            x: 0,
            y: 0,
            modes,
        });
    }
    out
}

fn read_trim(path: std::path::PathBuf) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn os_pretty_name() -> String {
    std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                let (key, value) = line.split_once('=')?;
                (key.trim() == "PRETTY_NAME")
                    .then(|| value.trim().trim_matches('"').trim_matches('\'').to_owned())
            })
        })
        .unwrap_or_else(|| "Linux".to_owned())
}

fn kernel_release() -> String {
    read_trim("/proc/sys/kernel/osrelease".into()).unwrap_or_else(|| tr("Not detected"))
}

fn hostname() -> String {
    read_trim("/etc/hostname".into())
        .or_else(|| std::env::var("HOSTNAME").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| tr("Not detected"))
}

fn user_name() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "?".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_pages_have_titles_and_icons() {
        for page in Page::ALL {
            assert!(!page.title_key().is_empty());
            assert!(!page.subtitle_key().is_empty());
        }
    }

    #[test]
    fn page_slugs_round_trip() {
        for page in Page::ALL {
            assert_eq!(Page::by_name(page.slug()), Some(page));
        }
        assert_eq!(Page::by_name(" Network "), Some(Page::Network));
        assert_eq!(Page::by_name("nope"), None);
    }

    fn app_with(page: Page) -> SettingsApp {
        let mut app = SettingsApp::new(&libluna_core::ThemeSettings::load());
        app.demo = true;
        app.page = page;
        app
    }

    fn row_titles(groups: &[Group]) -> Vec<String> {
        groups
            .iter()
            .flat_map(|group| group.rows.iter().map(|row| row.title.clone()))
            .collect()
    }

    #[test]
    fn bluetooth_page_lists_devices_with_contextual_actions() {
        use luna::bluetooth::{Adapter, Device, Snapshot};

        let mut app = app_with(Page::Bluetooth);
        app.bluetooth = Snapshot {
            available: true,
            adapters: vec![Adapter {
                path: "/org/bluez/hci0".to_owned(),
                name: "hci0".to_owned(),
                address: "AA:BB".to_owned(),
                powered: true,
                discovering: true,
            }],
            devices: vec![
                Device {
                    path: "/org/bluez/hci0/dev_1".to_owned(),
                    adapter: "/org/bluez/hci0".to_owned(),
                    address: "11:22".to_owned(),
                    name: "Słuchawki".to_owned(),
                    paired: true,
                    connected: true,
                    trusted: true,
                    rssi: Some(-40),
                    icon: None,
                },
                Device {
                    path: "/org/bluez/hci0/dev_2".to_owned(),
                    adapter: "/org/bluez/hci0".to_owned(),
                    address: "33:44".to_owned(),
                    name: "Telefon".to_owned(),
                    paired: false,
                    connected: false,
                    trusted: false,
                    rssi: None,
                    icon: None,
                },
            ],
            error: None,
        };
        let groups = groups(&app);
        let titles = row_titles(&groups);
        assert!(titles.contains(&"Słuchawki".to_owned()));
        assert!(titles.contains(&"Telefon".to_owned()));

        let connected = groups
            .iter()
            .flat_map(|group| group.rows.iter())
            .find(|row| row.title == "Słuchawki")
            .unwrap();
        match &connected.control {
            Control::Buttons { labels, .. } => {
                assert_eq!(labels, &vec![tr("Disconnect")])
            }
            other => panic!("oczekiwano przycisków, jest {other:?}"),
        }
        let unpaired = groups
            .iter()
            .flat_map(|group| group.rows.iter())
            .find(|row| row.title == "Telefon")
            .unwrap();
        match &unpaired.control {
            Control::Buttons { actions, .. } => {
                assert_eq!(
                    actions,
                    &vec![Action::BluetoothPair("/org/bluez/hci0/dev_2".to_owned())]
                )
            }
            other => panic!("oczekiwano przycisków, jest {other:?}"),
        }
    }

    #[test]
    fn power_page_shows_profiles_when_daemon_is_available() {
        let mut app = app_with(Page::Power);
        app.power_profiles = luna::power_profile::Profiles {
            available: true,
            active: "performance".to_owned(),
            choices: vec![
                "power-saver".to_owned(),
                "balanced".to_owned(),
                "performance".to_owned(),
            ],
            error: None,
        };
        let groups = groups(&app);
        let row = groups
            .iter()
            .flat_map(|group| group.rows.iter())
            .find(|row| row.title == tr("Profile"))
            .expect("wiersz profilu");
        match &row.control {
            Control::Segmented {
                selected, actions, ..
            } => {
                assert_eq!(*selected, 2);
                assert_eq!(
                    actions[2],
                    Action::SetPowerProfile("performance".to_owned())
                );
            }
            other => panic!("oczekiwano Segmented, jest {other:?}"),
        }
    }

    #[test]
    fn network_page_lists_vpn_profiles() {
        let mut app = app_with(Page::Network);
        app.vpn = luna::vpn::Snapshot {
            available: true,
            profiles: vec![luna::vpn::VpnProfile {
                name: "Biuro".to_owned(),
                uuid: "u-1".to_owned(),
                path: "/settings/1".to_owned(),
                kind: "wireguard".to_owned(),
                active: true,
            }],
            error: None,
        };
        let groups = groups(&app);
        let row = groups
            .iter()
            .flat_map(|group| group.rows.iter())
            .find(|row| row.title == "Biuro")
            .expect("profil VPN");
        match &row.control {
            Control::Buttons { actions, .. } => {
                assert_eq!(actions[0], Action::VpnDisconnect("u-1".to_owned()));
            }
            other => panic!("oczekiwano przycisków, jest {other:?}"),
        }
    }

    #[test]
    fn printers_page_offers_default_enable_and_remove() {
        let mut app = app_with(Page::Printers);
        app.printers = luna::printers::Snapshot {
            available: true,
            printers: vec![luna::printers::Printer {
                name: "biuro".to_owned(),
                info: "is idle.".to_owned(),
                is_default: false,
                enabled: true,
                uri: Some("ipp://biuro/ipp/print".to_owned()),
            }],
            default: None,
            error: None,
        };
        let groups = groups(&app);
        let row = groups
            .iter()
            .flat_map(|group| group.rows.iter())
            .find(|row| row.title == "biuro")
            .expect("drukarka");
        match &row.control {
            Control::Buttons {
                labels, actions, ..
            } => {
                assert_eq!(labels.len(), 3);
                assert!(actions.contains(&Action::PrinterDefault("biuro".to_owned())));
                assert!(actions.contains(&Action::PrinterRemove("biuro".to_owned())));
            }
            other => panic!("oczekiwano przycisków, jest {other:?}"),
        }
    }

    #[test]
    fn accessibility_page_uses_text_scale_choices() {
        let mut app = app_with(Page::Accessibility);
        app.accessibility.text_scale = 125;
        let groups = groups(&app);
        let row = groups
            .iter()
            .flat_map(|group| group.rows.iter())
            .find(|row| row.title == tr("Text size"))
            .expect("wiersz skali");
        match &row.control {
            Control::Segmented {
                options,
                selected,
                actions,
            } => {
                assert_eq!(options.len(), libluna_core::TEXT_SCALE_CHOICES.len());
                assert_eq!(*selected, 1);
                assert_eq!(actions[1], Action::SetTextScale(125));
            }
            other => panic!("oczekiwano Segmented, jest {other:?}"),
        }
    }

    #[test]
    fn users_page_lists_other_sessions() {
        let mut app = app_with(Page::Users);
        app.sessions = luna::sessions::Snapshot {
            available: true,
            sessions: vec![luna::sessions::Session {
                id: "4".to_owned(),
                user: "gosc".to_owned(),
                uid: 1001,
                seat: "seat0".to_owned(),
                kind: "wayland".to_owned(),
                active: false,
                current: false,
            }],
            error: None,
        };
        let groups = groups(&app);
        let row = groups
            .iter()
            .flat_map(|group| group.rows.iter())
            .find(|row| row.title == "gosc")
            .expect("sesja innego użytkownika");
        match &row.control {
            Control::Buttons { actions, .. } => {
                assert_eq!(actions[0], Action::SwitchSession("4".to_owned()))
            }
            other => panic!("oczekiwano przycisków, jest {other:?}"),
        }
    }

    #[test]
    fn default_apps_page_shows_current_programs() {
        let mut app = app_with(Page::DefaultApps);
        app.defaults = luna::default_apps::Defaults {
            available: true,
            apps: vec![luna::default_apps::DesktopApp {
                desktop_id: "firefox.desktop".to_owned(),
                name: "Firefox".to_owned(),
                exec: "firefox %u".to_owned(),
                categories: vec!["WebBrowser".to_owned()],
                mime_types: vec!["text/html".to_owned()],
            }],
            current: vec![Some("firefox.desktop".to_owned())],
            error: None,
        };
        let groups = groups(&app);
        let row = groups
            .iter()
            .flat_map(|group| group.rows.iter())
            .find(|row| row.title == tr("Web browser"))
            .expect("wiersz przeglądarki");
        match &row.control {
            Control::Select { value } => assert_eq!(value, "Firefox"),
            other => panic!("oczekiwano Select, jest {other:?}"),
        }
        assert_eq!(row.action, Action::OpenDefaultAppPicker(0));
    }

    #[test]
    fn default_app_picker_filters_apps_by_role() {
        let mut app = app_with(Page::DefaultApps);
        app.defaults = luna::default_apps::Defaults {
            available: true,
            apps: vec![
                luna::default_apps::DesktopApp {
                    desktop_id: "firefox.desktop".to_owned(),
                    name: "Firefox".to_owned(),
                    exec: "firefox".to_owned(),
                    categories: vec!["WebBrowser".to_owned()],
                    mime_types: vec!["text/html".to_owned()],
                },
                luna::default_apps::DesktopApp {
                    desktop_id: "gimp.desktop".to_owned(),
                    name: "GIMP".to_owned(),
                    exec: "gimp".to_owned(),
                    categories: vec!["Graphics".to_owned()],
                    mime_types: vec!["image/png".to_owned()],
                },
            ],
            current: vec![None; luna::default_apps::CATEGORIES.len()],
            error: None,
        };
        app.default_app_target = Some(0);
        let items = app.picker_items(PickerKind::DefaultApp);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "Firefox");

        app.default_app_target = Some(3);
        let items = app.picker_items(PickerKind::DefaultApp);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "GIMP");
    }

    #[test]
    fn shortcuts_page_lists_every_action_with_its_binding() {
        let mut app = app_with(Page::Shortcuts);
        app.shortcuts = libluna_core::ShortcutSettings::default();
        let groups = groups(&app);
        let recorded: Vec<libluna_core::ShortcutAction> = groups
            .iter()
            .flat_map(|group| group.rows.iter())
            .filter_map(|row| match row.action {
                Action::RecordShortcut(action) => Some(action),
                _ => None,
            })
            .collect();
        assert_eq!(recorded.len(), libluna_core::ShortcutAction::ALL.len());
        let close = groups
            .iter()
            .flat_map(|group| group.rows.iter())
            .find(|row| {
                row.action == Action::RecordShortcut(libluna_core::ShortcutAction::CloseWindow)
            })
            .expect("skrót zamykania");
        match &close.control {
            Control::Button { label, .. } => assert_eq!(label, "Super+Q"),
            other => panic!("oczekiwano przycisku, jest {other:?}"),
        }
    }

    #[test]
    fn displays_page_lists_color_profiles() {
        let mut app = app_with(Page::Displays);
        app.colors = luna::colors::Snapshot {
            available: true,
            devices: vec![luna::colors::ColorDevice {
                id: "xrandr-HDMI-A-1".to_owned(),
                model: "HDMI-A-1".to_owned(),
                kind: "display".to_owned(),
                profiles: vec!["icc-srgb".to_owned()],
                enabled: true,
            }],
            profiles: vec![luna::colors::ColorProfile {
                id: "icc-srgb".to_owned(),
                title: "sRGB".to_owned(),
                filename: None,
                kind: Some("display".to_owned()),
            }],
            error: None,
        };
        let groups = groups(&app);
        let color_group = groups
            .iter()
            .find(|group| group.title == tr("Color profiles"))
            .expect("grupa profili kolorów");
        let row = color_group
            .rows
            .iter()
            .find(|row| row.title == "HDMI-A-1")
            .expect("urządzenie colord");
        match &row.control {
            Control::Select { value } => assert_eq!(value, "sRGB"),
            other => panic!("oczekiwano Select, jest {other:?}"),
        }
        assert_eq!(
            row.action,
            Action::OpenColorPicker("xrandr-HDMI-A-1".to_owned())
        );
    }

    #[test]
    fn about_page_reveals_the_secret_after_the_easter_egg() {
        let mut app = app_with(Page::About);
        app.easter_egg = true;
        let groups = groups(&app);
        assert!(
            groups.iter().any(|group| group.title == tr("Secret")),
            "sekretna grupa po odkryciu"
        );
        let version = groups
            .iter()
            .flat_map(|group| group.rows.iter())
            .find(|row| row.title == tr("Version"))
            .expect("wiersz wersji");
        match &version.control {
            Control::Value(value) => assert!(!value.is_empty()),
            other => panic!("oczekiwano Value, jest {other:?}"),
        }
        assert_eq!(version.action, Action::AboutVersionClick);
    }

    #[test]
    fn idle_row_offers_never_and_common_intervals() {
        let row = idle_row("Test", "opis", 300, Action::SetIdleLock);
        match &row.control {
            Control::Segmented {
                options,
                selected,
                actions,
            } => {
                assert_eq!(options.len(), libluna_core::idle::CHOICES.len());
                assert_eq!(*selected, 2, "300 s = 5 min");
                assert_eq!(actions[0], Action::SetIdleLock(0));
                assert_eq!(actions[2], Action::SetIdleLock(300));
            }
            other => panic!("oczekiwano Segmented, jest {other:?}"),
        }
    }

    #[test]
    fn users_page_lists_accounts_and_current_profile() {
        use libluna_core::accounts::UserAccount;

        let mut app = SettingsApp::new(&libluna_core::ThemeSettings::load());
        app.page = Page::Users;
        app.accounts = vec![
            UserAccount {
                login: "ala".to_owned(),
                full_name: "Ala Kowalska".to_owned(),
                uid: 1000,
                gid: 1000,
                home: "/home/ala".to_owned(),
                shell: "/bin/bash".to_owned(),
                admin: true,
                current: true,
            },
            UserAccount {
                login: "bob".to_owned(),
                full_name: String::new(),
                uid: 1001,
                gid: 1001,
                home: "/home/bob".to_owned(),
                shell: "/bin/sh".to_owned(),
                admin: false,
                current: false,
            },
        ];
        let groups = groups(&app);
        let accounts = groups
            .iter()
            .find(|group| group.title == tr("Accounts"))
            .expect("grupa Accounts");
        assert_eq!(accounts.rows.len(), 2);
        assert!(accounts.rows[0].title.contains("Ala"));
        assert!(accounts.rows[0]
            .subtitle
            .as_deref()
            .is_some_and(|s| s.contains(&tr("Current account"))));
        assert_eq!(
            accounts.rows[0].action,
            Action::OpenPasswordChange("ala".to_owned())
        );
        assert_eq!(
            accounts.rows[1].action,
            Action::OpenPasswordChange("bob".to_owned())
        );
        assert!(matches!(
            accounts.rows[0].control,
            Control::Button {
                suggested: false,
                ..
            }
        ));
        let manage = groups
            .iter()
            .find(|group| group.title == tr("Manage accounts"))
            .expect("grupa zarządzania");
        assert_eq!(manage.rows[0].action, Action::OpenAddAccount);
        let details = groups
            .iter()
            .find(|group| group.title == tr("Current account"))
            .expect("grupa szczegółów");
        assert!(details.rows.iter().any(|row| row.title == tr("Login")));
    }

    #[test]
    fn equalizer_sliders_map_decibels() {
        assert_eq!(SliderKind::eq_db(0), -12);
        assert_eq!(SliderKind::eq_db(50), 0);
        assert_eq!(SliderKind::eq_db(100), 12);
        for db in [-12.0f32, -6.0, 0.0, 5.0, 12.0] {
            let pos = SliderKind::eq_position(db);
            assert!((pos - ((db + 12.0) / 24.0) as f64).abs() < 1e-9);
        }
    }

    #[test]
    fn sound_page_lists_devices_and_equalizer() {
        use luna::equalizer::{AudioStatus, Device};

        let mut app = SettingsApp::new(&libluna_core::ThemeSettings::load());
        app.page = Page::Sound;
        app.audio = AudioStatus {
            sinks: vec![
                Device {
                    id: 46,
                    description: "Głośniki".to_owned(),
                    is_default: false,
                },
                Device {
                    id: 57,
                    description: "HDMI".to_owned(),
                    is_default: false,
                },
            ],
            sources: vec![Device {
                id: 35,
                description: "Mikrofon".to_owned(),
                is_default: true,
            }],
            eq_id: Some(73),
        };
        app.eq_available = true;
        app.eq = luna::equalizer::Equalizer::default();
        app.eq.enabled = true;
        app.eq.set_preset(Preset::Bass);
        app.eq_output_id = Some(46);

        let groups = groups(&app);
        let sound = groups
            .iter()
            .find(|group| group.title == tr("Sound"))
            .expect("grupa Sound");
        // Dwa sprzętowe wyjścia + suwak głośności + wyciszenie.
        assert_eq!(
            sound
                .rows
                .iter()
                .filter(|row| matches!(row.action, Action::SetSink(_)))
                .count(),
            2
        );
        // Aktywne jest wyjście, na które trafia dźwięk po korekcji (nie „domyślne” PipeWire).
        let marked: Vec<u32> = sound
            .rows
            .iter()
            .filter_map(|row| match (&row.control, &row.action) {
                (Control::Check(true), Action::SetSink(id)) => Some(*id),
                _ => None,
            })
            .collect();
        assert_eq!(marked, vec![46]);

        let eq = groups
            .iter()
            .find(|group| group.title == tr("Equalizer"))
            .expect("grupa Equalizer");
        assert!(eq
            .rows
            .iter()
            .any(|row| matches!(row.action, Action::ToggleEqualizer)));
        assert_eq!(
            eq.rows
                .iter()
                .filter(|row| matches!(
                    row.control,
                    Control::Slider {
                        kind: SliderKind::EqBand(_),
                        ..
                    }
                ))
                .count(),
            10,
            "korektor ma 10 pasm"
        );
        assert!(eq.rows.iter().any(|row| matches!(
            row.control,
            Control::Slider {
                kind: SliderKind::EqPreamp,
                ..
            }
        )));
        assert!(eq
            .rows
            .iter()
            .any(|row| matches!(row.control, Control::Segmented { ref options, .. } if options.iter().any(|o| *o == tr("Bass")))));
    }

    #[test]
    fn wallpaper_labels_keep_directory_context() {
        assert_eq!(
            wallpaper_label(Path::new("/usr/share/wallpapers/paganlinux/tlo.png")),
            "paganlinux/tlo.png"
        );
        assert_eq!(wallpaper_label(Path::new("tlo.png")), "tlo.png");
    }

    #[test]
    fn monitors_parse_card_prefix() {
        // Funkcja czyta prawdziwe /sys; testujemy tylko, że nie panikuje i zwraca
        // listę posortowaną po pozycji, bez duplikatów (na maszynie bez DRM pusta).
        let list = monitors();
        let mut names: Vec<_> = list.iter().map(|m| m.name.clone()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), list.len(), "zduplikowane złącza");
        assert!(list
            .windows(2)
            .all(|pair| (pair[0].x, &pair[0].name) <= (pair[1].x, &pair[1].name)));
        assert!(list
            .iter()
            .all(|monitor| monitor.connected && !monitor.name.is_empty()));
    }

    #[test]
    fn runtime_snapshot_wins_over_sysfs() {
        fn monitor(name: &str) -> Monitor {
            Monitor {
                name: name.to_owned(),
                connected: true,
                enabled: true,
                width: 1920,
                height: 1080,
                refresh: 60,
                x: 0,
                y: 0,
                modes: vec![OutputMode::new(1920, 1080, 60)],
            }
        }

        let mut sysfs_called = false;
        let merged = choose_monitors(vec![monitor("HDMI-A-1")], || {
            sysfs_called = true;
            vec![monitor("DP-1")]
        });
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].name, "HDMI-A-1");
        assert!(
            !sysfs_called,
            "/sys nie powinien być czytany, gdy jest migawka"
        );

        let merged = choose_monitors(Vec::new(), || vec![monitor("DP-1")]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].name, "DP-1");
    }

    #[test]
    fn runtime_outputs_snapshot_parses_layout_and_modes() {
        let text = "# luna-monitors\n\
            eDP-1|1920|1080|60|0|0|1|1920x1080@60,1920x1080@48,1280x720@60\n\
            HDMI-A-1|2560|1440|144|1920|0|1|2560x1440@144,1920x1080@120\n";
        let list = parse_runtime_outputs(text);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "eDP-1");
        assert_eq!(
            list[0].current_mode(),
            Some(OutputMode::new(1920, 1080, 60))
        );
        assert_eq!(list[0].modes.len(), 3);
        assert_eq!(list[1].x, 1920);
        assert_eq!(list[1].refresh, 144);
        assert_eq!(
            list[1].modes.first(),
            Some(&OutputMode::new(2560, 1440, 144))
        );
    }

    #[test]
    fn monitor_lists_resolutions_and_refresh_rates() {
        let monitor = Monitor {
            name: "eDP-1".to_owned(),
            connected: true,
            enabled: true,
            width: 1920,
            height: 1080,
            refresh: 60,
            x: 0,
            y: 0,
            modes: vec![
                OutputMode::new(1920, 1080, 60),
                OutputMode::new(1920, 1080, 144),
                OutputMode::new(1280, 720, 60),
            ],
        };
        assert_eq!(monitor.resolutions(), vec![(1920, 1080), (1280, 720)]);
        assert_eq!(monitor.refresh_rates((1920, 1080)), vec![144, 60]);
        assert_eq!(monitor.refresh_rates((1280, 720)), vec![60]);
    }

    #[test]
    fn displays_page_shows_a_graphical_layout() {
        let mut app = SettingsApp::new(&libluna_core::ThemeSettings::load());
        app.page = Page::Displays;
        app.monitors = vec![
            Monitor {
                name: "eDP-1".to_owned(),
                connected: true,
                enabled: true,
                width: 1920,
                height: 1080,
                refresh: 60,
                x: 0,
                y: 0,
                modes: vec![OutputMode::new(1920, 1080, 60)],
            },
            Monitor {
                name: "HDMI-A-1".to_owned(),
                connected: true,
                enabled: true,
                width: 2560,
                height: 1440,
                refresh: 144,
                x: 1920,
                y: 0,
                modes: vec![
                    OutputMode::new(2560, 1440, 144),
                    OutputMode::new(2560, 1440, 60),
                ],
            },
        ];
        app.displays
            .set_mode("HDMI-A-1", Some(OutputMode::new(2560, 1440, 144)));
        app.selected_monitor = 1;

        let groups = groups(&app);
        let mode = groups
            .iter()
            .find(|group| group.title == tr("Mode"))
            .expect("mode group");
        assert!(mode
            .rows
            .iter()
            .any(|row| matches!(row.action, Action::OpenPicker(PickerKind::Resolution))));
        assert!(mode
            .rows
            .iter()
            .any(|row| matches!(row.action, Action::OpenPicker(PickerKind::Refresh))));

        let color = groups
            .iter()
            .find(|group| group.title == tr("Color"))
            .expect("color group");
        assert!(matches!(
            color.rows[0].control,
            Control::Slider {
                kind: SliderKind::NightLight,
                ..
            }
        ));
    }

    #[test]
    fn network_page_lists_wired_ipv4_and_saved_networks() {
        use luna::network::{Device, DeviceKind, DeviceState, Ipv4Method, KnownWifi, WiredConfig};

        let mut app = SettingsApp::new(&libluna_core::ThemeSettings::load());
        app.page = Page::Network;
        app.network.devices = vec![Device {
            path: "/dev/2".to_owned(),
            interface: "enp9s0".to_owned(),
            kind: DeviceKind::Ethernet,
            state: DeviceState::Activated,
            driver: "r8169".to_owned(),
            hw_address: "aa:bb".to_owned(),
            connection: Some("Wired connection".to_owned()),
            ip4: Some("192.168.1.20/24".to_owned()),
            wired: Some(WiredConfig {
                method: Ipv4Method::Manual,
                address: "192.168.1.20/24".to_owned(),
                gateway: "192.168.1.1".to_owned(),
                dns: vec!["1.1.1.1".to_owned()],
                ..WiredConfig::default()
            }),
            metered: true,
        }];
        app.network.known_wifi = vec![KnownWifi {
            ssid: "Pracownia".to_owned(),
            profile: "/settings/1".to_owned(),
            autoconnect: false,
        }];

        let groups = groups(&app);
        let wired = groups
            .iter()
            .find(|group| group.title == tr("Wired"))
            .expect("wired group");
        assert!(wired.rows.iter().any(|row| {
            row.title == tr("IPv4 configuration")
                && matches!(row.action, Action::OpenWiredEditor(_))
        }));
        assert!(wired.rows.iter().any(|row| row.title == tr("DNS")));
        assert!(wired
            .rows
            .iter()
            .any(|row| row.title == tr("Metered connection")));

        let saved = groups
            .iter()
            .find(|group| group.title == tr("Saved networks"))
            .expect("saved group");
        assert_eq!(saved.rows.len(), 1);
        assert_eq!(saved.rows[0].title, "Pracownia");
        assert!(matches!(
            saved.rows[0].action,
            Action::ForgetProfile(ref profile) if profile == "/settings/1"
        ));
    }

    #[test]
    fn technical_groups_are_split_for_the_info_sheet() {
        let mut app = SettingsApp::new(&libluna_core::ThemeSettings::load());
        for page in Page::ALL {
            app.page = page;
            let all = groups(&app);
            let content = content_groups(&app);
            let info = info_groups(&app);
            assert_eq!(content.len() + info.len(), all.len(), "{page:?}");
            assert!(content.iter().all(|group| !group.info), "{page:?}");
            assert!(info.iter().all(|group| group.info), "{page:?}");
        }
    }
}
