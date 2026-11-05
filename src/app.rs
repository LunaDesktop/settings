//! Stan aplikacji i pętla klatki `luna-settings`.
//!
//! Podział jest ten sam co w powłoce: dane i akcje ([`SettingsApp`]) są oddzielone
//! od rysowania ([`crate::ui`]). Dzięki temu strony da się testować bez okna, a te
//! same funkcje systemowe (`luna::system`) obsługują panel i ustawienia.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use libluna_core::accounts::UserAccount;
use libluna_core::idle::IdleSettings;
use libluna_core::shortcuts::{Binding, ShortcutAction, ShortcutSettings};
use libluna_core::theme_file::StyledTheme;
use libluna_core::{
    Accent, DisplaySettings, GreeterSettings, IconSet, IconSettings, InputSettings, LockSettings,
    OutputMode, Point, Rect, RegionSettings, ThemePreference, ThemeSettings, WorkspaceLayout,
};
use libluna_i18n::{set_locale, tr, tr_args, Locale};
use libluna_platform::{run as run_app, AppConfig, Frame};
use libluna_render::wallpaper;
use libluna_widgets::{
    BottomSheet, Entry, EntryIcon, NavEntry, NavList, PasswordEntry, ScrollView, Toast,
    WindowControl,
};
use luna::equalizer::{self, AudioStatus, Equalizer, Preset};
use luna::network::{self, Ipv4Method, Monitor, Security, Snapshot, WiredConfig};
use luna::notifications::{self, Placement};
use luna::system::{self, BatteryStatus};
use luna::{bluetooth, colors, default_apps, power_profile, printers, sessions, vpn};

use crate::pages::{self, Action, Page, PickerKind, SliderKind};
use crate::ui;

/// Zmiany, które trzeba zastosować do `Frame` po obsłudze zdarzeń.
#[derive(Debug, Default)]
pub struct Pending {
    pub preference: Option<ThemePreference>,
    pub accent: Option<Accent>,
    /// `Some(None)` = wróć do stylu domyślnego.
    pub style: Option<Option<String>>,
    pub close: bool,
    pub minimize: bool,
    pub maximize: bool,
    pub begin_move: bool,
}

/// Wyszukiwany wybór (język, locale, strefa).
pub struct Picker {
    pub kind: PickerKind,
    pub query: Entry,
    pub selected: usize,
    pub scroll: f64,
}

/// Modalny formularz hasła administratora (np. zapis ekranu logowania).
pub struct AdminPrompt {
    pub password: PasswordEntry,
    pub error: Option<String>,
    /// Wynik operacji z wątku tła; `None`, gdy nic nie trwa.
    pub pending: Option<mpsc::Receiver<Result<(), String>>>,
}

impl AdminPrompt {
    pub fn new() -> Self {
        let mut password = PasswordEntry::new(tr("Administrator password"));
        password.set_focused(true);
        Self {
            password,
            error: None,
            pending: None,
        }
    }

    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
}

impl Picker {
    fn new(kind: PickerKind) -> Self {
        let mut query = Entry::new(tr("Search")).leading(EntryIcon::Search);
        query.set_focused(true);
        Self {
            kind,
            query,
            selected: 0,
            scroll: 0.0,
        }
    }
}

/// Pozycja wyszukiwanego wyboru.
#[derive(Debug, Clone)]
pub struct PickerItem {
    pub label: String,
    pub value: String,
}

/// Co edytuje [`TextEditor`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPurpose {
    WorkspaceName(usize),
    /// Nazwa sieci, która nie rozgłasza SSID.
    HiddenNetwork,
    /// Ścieżka pliku profilu VPN (OpenVPN/WireGuard).
    VpnImport,
    /// Adres IPP nowej drukarki.
    PrinterUri,
    /// Ścieżka pliku profilu ICC (colord).
    ColorProfilePath,
}

/// Mały edytor tekstu (np. nazwa przestrzeni roboczej).
pub struct TextEditor {
    pub title: String,
    pub entry: Entry,
    pub purpose: EditorPurpose,
}

/// Formularz IPv4 profilu przewodowego.
pub struct WiredEditor {
    /// Ścieżka D-Bus urządzenia.
    pub device: String,
    /// Zapisany profil (jeśli już istnieje).
    pub profile: Option<String>,
    pub address: Entry,
    pub gateway: Entry,
    pub dns: Entry,
    pub autoconnect: bool,
    pub error: Option<String>,
}

/// Modalne pytanie o hasło do sieci Wi-Fi.
pub struct WifiPrompt {
    pub ssid: String,
    /// Ścieżka D-Bus urządzenia Wi-Fi.
    pub device: String,
    pub security: Security,
    pub hidden: bool,
    pub password: PasswordEntry,
    pub error: Option<String>,
    pub pending: Option<mpsc::Receiver<Result<(), String>>>,
}

impl WifiPrompt {
    fn new(ssid: String, device: String, security: Security, hidden: bool) -> Self {
        let mut password = PasswordEntry::new(tr("Wi-Fi password"));
        password.set_focused(true);
        Self {
            ssid,
            device,
            security,
            hidden,
            password,
            error: None,
            pending: None,
        }
    }

    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
}

/// Co robi formularz konta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountMode {
    /// Nowe konto: login, pełna nazwa, hasło, hasło administratora.
    AddUser,
    /// Zmiana hasła istniejącego konta (login jest znany).
    ChangePassword { login: String },
}

impl AccountMode {
    pub fn title(&self) -> &'static str {
        match self {
            AccountMode::AddUser => "Add account",
            AccountMode::ChangePassword { .. } => "Change password",
        }
    }
}

/// Pole formularza konta — Tab przełącza, klik ustawia fokus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountField {
    Login,
    FullName,
    Password,
    Confirm,
    Admin,
}

/// Formularz dodawania konta / zmiany hasła (modal z hasłem administratora).
pub struct AccountForm {
    pub mode: AccountMode,
    pub login: Entry,
    pub full_name: Entry,
    pub password: Entry,
    pub confirm: Entry,
    /// Hasło administratora — `sudo -S` zużywa je przed uruchomieniem pomocnika.
    pub admin: Entry,
    pub focus: AccountField,
    pub error: Option<String>,
    pub pending: Option<mpsc::Receiver<Result<(), String>>>,
}

impl AccountForm {
    fn new(mode: AccountMode) -> Self {
        let add = mode == AccountMode::AddUser;
        let mut login = Entry::new(tr("Login"));
        if let AccountMode::ChangePassword { login: name } = &mode {
            login.set_text(name.clone());
        }
        let mut form = Self {
            mode,
            login,
            full_name: Entry::new(tr("Full name")),
            password: Entry::new(tr("New password"))
                .masked('•')
                .leading(EntryIcon::Lock),
            confirm: Entry::new(tr("Repeat password"))
                .masked('•')
                .leading(EntryIcon::Lock),
            admin: Entry::new(tr("Administrator password"))
                .masked('•')
                .leading(EntryIcon::Lock),
            focus: if add {
                AccountField::Login
            } else {
                AccountField::Password
            },
            error: None,
            pending: None,
        };
        form.sync_focus();
        form
    }

    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Pola widoczne w tym trybie (kolejność Tab).
    pub fn fields(&self) -> Vec<AccountField> {
        match self.mode {
            AccountMode::AddUser => vec![
                AccountField::Login,
                AccountField::FullName,
                AccountField::Password,
                AccountField::Confirm,
                AccountField::Admin,
            ],
            AccountMode::ChangePassword { .. } => vec![
                AccountField::Password,
                AccountField::Confirm,
                AccountField::Admin,
            ],
        }
    }

    pub fn entry(&self, field: AccountField) -> &Entry {
        match field {
            AccountField::Login => &self.login,
            AccountField::FullName => &self.full_name,
            AccountField::Password => &self.password,
            AccountField::Confirm => &self.confirm,
            AccountField::Admin => &self.admin,
        }
    }

    pub fn entry_mut(&mut self, field: AccountField) -> &mut Entry {
        match field {
            AccountField::Login => &mut self.login,
            AccountField::FullName => &mut self.full_name,
            AccountField::Password => &mut self.password,
            AccountField::Confirm => &mut self.confirm,
            AccountField::Admin => &mut self.admin,
        }
    }

    /// Ustawia flagę fokusu tylko na aktywnym polu (Entry rysuje kursor).
    pub fn sync_focus(&mut self) {
        let focus = self.focus;
        for field in self.fields() {
            self.entry_mut(field).set_focused(field == focus);
        }
    }

    /// Przechodzi do następnego/poprzedniego pola (Tab / Shift+Tab).
    pub fn cycle_focus(&mut self, backwards: bool) {
        let fields = self.fields();
        if fields.is_empty() {
            return;
        }
        let index = fields
            .iter()
            .position(|field| *field == self.focus)
            .unwrap_or(0);
        let next = if backwards {
            (index + fields.len() - 1) % fields.len()
        } else {
            (index + 1) % fields.len()
        };
        self.focus = fields[next];
        self.sync_focus();
    }
}

/// Czego dotyczy wybór tapety.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallpaperTarget {
    /// Ekran logowania (`/etc/luna/greeter.conf`, wymaga administratora).
    Greeter,
    /// Pulpit (`~/.config/luna/wallpaper.conf`, działa od razu).
    Desktop,
}

/// Przeciąganie monitora na grafice układu.
#[derive(Debug, Clone, Copy)]
pub struct MonitorDrag {
    pub index: usize,
    /// Pozycja kursora w chwili chwycenia (współrzędne okna).
    pub grab: Point,
    /// Pozycja logiczna monitora w chwili chwycenia.
    pub origin: (i32, i32),
    /// Ile pikseli ekranu przypada na jednostkę logiczną w chwili chwycenia.
    pub scale: f64,
}

/// Źródło tapet w arkuszu „Więcej tapet”.
#[derive(Debug, Clone)]
pub struct WallpaperSource {
    /// Klucz tłumaczenia (tekst angielski).
    pub key: &'static str,
    pub path: PathBuf,
}

/// Popularne katalogi z tapetami — tylko te, które istnieją na dysku.
pub fn wallpaper_sources() -> Vec<WallpaperSource> {
    let mut out = Vec::new();
    if let Some(pictures) = pictures_dir() {
        push_wallpaper_source(&mut out, "Pictures", pictures);
    }
    push_wallpaper_source(
        &mut out,
        "System wallpapers",
        PathBuf::from("/usr/share/wallpapers"),
    );
    push_wallpaper_source(
        &mut out,
        "System wallpapers",
        PathBuf::from("/usr/share/backgrounds"),
    );
    if let Some(home) = std::env::var_os("HOME") {
        push_wallpaper_source(&mut out, "Home folder", PathBuf::from(home));
    }
    out
}

/// Katalog Obrazy użytkownika, jeśli naprawdę istnieje.
fn pictures_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_PICTURES_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join("Pictures"))
                .filter(|path| path.is_dir())
        })
}

fn push_wallpaper_source(out: &mut Vec<WallpaperSource>, key: &'static str, path: PathBuf) {
    if path.is_dir() && !out.iter().any(|source| source.path == path) {
        out.push(WallpaperSource { key, path });
    }
}

/// Wybór tapety: siatka miniatur z dysku albo lista plików.
#[derive(Debug, Clone)]
pub struct WallpaperPicker {
    pub target: WallpaperTarget,
    pub dir: PathBuf,
    /// Pozycja 0 to zawsze katalog nadrzędny (jeśli istnieje).
    pub entries: Vec<PathBuf>,
    pub selected: usize,
    pub scroll: f64,
    pub hover: Option<usize>,
    /// Tryb listy plików („Przeglądaj dysk”); domyślnie siatka miniatur.
    pub browse: bool,
    /// Arkusz „Więcej tapet” (źródła, losowa tapeta, przeglądanie dysku).
    pub more: bool,
    pub more_hover: Option<usize>,
    /// Popularne katalogi z tapetami.
    pub sources: Vec<WallpaperSource>,
    /// Tapeta sprzed otwarcia wybieraka — podgląd na żywo i „Anuluj”.
    pub original: Option<PathBuf>,
    /// Czy kliknięcie kafla zmieniło już pulpit (trzeba przywrócić przy anulowaniu).
    pub previewed: bool,
}

impl WallpaperPicker {
    fn new(target: WallpaperTarget, current: Option<PathBuf>) -> Self {
        // Startujemy w katalogu bieżącej tapety, żeby od razu było widać wybór.
        let start = current
            .as_deref()
            .and_then(Path::parent)
            .filter(|dir| dir.is_dir())
            .map(Path::to_path_buf)
            .unwrap_or_else(default_pictures_dir);
        let mut picker = Self {
            target,
            dir: start.clone(),
            entries: Vec::new(),
            selected: 0,
            scroll: 0.0,
            hover: None,
            browse: false,
            more: false,
            more_hover: None,
            sources: wallpaper_sources(),
            original: current.clone(),
            previewed: false,
        };
        picker.navigate(start);
        if let Some(current) = current {
            if let Some(index) = picker.entries.iter().position(|entry| entry == &current) {
                picker.selected = index;
            }
        }
        picker
    }

    /// Wchodzi do katalogu i odświeża listę (katalogi + PNG).
    pub fn navigate(&mut self, dir: PathBuf) {
        self.dir = dir;
        self.entries = scan_wallpaper_dir(&self.dir);
        self.selected = 0;
        self.scroll = 0.0;
        self.hover = None;
    }

    pub fn selected_path(&self) -> Option<&PathBuf> {
        self.entries.get(self.selected)
    }

    /// Losowo wybiera obraz z bieżącego katalogu (bez katalogów).
    fn surprise_pick(&mut self) -> bool {
        let images: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, path)| path.is_file())
            .map(|(index, _)| index)
            .collect();
        if images.is_empty() {
            return false;
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.subsec_nanos() as usize)
            .unwrap_or(0);
        self.selected = images[nanos % images.len()];
        true
    }

    /// Przełącza na listę plików („Przeglądaj dysk”).
    pub fn browse_disk(&mut self) {
        self.browse = true;
        self.more = false;
    }

    /// Wraca do siatki miniatur.
    pub fn show_grid(&mut self) {
        self.browse = false;
        self.more = false;
    }

    /// Co przywrócić po anulowaniu podglądu: `None` = nic nie zmieniać.
    fn restore_target(&self) -> Option<Option<PathBuf>> {
        (self.target == WallpaperTarget::Desktop && self.previewed).then(|| self.original.clone())
    }
}

/// Katalog startowy wyboru tapety.
fn default_pictures_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_PICTURES_DIR") {
        if !dir.trim().is_empty() && PathBuf::from(&dir).is_dir() {
            return PathBuf::from(dir);
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let pictures = PathBuf::from(&home).join("Pictures");
        if pictures.is_dir() {
            return pictures;
        }
        return PathBuf::from(home);
    }
    PathBuf::from("/")
}

/// Katalogi oraz pliki PNG; pierwszy wpis to katalog nadrzędny.
fn scan_wallpaper_dir(dir: &std::path::Path) -> Vec<PathBuf> {
    let parent = dir.parent().map(|parent| parent.to_path_buf());
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                dirs.push(path);
            } else if path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
            {
                files.push(path);
            }
        }
    }
    dirs.sort();
    files.sort();
    let mut out = Vec::new();
    if let Some(parent) = parent {
        out.push(parent);
    }
    out.extend(dirs);
    out.extend(files);
    out
}

/// Stan aplikacji.
pub struct SettingsApp {
    pub page: Page,
    pub nav: NavList,
    pub nav_pages: Vec<Option<Page>>,
    pub scroll: ScrollView,

    /// Stan systemu odpytywany w tle (ta sama implementacja co w pasku Luny).
    pub system: system::Shared,
    pub system_ready: bool,
    pub user: String,
    /// Tytuł okna z nazwą systemu (np. „Ustawienia — Pagan Linux”).
    pub title: String,
    pub volume: Option<(u8, bool)>,
    pub audio_sink: Option<String>,
    pub brightness: Option<u8>,
    pub battery: Option<BatteryStatus>,
    pub power_profile: Option<String>,

    pub theme_pref: ThemePreference,
    pub accent: Accent,
    pub styles: Vec<StyledTheme>,
    pub style: Option<String>,
    pub language: String,
    pub languages: Vec<(String, String)>,

    pub icons: IconSettings,
    /// Zestawy ikon freedesktop wykryte w systemie.
    pub icon_sets: Vec<IconSet>,
    pub lock: LockSettings,
    pub region: RegionSettings,
    pub workspace_layout: WorkspaceLayout,
    pub input: InputSettings,
    pub notification_placement: Placement,
    /// Locale i strefy wykryte w systemie (dla wyszukiwanego wyboru).
    pub locales: Vec<String>,
    pub timezones: Vec<String>,

    pub picker: Option<Picker>,
    pub picker_hover: Option<usize>,
    pub editor: Option<TextEditor>,
    /// Wybór tapety ekranu logowania z dysku.
    pub wallpaper_picker: Option<WallpaperPicker>,
    /// Panel techniczny („Configuration”/„Information”) otwierany przyciskiem Info.
    pub info_sheet: BottomSheet,
    pub info_scroll: ScrollView,
    /// Formularz hasła do zmian wymagających uprawnień administratora.
    pub admin: Option<AdminPrompt>,
    /// Czy ekran logowania ma niezapisane zmiany (wybory są szkicem).
    pub greeter_dirty: bool,

    pub dnd: bool,
    pub power_saver: bool,
    /// Stan synchronizacji czasu (NTP) w systemie.
    pub ntp: Option<bool>,

    /// Wykryte monitory (migawka Selene + `/sys/class/drm`).
    pub monitors: Vec<pages::Monitor>,
    /// Monitor, którego dotyczą ustawienia trybu.
    pub selected_monitor: usize,
    pub monitors_next: std::time::Instant,
    /// Aktywne przeciąganie monitora na grafice układu.
    pub monitor_drag: Option<MonitorDrag>,
    /// Preferencje wyświetlaczy (`~/.config/luna/displays.conf`).
    pub displays: DisplaySettings,
    pub night_light_dirty: bool,
    pub night_light_saved_at: std::time::Instant,

    /// Stan korektora (`~/.config/luna/equalizer.conf`).
    pub eq: Equalizer,
    /// Czy wtyczka `luna-eq` jest zainstalowana.
    pub eq_available: bool,
    /// Sprzętowe wyjście, na które trafia dźwięk po korekcji.
    pub eq_output_id: Option<u32>,
    /// Pasma zmienione, ale jeszcze nieprzesłane do PipeWire (debounce suwaków).
    pub eq_dirty: bool,
    pub eq_changed_at: std::time::Instant,
    /// Ostatni błąd uruchomienia korektora (pokazywany na stronie Dźwięk).
    pub eq_error: Option<String>,

    /// Progi wygaszania/blokady/usypiania (`~/.config/luna/idle.conf`).
    pub idle: IdleSettings,
    /// Lokalne konta (`/etc/passwd`) — lista na stronie Użytkownicy.
    pub accounts: Vec<UserAccount>,
    /// Otwarty formularz dodawania konta / zmiany hasła.
    pub account_form: Option<AccountForm>,

    /// Ostatnia migawka urządzeń audio (`wpctl status`).
    pub audio: AudioStatus,
    pub audio_next: std::time::Instant,

    /// Ostatni skan Wi-Fi (lista sieci) i stan adaptera.
    pub network: Snapshot,
    pub network_monitor: Monitor,
    pub network_next: std::time::Instant,
    pub wifi_prompt: Option<WifiPrompt>,
    /// Sieć łączona bez pytania o hasło (zapisany profil). Gdy aktywacja odrzuci
    /// zapisane sekrety, pokazujemy formularz hasła dla tego samego SSID.
    pub wifi_retry: Option<(String, String, Security, bool)>,
    /// Formularz IPv4 połączenia przewodowego.
    pub wired_editor: Option<WiredEditor>,
    /// Operacja sieciowa w tle (połączenie/rozłączenie).
    pub network_pending: Option<mpsc::Receiver<Result<(), String>>>,

    // — Nowe moduły: Bluetooth, profile zasilania, VPN, drukarki, sesje, colord —
    /// Ostatnia migawka BlueZ (adaptery i urządzenia).
    pub bluetooth: bluetooth::Snapshot,
    /// Migawka Bluetooth liczona w tle (skanowanie może trwać).
    pub bluetooth_snapshot: Option<mpsc::Receiver<bluetooth::Snapshot>>,
    pub bluetooth_next: std::time::Instant,
    /// Profile zasilania z `power-profiles-daemon`.
    pub power_profiles: power_profile::Profiles,
    /// Profile VPN z NetworkManagera.
    pub vpn: vpn::Snapshot,
    /// Drukarki CUPS.
    pub printers: printers::Snapshot,
    /// Sesje logind (przełączanie użytkownika).
    pub sessions: sessions::Snapshot,
    /// Urządzenia i profile kolorów colord.
    pub colors: colors::Snapshot,
    /// Urządzenie colord, dla którego otwarto wybór profilu.
    pub color_target: Option<String>,
    /// Ustawienia dostępności (`~/.config/luna/accessibility.conf`).
    pub accessibility: libluna_core::AccessibilitySettings,
    /// Domyślne aplikacje (`xdg-mime`, wpisy `.desktop`).
    pub defaults: default_apps::Defaults,
    /// Kategoria, dla której otwarto wybór domyślnej aplikacji.
    pub default_app_target: Option<usize>,
    /// Skróty klawiszowe (`~/.config/luna/shortcuts.conf`).
    pub shortcuts: ShortcutSettings,
    /// Akcja, dla której trwa nagrywanie kombinacji.
    pub shortcut_record: Option<ShortcutAction>,
    /// Przewinięcie listy stron w sidebarze.
    pub nav_scroll: f64,
    /// Po zmianie strony przewiń sidebar do wybranej pozycji.
    pub nav_reveal: bool,
    /// Licznik szybkich kliknięć w wersję na stronie „O Lunie”.
    pub about_clicks: u8,
    pub about_clicked_at: Option<std::time::Instant>,
    /// Czy easter egg został odkryty (per sesja).
    pub easter_egg: bool,
    /// Operacje modułów wykonujące się w tle (z odświeżeniem po sukcesie).
    pub action_tasks: Vec<(PendingKind, mpsc::Receiver<Result<(), String>>)>,
    /// Motyw trzeba rozwiązać ponownie (zmiana dostępności).
    pub theme_reload: bool,
    /// Gdy `true`, ["go_to"] nie odpytuje usług systemowych (podglądy, testy).
    pub demo: bool,

    /// Ustawienia ekranu logowania (`/etc/luna/greeter.conf`).
    pub greeter: GreeterSettings,
    /// Wykryte tapety PNG proponowane dla ekranu logowania.
    pub wallpapers: Vec<PathBuf>,

    pub status: Toast,
    pub status_until: Option<std::time::Instant>,
    pub pending: Pending,
    pub hover_control: Option<WindowControl>,
    /// Ostatnia pozycja kursora — do podświetlania wierszy i kontrolek.
    pub pointer: Option<Point>,
    /// Kursor nad przyciskiem Info w nagłówku.
    pub info_hover: bool,
    /// Opis działającej usługi powiadomień (albo „Nie działa”).
    pub notification_daemon: String,
    /// Kiedy następnym razem sprawdzić usługę powiadomień.
    pub notification_check_at: std::time::Instant,
    /// Aktywny suwak: rodzaj i prostokąt toru (do przeciągania).
    pub drag: Option<(SliderKind, Rect)>,
    /// Tekst do systemowego schowka (odbierany przez pętlę klatki).
    pub clipboard_text: Option<String>,
    /// Prośba o odczyt schowka; wynik wróci jako `Event::Paste`.
    pub paste_requested: bool,
    /// Pole, do którego trafi wklejony tekst.
    pub paste_target: Option<TextPasteTarget>,
}

/// Pole tekstowe obsługujące systemowy schowek.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextPasteTarget {
    PickerQuery,
    EditorEntry,
    AdminPassword,
    WifiPassword,
    /// Pole formularza IPv4 (0 = adres, 1 = brama, 2 = DNS).
    WiredEntry(usize),
    /// Pole formularza konta (login, hasło, hasło administratora…).
    AccountEntry(AccountField),
}

/// Moduł, którego operacja zakończyła się w tle (decyduje o odświeżeniu).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingKind {
    Bluetooth,
    Power,
    Vpn,
    Printer,
    Color,
    Session,
    DefaultApps,
}

impl PendingKind {
    fn success_key(self) -> &'static str {
        match self {
            PendingKind::Bluetooth => "Bluetooth updated",
            PendingKind::Power => "Power profile changed",
            PendingKind::Vpn => "VPN updated",
            PendingKind::Printer => "Printer updated",
            PendingKind::Color => "Color profile updated",
            PendingKind::Session => "Switching session…",
            PendingKind::DefaultApps => "Default application changed",
        }
    }
}

impl SettingsApp {
    pub fn new(settings: &ThemeSettings) -> Self {
        let language = libluna_i18n::locale().language().to_owned();
        let mut languages: Vec<(String, String)> = libluna_i18n::languages()
            .into_iter()
            .map(|code| {
                let name = libluna_i18n::language_name(&code).unwrap_or_else(|| code.clone());
                (code, name)
            })
            .collect();
        languages.sort_by_cached_key(|(_, name)| name.to_lowercase());

        // Wybrany wcześniej zestaw ikon ma obowiązywać także aplikacje GTK/Qt/KDE;
        // ponawiamy zapis przy starcie, gdyby inny program nadpisał te pliki.
        let mut icons = IconSettings::load();
        libluna_core::icons::set_active_icon_theme(icons.theme);
        // Domyślnie używamy dołączonego pakietu Luna-Icons. Zapamiętujemy
        // poprzedni motyw GTK, żeby powrót do ikon wektorowych mógł go przywrócić.
        if icons.external.is_some() && icons.previous_gtk_theme.is_none() {
            icons.previous_gtk_theme = libluna_core::icons::gtk_icon_theme()
                .filter(|name| Some(name.as_str()) != icons.external.as_deref())
                .or_else(|| Some("Adwaita".to_owned()));
        }

        let mut app = Self {
            page: Page::default(),
            nav: NavList::new(Vec::new()),
            nav_pages: Vec::new(),
            scroll: ScrollView::new(),
            system: system::spawn_light(),
            system_ready: false,
            user: std::env::var("USER")
                .or_else(|_| std::env::var("LOGNAME"))
                .unwrap_or_else(|_| "?".to_owned()),
            title: app_title(),
            volume: system::read_volume(),
            audio_sink: None,
            brightness: system::read_brightness(),
            battery: None,
            power_profile: None,
            theme_pref: settings.preference(),
            accent: settings.accent(),
            styles: libluna_core::theme_file::available(),
            style: settings.style().map(str::to_owned),
            language,
            languages,
            icons,
            icon_sets: libluna_core::icons::external_icon_sets(),
            lock: LockSettings::load(),
            region: RegionSettings::load(),
            workspace_layout: WorkspaceLayout::load(),
            input: InputSettings::load(),
            notification_placement: notifications::load_placement(),
            locales: libluna_core::region::system_locales(),
            timezones: libluna_core::region::system_timezones(),
            picker: None,
            picker_hover: None,
            editor: None,
            wallpaper_picker: None,
            info_sheet: BottomSheet::new(),
            info_scroll: ScrollView::new(),
            admin: None,
            greeter_dirty: false,
            dnd: system::dnd(),
            power_saver: system::power_saver(),
            ntp: system::ntp_enabled(),
            monitors: pages::monitors(),
            selected_monitor: 0,
            monitors_next: std::time::Instant::now(),
            monitor_drag: None,
            displays: DisplaySettings::load(),
            night_light_dirty: false,
            night_light_saved_at: std::time::Instant::now(),
            eq: Equalizer::load(),
            eq_available: equalizer::available(),
            eq_output_id: None,
            eq_dirty: false,
            eq_changed_at: std::time::Instant::now(),
            eq_error: None,
            idle: IdleSettings::load(),
            accounts: libluna_core::accounts::local_users(),
            account_form: None,
            audio: AudioStatus::default(),
            audio_next: std::time::Instant::now(),
            network: Snapshot::default(),
            network_monitor: Monitor::spawn(),
            network_next: std::time::Instant::now(),
            wifi_prompt: None,
            wifi_retry: None,
            wired_editor: None,
            network_pending: None,
            bluetooth: bluetooth::Snapshot::default(),
            bluetooth_snapshot: None,
            bluetooth_next: std::time::Instant::now(),
            power_profiles: power_profile::Profiles::default(),
            vpn: vpn::Snapshot::default(),
            printers: printers::Snapshot::default(),
            sessions: sessions::Snapshot::default(),
            colors: colors::Snapshot::default(),
            color_target: None,
            accessibility: libluna_core::AccessibilitySettings::load(),
            defaults: default_apps::Defaults::default(),
            default_app_target: None,
            shortcuts: ShortcutSettings::load(),
            shortcut_record: None,
            nav_scroll: 0.0,
            nav_reveal: false,
            about_clicks: 0,
            about_clicked_at: None,
            easter_egg: false,
            action_tasks: Vec::new(),
            theme_reload: false,
            demo: false,
            greeter: GreeterSettings::load(),
            wallpapers: pages::wallpapers(),
            status: Toast::default(),
            status_until: None,
            pending: Pending::default(),
            hover_control: None,
            pointer: None,
            info_hover: false,
            notification_daemon: String::new(),
            notification_check_at: std::time::Instant::now(),
            drag: None,
            clipboard_text: None,
            paste_requested: false,
            paste_target: None,
        };
        app.rebuild_nav();
        app
    }

    /// Pokazuje krótki komunikat nad treścią (bez animacji przy „Reduce motion”).
    pub fn show_status(&mut self, message: impl Into<String>) {
        self.status = Toast::new(message);
        if self.accessibility.reduce_motion {
            self.status.show();
        } else {
            self.status.show_animated();
        }
        self.status_until = Some(std::time::Instant::now() + Duration::from_secs(3));
    }

    /// Odbudowuje etykiety sidebaru (np. po zmianie języka), zachowując stronę.
    pub fn rebuild_nav(&mut self) {
        let (mut nav, pages) = nav_entries();
        let selected = pages
            .iter()
            .position(|p| *p == Some(self.page))
            .unwrap_or(0);
        nav.set_selected(Some(selected));
        self.nav = nav;
        self.nav_pages = pages;
    }

    /// Wybiera stronę i wraca na górę przewijania.
    pub fn go_to(&mut self, page: Page) {
        if self.page != page {
            self.page = page;
            self.nav_reveal = true;
            self.scroll = ScrollView::new();
            self.info_scroll = ScrollView::new();
            let selected = self.nav_pages.iter().position(|p| *p == Some(page));
            self.nav.set_selected(selected);
            if page == Page::Network {
                self.network_monitor.scan();
                self.network = self.network_monitor.current();
                self.network_next = std::time::Instant::now() + Duration::from_secs(3);
            }
            if page == Page::Notifications {
                self.notification_check_at = std::time::Instant::now();
            }
            if page == Page::Displays {
                self.monitors = pages::monitors();
                self.selected_monitor = self
                    .selected_monitor
                    .min(self.monitors.len().saturating_sub(1));
                self.monitors_next = std::time::Instant::now() + Duration::from_secs(3);
            }
            self.refresh_page(page);
        }
    }

    /// Wczytuje dane strony z usług systemowych (poza podglądami).
    fn refresh_page(&mut self, page: Page) {
        if self.demo {
            return;
        }
        match page {
            Page::Bluetooth => self.refresh_bluetooth(),
            Page::Power => self.power_profiles = power_profile::snapshot(),
            Page::Network => self.vpn = vpn::snapshot(),
            Page::Printers => self.printers = printers::snapshot(),
            Page::Users => self.sessions = sessions::snapshot(),
            Page::Displays => self.colors = colors::snapshot(),
            Page::Accessibility => self.accessibility = libluna_core::AccessibilitySettings::load(),
            Page::DefaultApps => self.defaults = default_apps::snapshot(),
            Page::Shortcuts => self.shortcuts = ShortcutSettings::load(),
            _ => {}
        }
    }

    /// Odświeża migawkę BlueZ w tle (D-Bus może czekać na skanowanie).
    pub fn refresh_bluetooth(&mut self) {
        if self.demo || self.bluetooth_snapshot.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(bluetooth::snapshot());
        });
        self.bluetooth_snapshot = Some(rx);
    }

    /// Odbiera migawkę Bluetooth z wątku tła.
    pub fn poll_bluetooth(&mut self) {
        let Some(rx) = self.bluetooth_snapshot.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(snapshot) => {
                self.bluetooth = snapshot;
                self.bluetooth_snapshot = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => self.bluetooth_snapshot = None,
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    /// Uruchamia operację modułu w tle i zapamiętuje, co odświeżyć po sukcesie.
    pub fn run_task<F>(&mut self, kind: PendingKind, operation: F)
    where
        F: FnOnce() -> Result<(), String> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(operation());
        });
        self.action_tasks.push((kind, rx));
    }

    /// Odbiera wyniki operacji w tle (odświeża dane i pokazuje komunikat).
    pub fn poll_action_tasks(&mut self) {
        let mut done = Vec::new();
        for (index, (kind, rx)) in self.action_tasks.iter().enumerate() {
            match rx.try_recv() {
                Ok(result) => done.push((index, *kind, result)),
                Err(mpsc::TryRecvError::Disconnected) => done.push((
                    index,
                    *kind,
                    Err(tr("The operation was interrupted").to_owned()),
                )),
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        for (index, kind, result) in done.into_iter().rev() {
            self.action_tasks.remove(index);
            match result {
                Ok(()) => {
                    self.refresh_kind(kind);
                    self.show_status(tr(kind.success_key()));
                }
                Err(err) => {
                    // Błąd też odświeża stan — np. po nieudanym `xdg-mime` wracamy
                    // do faktycznego skojarzenia zamiast optymistycznego wyboru.
                    self.refresh_kind(kind);
                    self.show_status(err);
                }
            }
        }
    }

    fn refresh_kind(&mut self, kind: PendingKind) {
        if self.demo {
            return;
        }
        match kind {
            PendingKind::Bluetooth => self.refresh_bluetooth(),
            PendingKind::Power => self.power_profiles = power_profile::snapshot(),
            PendingKind::Vpn => self.vpn = vpn::snapshot(),
            PendingKind::Printer => self.printers = printers::snapshot(),
            PendingKind::Color => self.colors = colors::snapshot(),
            PendingKind::Session => self.sessions = sessions::snapshot(),
            PendingKind::DefaultApps => self.defaults = default_apps::snapshot(),
        }
    }

    /// Otwiera panel Info z grupami technicznymi bieżącej strony.
    pub fn open_info_sheet(&mut self) {
        self.info_scroll = ScrollView::new();
        self.info_sheet
            .set_title(Some(tr("Configuration and information")));
        self.info_sheet.set_open_animated(true);
    }

    pub fn close_info_sheet(&mut self) {
        self.info_sheet.set_open_animated(false);
    }

    /// Otwiera wybór tapety z dysku (ekran logowania).
    pub fn open_wallpaper_picker(&mut self) {
        let current = self
            .greeter
            .wallpaper
            .clone()
            .or_else(|| wallpaper::configured_path());
        self.wallpaper_picker = Some(WallpaperPicker::new(WallpaperTarget::Greeter, current));
    }

    pub fn open_desktop_wallpaper_picker(&mut self) {
        self.wallpaper_picker = Some(WallpaperPicker::new(
            WallpaperTarget::Desktop,
            wallpaper::configured_path(),
        ));
    }

    /// „Zaskocz mnie”: losowa tapeta z bieżącego katalogu, od razu zastosowana.
    pub fn surprise_wallpaper(&mut self) {
        let picked = match self.wallpaper_picker.as_mut() {
            Some(picker) => {
                if picker.surprise_pick() {
                    picker.more = false;
                    true
                } else {
                    false
                }
            }
            None => false,
        };
        if !picked {
            self.show_status(tr("No wallpapers found"));
            return;
        }
        self.choose_wallpaper();
    }

    /// Ustawia tapetę pulpitu i odświeża Selene (mtime pliku zmienia sygnaturę klatki).
    pub fn set_desktop_wallpaper(&mut self, path: PathBuf) {
        if !path.is_file() {
            self.show_status(tr("The selected file does not exist"));
            return;
        }
        match wallpaper::write_config(&path) {
            Ok(()) => self.show_status(tr("Wallpaper updated")),
            Err(err) => self.show_status(format!("{}: {err}", tr("Could not set the wallpaper"))),
        }
    }

    /// Wraca do wbudowanej tapety (usuwa `wallpaper.conf`).
    pub fn clear_desktop_wallpaper(&mut self) {
        match wallpaper::clear_config() {
            Ok(()) => self.show_status(tr("Built-in wallpaper restored")),
            Err(err) => self.show_status(format!("{}: {err}", tr("Could not set the wallpaper"))),
        }
    }

    /// Odświeża dane z wątku systemowego i plików powłoki.
    pub fn sync_system(&mut self) {
        if let Ok(sys) = self.system.lock() {
            self.system_ready = true;
            if !sys.user.is_empty() {
                self.user = sys.user.clone();
            }
            if !matches!(self.drag.map(|(kind, _)| kind), Some(SliderKind::Volume))
                && sys.volume.is_some()
            {
                self.volume = sys.volume;
            }
            self.audio_sink = sys.audio_sink.clone();
            if !matches!(
                self.drag.map(|(kind, _)| kind),
                Some(SliderKind::Brightness)
            ) {
                self.brightness = sys.brightness;
            }
            self.battery = sys.battery;
            self.power_profile = sys.power_profile.clone();
        }
        // Zapisywanie night lightu tylko po ustabilizowaniu suwaka (kilka zdarzeń na klatkę).
        if self.night_light_dirty
            && self.night_light_saved_at.elapsed() >= Duration::from_millis(400)
        {
            self.night_light_dirty = false;
            self.night_light_saved_at = std::time::Instant::now();
            let _ = self.displays.save();
        }
        if self.page == Page::Displays
            && self.monitor_drag.is_none()
            && std::time::Instant::now() >= self.monitors_next
        {
            self.monitors = pages::monitors();
            self.selected_monitor = self
                .selected_monitor
                .min(self.monitors.len().saturating_sub(1));
            self.monitors_next = std::time::Instant::now() + Duration::from_secs(3);
        }

        // Urządzenia audio odświeżamy tylko na stronie Dźwięk (każde `wpctl` to proces).
        if self.page == Page::Sound && std::time::Instant::now() >= self.audio_next {
            self.refresh_audio();
        }
        // Zmiany pasm wysyłamy dopiero, gdy suwak się zatrzyma — każde przesłanie
        // restartuje łańcuch filtrów (krótka przerwa w dźwięku).
        if self.eq_dirty
            && self.drag.is_none()
            && self.eq_changed_at.elapsed() >= Duration::from_millis(700)
        {
            self.eq_dirty = false;
            match equalizer::save_and_apply(&self.eq) {
                Ok(()) => {
                    self.eq_error = None;
                    self.refresh_audio();
                }
                Err(err) => self.eq_error = Some(err),
            }
        }
        self.dnd = system::dnd();
        self.power_saver = system::power_saver();
        if self.page == Page::Notifications
            && std::time::Instant::now() >= self.notification_check_at
        {
            self.notification_check_at = std::time::Instant::now() + Duration::from_secs(2);
            self.notification_daemon = match crate::notify::server_status() {
                Ok(info) => info,
                Err(err) => format!("{} ({err})", tr("Not running")),
            };
        }
        if self.page == Page::Network {
            if std::time::Instant::now() >= self.network_next {
                self.network_monitor.refresh();
                self.network_next = std::time::Instant::now() + Duration::from_secs(3);
            }
            self.network = self.network_monitor.current();
        }
    }

    /// Otwiera wyszukiwany wybór dla języka, locale albo strefy.
    pub fn open_picker(&mut self, kind: PickerKind) {
        let mut picker = Picker::new(kind);
        picker.selected = self.picker_current_index(kind, &picker);
        self.picker = Some(picker);
        self.picker_hover = None;
    }

    /// Wszystkie pozycje wybranego rodzaju (przed filtrowaniem).
    pub fn picker_items(&self, kind: PickerKind) -> Vec<PickerItem> {
        match kind {
            PickerKind::Language => self
                .languages
                .iter()
                .map(|(code, name)| PickerItem {
                    label: format!("{name} ({code})"),
                    value: code.clone(),
                })
                .collect(),
            PickerKind::Locale => {
                let system = libluna_core::region::current_locale();
                let mut items = vec![PickerItem {
                    label: format!("{}: {system}", tr("System")),
                    value: String::new(),
                }];
                items.extend(self.locales.iter().map(|locale| PickerItem {
                    label: locale.clone(),
                    value: locale.clone(),
                }));
                items
            }
            PickerKind::Timezone => {
                let system = libluna_core::region::current_timezone();
                let mut items = vec![PickerItem {
                    label: format!("{}: {system}", tr("System")),
                    value: String::new(),
                }];
                items.extend(self.timezones.iter().map(|zone| PickerItem {
                    label: zone.clone(),
                    value: zone.clone(),
                }));
                items
            }
            PickerKind::IconSet => {
                let mut items = vec![PickerItem {
                    label: tr("Vector icons (Luna)"),
                    value: String::new(),
                }];
                items.extend(self.icon_sets.iter().map(|set| {
                    let display = libluna_core::icons::icon_set_display_name(set);
                    let label = if display == set.name {
                        set.name.clone()
                    } else {
                        format!("{} ({})", display, set.name)
                    };
                    PickerItem {
                        label,
                        value: set.name.clone(),
                    }
                }));
                items
            }
            PickerKind::ColorProfile => self
                .colors
                .profiles
                .iter()
                .filter(|profile| profile.kind.as_deref() != Some("printer"))
                .map(|profile| PickerItem {
                    label: profile.title.clone(),
                    value: profile.id.clone(),
                })
                .collect(),
            PickerKind::DefaultApp => {
                let Some(index) = self.default_app_target else {
                    return Vec::new();
                };
                self.defaults
                    .apps_for(index)
                    .into_iter()
                    .map(|app| PickerItem {
                        label: app.name.clone(),
                        value: app.desktop_id.clone(),
                    })
                    .collect()
            }
            PickerKind::Resolution => {
                let Some(monitor) = self.selected_monitor() else {
                    return Vec::new();
                };
                let mut items = vec![PickerItem {
                    label: tr("Auto (native)"),
                    value: "auto".to_owned(),
                }];
                items.extend(
                    monitor
                        .resolutions()
                        .into_iter()
                        .map(|(width, height)| PickerItem {
                            label: format!("{width} × {height}"),
                            value: format!("{width}x{height}"),
                        }),
                );
                items
            }
            PickerKind::Refresh => {
                let Some(monitor) = self.selected_monitor() else {
                    return Vec::new();
                };
                let Some(resolution) = self.selected_resolution() else {
                    return Vec::new();
                };
                let mut items = vec![PickerItem {
                    label: tr("Auto"),
                    value: "auto".to_owned(),
                }];
                items.extend(
                    monitor
                        .refresh_rates(resolution)
                        .into_iter()
                        .map(|refresh| PickerItem {
                            label: format!("{refresh} Hz"),
                            value: refresh.to_string(),
                        }),
                );
                items
            }
        }
    }

    /// Pozycje przefiltrowane frazą z pola wyszukiwania.
    pub fn picker_filtered(&self) -> Vec<PickerItem> {
        let Some(picker) = &self.picker else {
            return Vec::new();
        };
        let query = picker.query.text().trim().to_lowercase();
        self.picker_items(picker.kind)
            .into_iter()
            .filter(|item| {
                query.is_empty()
                    || item.label.to_lowercase().contains(&query)
                    || item.value.to_lowercase().contains(&query)
            })
            .collect()
    }

    fn picker_current_index(&self, kind: PickerKind, picker: &Picker) -> usize {
        let query = picker.query.text().to_lowercase();
        let current = self.picker_current_value(kind);
        self.picker_items(kind)
            .into_iter()
            .filter(|item| {
                query.is_empty()
                    || item.label.to_lowercase().contains(&query)
                    || item.value.to_lowercase().contains(&query)
            })
            .position(|item| item.value == current)
            .unwrap_or(0)
    }

    /// Bieżąca wartość pickera (do zaznaczenia wiersza).
    pub fn picker_current_value(&self, kind: PickerKind) -> String {
        match kind {
            PickerKind::Language => self.language.clone(),
            PickerKind::Locale => self.region.locale.clone().unwrap_or_default(),
            PickerKind::Timezone => self.region.timezone.clone().unwrap_or_default(),
            PickerKind::IconSet => self.icons.external.clone().unwrap_or_default(),
            PickerKind::ColorProfile => {
                let Some(device) = self.color_target.as_ref() else {
                    return String::new();
                };
                self.colors
                    .devices
                    .iter()
                    .find(|device_info| &device_info.id == device)
                    .and_then(|device_info| {
                        self.colors
                            .profiles
                            .iter()
                            .find(|profile| device_info.profiles.contains(&profile.id))
                    })
                    .map(|profile| profile.id.clone())
                    .unwrap_or_default()
            }
            PickerKind::DefaultApp => self
                .default_app_target
                .and_then(|index| self.defaults.current.get(index).cloned().flatten())
                .unwrap_or_default(),
            PickerKind::Resolution => self
                .selected_resolution()
                .map(|(width, height)| format!("{width}x{height}"))
                .unwrap_or_else(|| "auto".to_owned()),
            PickerKind::Refresh => self
                .selected_monitor()
                .and_then(|monitor| {
                    self.displays
                        .mode_for(&monitor.name)
                        .or_else(|| monitor.current_mode())
                })
                .map(|mode| mode.refresh.to_string())
                .unwrap_or_else(|| "auto".to_owned()),
        }
    }

    /// Zatwierdza aktualną pozycję pickera.
    pub fn apply_picker(&mut self) {
        let Some(picker) = self.picker.take() else {
            return;
        };
        let items = self.picker_filtered_for(&picker);
        let Some(item) = items.get(picker.selected.clamp(0, items.len().saturating_sub(1))) else {
            return;
        };
        match picker.kind {
            PickerKind::Language => {
                if let Some(index) = self
                    .languages
                    .iter()
                    .position(|(code, _)| *code == item.value)
                {
                    self.on_action(Action::SetLanguage(index));
                }
            }
            PickerKind::Locale => {
                self.region.locale = (!item.value.is_empty()).then(|| item.value.clone());
                self.save_region();
            }
            PickerKind::Timezone => {
                self.region.timezone = (!item.value.is_empty()).then(|| item.value.clone());
                self.save_region();
            }
            PickerKind::IconSet => {
                self.on_action(Action::SetExternalIconSet(
                    (!item.value.is_empty()).then(|| item.value.clone()),
                ));
            }
            PickerKind::Resolution => self.set_monitor_resolution(&item.value),
            PickerKind::Refresh => self.set_monitor_refresh(&item.value),
            PickerKind::ColorProfile => self.on_action(Action::SetColorProfile(item.value.clone())),
            PickerKind::DefaultApp => self.on_action(Action::SetDefaultApp(item.value.clone())),
        }
    }

    fn picker_filtered_for(&self, picker: &Picker) -> Vec<PickerItem> {
        let query = picker.query.text().trim().to_lowercase();
        self.picker_items(picker.kind)
            .into_iter()
            .filter(|item| {
                query.is_empty()
                    || item.label.to_lowercase().contains(&query)
                    || item.value.to_lowercase().contains(&query)
            })
            .collect()
    }

    /// Otwiera edytor nazwy przestrzeni.
    pub fn open_workspace_editor(&mut self, index: usize) {
        if index >= self.workspace_layout.count() {
            return;
        }
        let mut entry = Entry::new(tr("Workspace name"));
        entry.set_text(self.workspace_layout.name(index));
        entry.set_focused(true);
        self.editor = Some(TextEditor {
            title: tr_args("Workspace {n}", &[("n", &(index + 1).to_string())]),
            entry,
            purpose: EditorPurpose::WorkspaceName(index),
        });
    }

    /// Otwiera formularz nazwy sieci ukrytej (bez rozgłaszania SSID).
    pub fn open_hidden_network(&mut self) {
        let mut entry = Entry::new(tr("Network name (SSID)"));
        entry.set_focused(true);
        self.editor = Some(TextEditor {
            title: tr("Hidden network"),
            entry,
            purpose: EditorPurpose::HiddenNetwork,
        });
    }

    /// Otwiera pole ścieżki do importu profilu VPN.
    pub fn open_vpn_import(&mut self) {
        let mut entry = Entry::new(tr("Path to the VPN profile"));
        entry.set_focused(true);
        self.editor = Some(TextEditor {
            title: tr("Import VPN profile"),
            entry,
            purpose: EditorPurpose::VpnImport,
        });
    }

    /// Otwiera pole adresu nowej drukarki (IPP/driverless).
    pub fn open_printer_add(&mut self) {
        let mut entry = Entry::new(tr("Printer address (IPP)"));
        entry.set_focused(true);
        self.editor = Some(TextEditor {
            title: tr("Add printer"),
            entry,
            purpose: EditorPurpose::PrinterUri,
        });
    }

    /// Otwiera pole ścieżki pliku ICC.
    pub fn open_color_import(&mut self) {
        let mut entry = Entry::new(tr("Path to the ICC profile"));
        entry.set_focused(true);
        self.editor = Some(TextEditor {
            title: tr("Import color profile"),
            entry,
            purpose: EditorPurpose::ColorProfilePath,
        });
    }

    /// Zapisuje aktywny edytor tekstowy (nazwa przestrzeni albo ukryta sieć).
    pub fn save_editor(&mut self) {
        let Some(editor) = self.editor.take() else {
            return;
        };
        let value = editor.entry.text().trim().to_owned();
        match editor.purpose {
            EditorPurpose::WorkspaceName(index) => self.save_workspace_name(index, &value),
            EditorPurpose::HiddenNetwork => {
                if value.is_empty() {
                    return;
                }
                let Some(device) = self.network.wifi_devices().next().map(|d| d.path.clone())
                else {
                    self.show_status(tr("No Wi-Fi adapter found"));
                    return;
                };
                // Bez skanu nie znamy zabezpieczenia — zakładamy WPA2 i pytamy o hasło.
                self.wifi_retry = None;
                self.wifi_prompt = Some(WifiPrompt::new(value, device, Security::Wpa2, true));
            }
            EditorPurpose::VpnImport => {
                if value.is_empty() {
                    return;
                }
                self.run_task(PendingKind::Vpn, move || vpn::import(&value));
                self.show_status(tr("Importing the VPN profile…"));
            }
            EditorPurpose::PrinterUri => {
                if value.is_empty() {
                    return;
                }
                if let Err(err) = printers::validate_uri(&value) {
                    self.show_status(err);
                    return;
                }
                let Some(name) = printer_name_from_uri(&value) else {
                    self.show_status(tr("Could not derive a printer name from the address"));
                    return;
                };
                self.run_task(PendingKind::Printer, move || {
                    printers::add(&name, &value, "everywhere")
                });
                self.show_status(tr("Adding the printer…"));
            }
            EditorPurpose::ColorProfilePath => {
                if value.is_empty() {
                    return;
                }
                self.run_task(PendingKind::Color, move || colors::import(&value));
                self.show_status(tr("Importing the color profile…"));
            }
        }
    }

    /// Zapisuje nazwę przestrzeni i od razu wysyła ją do Selene.
    fn save_workspace_name(&mut self, index: usize, value: &str) {
        self.workspace_layout
            .set_name(index, (!value.is_empty()).then_some(value));
        if let Err(err) = self.workspace_layout.save() {
            self.show_status(format!("{}: {err}", tr("Could not save workspaces")));
            return;
        }
        let name = one_line(&self.workspace_layout.name(index));
        control_send(&format!("workspace-name {index} {name}"));
        self.show_status(tr("Workspace renamed"));
    }

    /// Otwiera formularz IPv4 dla urządzenia przewodowego.
    pub fn open_wired_editor(&mut self, device: String) {
        let Some(info) = self.network.devices.iter().find(|info| info.path == device) else {
            return;
        };
        let config = info.wired.clone().unwrap_or_default();
        let mut address = Entry::new(tr("192.168.1.20/24"));
        address.set_text(&config.address);
        address.set_focused(true);
        let mut gateway = Entry::new(tr("192.168.1.1"));
        gateway.set_text(&config.gateway);
        let mut dns = Entry::new(tr("1.1.1.1, 8.8.8.8"));
        dns.set_text(config.dns.join(", "));
        self.wired_editor = Some(WiredEditor {
            device,
            profile: config.profile,
            address,
            gateway,
            dns,
            autoconnect: config.autoconnect,
            error: None,
        });
    }

    /// Waliduje i zapisuje IPv4 profilu przewodowego (puste pole adresu = DHCP).
    pub fn save_wired_editor(&mut self) {
        let Some(editor) = self.wired_editor.take() else {
            return;
        };
        let address = editor.address.text().trim().to_owned();
        let config = WiredConfig {
            method: if address.is_empty() {
                Ipv4Method::Auto
            } else {
                Ipv4Method::Manual
            },
            address,
            gateway: editor.gateway.text().trim().to_owned(),
            dns: editor
                .dns
                .text()
                .split([',', ' '])
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(str::to_owned)
                .collect(),
            profile: editor.profile.clone(),
            autoconnect: editor.autoconnect,
        };
        if let Err(err) = config.validate() {
            let mut editor = editor;
            editor.error = Some(err.to_string());
            self.wired_editor = Some(editor);
            return;
        }
        let device = editor.device.clone();
        let rx = spawn_network(move || network::set_wired_ipv4(&device, &config));
        self.network_pending = Some(rx);
        self.show_status(tr("Saving IPv4 configuration…"));
    }

    fn save_region(&mut self) {
        match self.region.save() {
            Ok(()) => self.show_status(tr("Region settings saved")),
            Err(err) => self.show_status(format!("{}: {err}", tr("Could not save region"))),
        }
    }

    /// Zapisuje ustawienia wejścia i przekazuje je działającemu Selene.
    fn save_input(&mut self) {
        match self.input.save() {
            Ok(()) => {
                let repeat = control_send(&format!(
                    "input repeat {} {}",
                    self.input.repeat_delay(),
                    self.input.repeat_rate()
                ));
                let scroll = control_send(&format!(
                    "input natural-scroll {}",
                    u8::from(self.input.natural_scroll())
                ));
                self.show_status(if repeat && scroll {
                    tr("Input settings applied")
                } else {
                    tr("Input settings saved — restart Selene to apply")
                });
            }
            Err(err) => self.show_status(format!("{}: {err}", tr("Could not save input settings"))),
        }
    }

    /// Zatwierdza wybór z przeglądarki tapet (katalog = wejście, plik = wybór).
    pub fn choose_wallpaper(&mut self) {
        let Some(path) = self
            .wallpaper_picker
            .as_ref()
            .and_then(WallpaperPicker::selected_path)
            .cloned()
        else {
            return;
        };
        if path.is_dir() {
            if let Some(picker) = &mut self.wallpaper_picker {
                picker.navigate(path);
            }
            return;
        }
        let target = self
            .wallpaper_picker
            .as_ref()
            .map(|picker| picker.target)
            .unwrap_or(WallpaperTarget::Greeter);
        self.wallpaper_picker = None;
        match target {
            WallpaperTarget::Greeter => {
                self.greeter.wallpaper = Some(path);
                self.greeter_dirty = true;
            }
            WallpaperTarget::Desktop => self.set_desktop_wallpaper(path),
        }
    }

    /// Monitor, którego dotyczą ustawienia trybu.
    pub fn selected_monitor(&self) -> Option<&pages::Monitor> {
        self.monitors.get(self.selected_monitor)
    }

    /// Rozdzielczość wybrana w pickerze — z zapisu albo bieżąca.
    fn selected_resolution(&self) -> Option<(u16, u16)> {
        let monitor = self.selected_monitor()?;
        self.displays
            .mode_for(&monitor.name)
            .or_else(|| monitor.current_mode())
            .map(|mode| mode.resolution())
    }

    /// Zapisuje tryb monitora (wybranego w liście) i przełącza go na żywo w Selene.
    pub fn set_monitor_mode(&mut self, mode: Option<OutputMode>) {
        let Some(name) = self.selected_monitor().map(|monitor| monitor.name.clone()) else {
            return;
        };
        let value = mode
            .map(|mode| mode.config_value())
            .unwrap_or_else(|| "auto".to_owned());
        self.displays.set_mode(&name, mode);
        let live = control_send(&format!("output-mode {name} {value}"));
        match self.displays.save() {
            Ok(()) => self.show_status(if live {
                tr("Display mode changed")
            } else {
                tr("Display mode saved — it applies when the session starts")
            }),
            Err(err) => {
                self.show_status(format!("{}: {err}", tr("Could not save display settings")))
            }
        }
    }

    /// `auto` albo `1920x1080` z pickera rozdzielczości.
    pub fn set_monitor_resolution(&mut self, value: &str) {
        if value == "auto" {
            self.set_monitor_mode(None);
            return;
        }
        let Some(mode) = OutputMode::parse(value) else {
            return;
        };
        let resolution = mode.resolution();
        let current = self.selected_monitor().and_then(|monitor| {
            self.displays
                .mode_for(&monitor.name)
                .or_else(|| monitor.current_mode())
        });
        let keep_refresh = current
            .filter(|mode| mode.resolution() == resolution)
            .map(|mode| mode.refresh);
        let refresh = keep_refresh.unwrap_or_else(|| {
            self.selected_monitor()
                .and_then(|monitor| monitor.refresh_rates(resolution).first().copied())
                .unwrap_or(0)
        });
        self.set_monitor_mode(Some(OutputMode::new(mode.width, mode.height, refresh)));
    }

    /// `auto` albo liczba Hz z pickera odświeżania.
    pub fn set_monitor_refresh(&mut self, value: &str) {
        let Some((width, height)) = self.selected_resolution() else {
            return;
        };
        let refresh = if value == "auto" {
            0
        } else {
            match value.parse::<u16>() {
                Ok(refresh) => refresh,
                Err(_) => return,
            }
        };
        self.set_monitor_mode(Some(OutputMode::new(width, height, refresh)));
    }

    /// Suwak night lightu: natychmiast przez `luna-control`, zapis z opóźnieniem.
    pub fn set_night_light(&mut self, percent: u8) {
        let percent = percent.min(100);
        self.displays.night_light = percent;
        self.night_light_dirty = true;
        control_send(&format!("night-light {percent}"));
    }

    /// Rozpoczyna przeciąganie monitora w grafice układu.
    pub fn begin_monitor_drag(&mut self, index: usize, grab: Point, scale: f64) {
        let Some(monitor) = self.monitors.get(index) else {
            return;
        };
        self.selected_monitor = index;
        self.monitor_drag = Some(MonitorDrag {
            index,
            grab,
            origin: (monitor.x, monitor.y),
            scale: scale.max(0.01),
        });
    }

    /// Przesuwa przeciągany monitor; pozycja przyciąga się do krawędzi innych ekranów.
    pub fn drag_monitor(&mut self, position: Point) {
        let Some(drag) = self.monitor_drag else {
            return;
        };
        let Some(monitor) = self.monitors.get(drag.index) else {
            return;
        };
        let (width, height) = (monitor.width.max(1) as i32, monitor.height.max(1) as i32);
        let dx = ((position.x - drag.grab.x) / drag.scale).round() as i32;
        let dy = ((position.y - drag.grab.y) / drag.scale).round() as i32;
        let others: Vec<(i32, i32, i32, i32)> = self
            .monitors
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != drag.index)
            .map(|(_, other)| {
                (
                    other.x,
                    other.y,
                    other.width.max(1) as i32,
                    other.height.max(1) as i32,
                )
            })
            .collect();
        let (x, y) = snap_position(
            drag.origin.0 + dx,
            drag.origin.1 + dy,
            width,
            height,
            &others,
        );
        if let Some(monitor) = self.monitors.get_mut(drag.index) {
            monitor.x = x;
            monitor.y = y;
        }
    }

    /// Kończy przeciąganie i zapisuje układ.
    pub fn end_monitor_drag(&mut self) {
        if self.monitor_drag.take().is_none() {
            return;
        }
        self.save_monitor_layout();
    }

    /// Zapisuje pozycje wszystkich monitorów i przekazuje je Selene.
    pub fn save_monitor_layout(&mut self) {
        let positions: Vec<(String, i32, i32)> = self
            .monitors
            .iter()
            .map(|monitor| (monitor.name.clone(), monitor.x, monitor.y))
            .collect();
        for (name, x, y) in &positions {
            self.displays.set_position(name, Some((*x, *y)));
        }
        if let Err(err) = self.displays.save() {
            self.show_status(format!("{}: {err}", tr("Could not save display settings")));
            return;
        }
        for (name, x, y) in positions {
            control_send(&format!("output-position {name} {x} {y}"));
        }
        self.show_status(tr("Monitor layout saved"));
    }

    /// Układa monitory poziomo od lewej do prawej.
    pub fn auto_arrange_monitors(&mut self) {
        let mut x = 0;
        for monitor in &mut self.monitors {
            monitor.x = x;
            monitor.y = 0;
            x += monitor.width.max(1) as i32;
        }
        self.save_monitor_layout();
    }

    /// Wybiera wpis w przeglądarce tapet (katalog otwiera się od razu, plik
    /// pokazuje się natychmiast na pulpicie).
    pub fn wallpaper_pick(&mut self, index: usize) {
        let preview = {
            let Some(picker) = &mut self.wallpaper_picker else {
                return;
            };
            if index >= picker.entries.len() {
                return;
            }
            picker.selected = index;
            let path = picker.entries[index].clone();
            if path.is_dir() {
                picker.navigate(path);
                return;
            }
            if picker.target == WallpaperTarget::Desktop {
                picker.previewed = true;
                Some(path)
            } else {
                None
            }
        };
        if let Some(path) = preview {
            // Podgląd na żywo: Selene widzi zmianę `wallpaper.conf` od razu.
            if let Err(err) = wallpaper::write_config(&path) {
                self.show_status(format!("{}: {err}", tr("Could not set the wallpaper")));
            }
        }
    }

    /// Zamyka wybierak; jeśli był podgląd na żywo, przywraca poprzednią tapetę.
    pub fn cancel_wallpaper_picker(&mut self) {
        let restore = self
            .wallpaper_picker
            .as_ref()
            .and_then(WallpaperPicker::restore_target);
        self.wallpaper_picker = None;
        let Some(restore) = restore else {
            return;
        };
        let result = match restore {
            Some(path) => wallpaper::write_config(&path),
            None => wallpaper::clear_config(),
        };
        if let Err(err) = result {
            self.show_status(format!("{}: {err}", tr("Could not set the wallpaper")));
        }
    }

    /// Łączy z siecią. Hasła pytamy tylko dla **nowych** sieci — zapisany profil
    /// ma już sekrety i powinien łączyć się jednym kliknięciem. Jeśli zapisane
    /// sekrety okażą się nieaktualne, `finish_network` otworzy formularz hasła.
    fn connect_wifi(
        &mut self,
        device: String,
        ssid: String,
        security: Security,
        hidden: bool,
        known: bool,
    ) {
        self.wifi_retry = None;
        if security.needs_password() && !known {
            self.wifi_prompt = Some(WifiPrompt::new(ssid, device, security, hidden));
            return;
        }
        if security.needs_password() {
            self.wifi_retry = Some((device.clone(), ssid.clone(), security, hidden));
        }
        let rx =
            spawn_network(move || network::connect_wifi(&device, &ssid, security, None, hidden));
        self.network_pending = Some(rx);
        self.show_status(tr("Connecting…"));
    }

    fn connect_ethernet(&mut self, device: String) {
        let rx = spawn_network(move || network::connect_ethernet(&device));
        self.network_pending = Some(rx);
        self.show_status(tr("Connecting…"));
    }

    fn disconnect_device(&mut self, device: String) {
        let rx = spawn_network(move || network::disconnect_device(&device));
        self.network_pending = Some(rx);
        self.show_status(tr("Disconnecting…"));
    }

    fn forget_wifi(&mut self, ssid: String) {
        let rx = spawn_network(move || network::forget_wifi(&ssid).map(|_removed| ()));
        self.network_pending = Some(rx);
        self.show_status(tr("Forgetting the network…"));
    }

    fn forget_profile(&mut self, profile: String) {
        let rx = spawn_network(move || network::forget_connection(&profile));
        self.network_pending = Some(rx);
        self.show_status(tr("Forgetting the network…"));
    }

    fn set_autoconnect(&mut self, profile: String, enabled: bool) {
        let rx = spawn_network(move || network::set_autoconnect(&profile, enabled));
        self.network_pending = Some(rx);
        self.show_status(if enabled {
            tr("Automatic connection enabled")
        } else {
            tr("Automatic connection disabled")
        });
    }

    /// Zatwierdza hasło Wi-Fi z formularza.
    pub fn submit_wifi_password(&mut self) {
        let Some(prompt) = &mut self.wifi_prompt else {
            return;
        };
        if prompt.is_pending() {
            return;
        }
        let password = prompt.password.text().to_owned();
        if password.is_empty() {
            prompt.error = Some(tr("Enter the Wi-Fi password"));
            return;
        }
        prompt.password.clear();
        prompt.error = None;
        let device = prompt.device.clone();
        let ssid = prompt.ssid.clone();
        let security = prompt.security;
        let hidden = prompt.hidden;
        let rx = spawn_network(move || {
            network::connect_wifi(&device, &ssid, security, Some(&password), hidden)
        });
        if let Some(prompt) = &mut self.wifi_prompt {
            prompt.pending = Some(rx);
        }
        self.show_status(tr("Connecting…"));
    }

    /// Odbiera wynik operacji sieciowej (bez blokowania UI).
    pub fn poll_wifi(&mut self) {
        let pending = {
            let Some(rx) = &self.network_pending else {
                return;
            };
            match rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err(tr("NetworkManager did not respond")))
                }
            }
        };
        if let Some(result) = pending {
            self.network_pending = None;
            self.finish_network(result, true);
        }
        let result = {
            let Some(prompt) = &mut self.wifi_prompt else {
                return;
            };
            let Some(rx) = prompt.pending.as_ref() else {
                return;
            };
            match rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err(tr("NetworkManager did not respond")))
                }
            }
        };
        if let Some(result) = result {
            self.finish_network(result, false);
        }
    }

    fn finish_network(&mut self, result: Result<(), String>, toast: bool) {
        match result {
            Ok(()) => {
                self.wifi_prompt = None;
                self.wifi_retry = None;
                self.network_monitor.scan();
                self.network_next = std::time::Instant::now() + Duration::from_secs(2);
                self.show_status(tr("Network updated"));
            }
            Err(raw) => {
                let secret_problem = {
                    let lower = raw.to_lowercase();
                    lower.contains("secrets")
                        || lower.contains("password")
                        || lower.contains("hasło")
                };
                let err = friendly_network_error(&raw);
                if !toast {
                    if let Some(prompt) = &mut self.wifi_prompt {
                        prompt.pending = None;
                        prompt.error = Some(err);
                        return;
                    }
                }
                // Zapisany profil odrzucił sekrety (np. hasło zmienione na routerze)
                // — poproś o nie zamiast tylko pokazywać błąd.
                if let Some((device, ssid, security, hidden)) = self.wifi_retry.take() {
                    if secret_problem {
                        self.wifi_prompt = Some(WifiPrompt::new(ssid, device, security, hidden));
                        return;
                    }
                }
                self.show_status(format!("{}: {err}", tr("Could not update the network")));
            }
        }
    }

    /// Stosuje odłożone zmiany do okna (motyw, kontrolki okna).
    pub fn apply_pending(&mut self, frame: &mut Frame) {
        if let Some(preference) = self.pending.preference.take() {
            frame.set_preference(preference);
        }
        if let Some(accent) = self.pending.accent.take() {
            frame.set_accent(accent);
        }
        if let Some(style) = self.pending.style.take() {
            frame.set_style(style);
        }
        if self.theme_reload {
            self.theme_reload = false;
            frame.reload_theme();
        }
        if self.pending.close {
            frame.close();
            self.pending.close = false;
        }
        if self.pending.minimize {
            frame.minimize();
            self.pending.minimize = false;
        }
        if self.pending.maximize {
            frame.toggle_maximize();
            self.pending.maximize = false;
        }
        if self.pending.begin_move {
            frame.begin_move();
            self.pending.begin_move = false;
        }
    }

    /// Wykonuje akcję wybranej kontrolki.
    /// Ponawia globalny motyw ikon dla GTK/Qt/KDE po starcie aplikacji.
    ///
    /// Celowo nie robimy tego w [`SettingsApp::new`] — konstruktor jest używany
    /// przez testy i podglądy, a nie powinien pisać do `~/.config` użytkownika.
    pub fn apply_icon_theme_globally(&self) {
        let Some(name) = &self.icons.external else {
            return;
        };
        if libluna_core::icons::find_icon_set(name).is_some() {
            libluna_core::icons::apply_icon_theme_globally(name);
        }
    }

    /// Oznacza zmianę pasm/preampu (wysłanie do PipeWire po ustabilizowaniu suwaka).
    fn mark_eq_dirty(&mut self) {
        self.eq_changed_at = std::time::Instant::now();
        self.eq_dirty = true;
    }

    /// Otwiera formularz konta (dodawanie albo zmiana hasła).
    pub fn open_account_form(&mut self, mode: AccountMode) {
        if self
            .account_form
            .as_ref()
            .is_some_and(AccountForm::is_pending)
        {
            return;
        }
        self.account_form = Some(AccountForm::new(mode));
    }

    /// Sprawdza formularz i uruchamia pomocnika w tle (hasła tylko przez stdin).
    pub fn submit_account_form(&mut self) {
        let Some(form) = &mut self.account_form else {
            return;
        };
        if form.is_pending() {
            return;
        }
        let password = form.password.text().to_owned();
        let confirm = form.confirm.text().to_owned();
        let admin = form.admin.text().to_owned();

        let mode = form.mode.clone();
        let task = match &mode {
            AccountMode::AddUser => {
                let login = form.login.text().trim().to_owned();
                let full_name = form.full_name.text().trim().to_owned();
                if let Err(err) = luna::accounts_service::validate_login(&login) {
                    form.error = Some(err);
                    return;
                }
                if let Err(err) = luna::accounts_service::validate_full_name(&full_name) {
                    form.error = Some(err);
                    return;
                }
                crate::accounts_admin::AccountTask::Add { login, full_name }
            }
            AccountMode::ChangePassword { login } => crate::accounts_admin::AccountTask::Password {
                login: login.clone(),
            },
        };
        if let Err(err) = luna::accounts_service::validate_password(&password) {
            form.error = Some(err);
            return;
        }
        if password != confirm {
            form.error = Some(tr("The passwords do not match"));
            return;
        }
        if admin.is_empty() {
            form.error = Some(tr("Enter the administrator password"));
            return;
        }
        form.error = None;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = crate::accounts_admin::run_with_password(&task, &password, &admin);
            let _ = tx.send(result);
        });
        form.pending = Some(rx);
    }

    /// Odbiera wynik operacji na koncie (wołane w pętli klatki).
    pub fn poll_account_form(&mut self) {
        let Some(form) = &mut self.account_form else {
            return;
        };
        let Some(rx) = &form.pending else {
            return;
        };
        let received = match rx.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("Operacja została przerwana".to_owned()))
            }
        };
        let Some(result) = received else {
            return;
        };
        let message = form.mode.clone();
        form.pending = None;
        match result {
            Ok(()) => {
                let status = match message {
                    AccountMode::AddUser => tr("Account created"),
                    AccountMode::ChangePassword { .. } => tr("Password changed"),
                };
                self.account_form = None;
                self.accounts = libluna_core::accounts::local_users();
                self.show_status(status);
            }
            Err(err) => {
                if let Some(form) = &mut self.account_form {
                    form.error = Some(err);
                }
            }
        }
    }

    /// Zapisuje progi bezczynności i od razu przelicza je w Selene.
    ///
    /// Bez `luna-control` (np. uruchomienie poza sesją) zostaje zapis do pliku —
    /// nowe wartości zadziałają po restarcie kompozytora.
    fn update_idle(&mut self, change: impl FnOnce(&mut IdleSettings)) {
        change(&mut self.idle);
        match self.idle.save() {
            Ok(()) => {
                let live = control_send("idle-reload");
                self.show_status(if live {
                    tr("Screen and lock timers updated")
                } else {
                    tr("Timers saved — restart Selene to apply")
                });
            }
            Err(err) => {
                self.show_status(format!("{}: {err}", tr("Could not save the idle settings")))
            }
        }
    }

    /// Odświeża listę urządzeń i stan korektora (wywoływane w tle).
    pub fn refresh_audio(&mut self) {
        self.audio = equalizer::status();
        self.eq_available = equalizer::available();
        let active = equalizer::active_output();
        self.eq_output_id = active.and_then(|name| {
            self.audio
                .hardware_sinks()
                .find(|device| equalizer::node_name(device.id).as_deref() == Some(name.as_str()))
                .map(|device| device.id)
        });
        self.audio_next = std::time::Instant::now() + Duration::from_secs(3);
    }

    pub fn on_action(&mut self, action: Action) {
        match action {
            Action::None => {}
            Action::SetPreference(preference) => {
                self.theme_pref = preference;
                self.pending.preference = Some(preference);
            }
            Action::SetAccent(accent) => {
                self.accent = accent;
                self.pending.accent = Some(accent);
            }
            Action::SetStyle(index) => {
                let style = if index == 0 {
                    None
                } else {
                    self.styles.get(index - 1).map(|s| s.id.clone())
                };
                self.style = style.clone();
                self.pending.style = Some(style);
            }
            Action::SetLanguage(index) => {
                let Some((code, _)) = self.languages.get(index).cloned() else {
                    return;
                };
                self.language = code.clone();
                let locale = Locale::new(&code);
                set_locale(locale.clone());
                libluna_i18n::load_catalogs_from_dirs();
                let _ = libluna_i18n::save_preferred_locale(&locale);
                self.title = app_title();
                self.rebuild_nav();
                self.show_status(tr("Language changed — all Luna apps switch automatically"));
            }
            Action::OpenPicker(kind) => self.open_picker(kind),
            Action::SetIconTheme(theme) => {
                if self.icons.theme == theme {
                    return;
                }
                self.icons.theme = theme;
                libluna_core::icons::set_active_icon_theme(theme);
                match self.icons.save() {
                    Ok(()) => self.show_status(tr("Icon style updated")),
                    Err(err) => {
                        self.show_status(format!("{}: {err}", tr("Could not save icon style")))
                    }
                }
            }
            Action::SetExternalIconSet(name) => {
                if self.icons.external == name {
                    return;
                }
                let previous = self.icons.clone();
                self.icons.set_external(name);
                match self.icons.save() {
                    Ok(()) => {
                        // Globalnie dla wszystkich procesów libLuny (platforma wykrywa
                        // też zmianę pliku, ale ta aplikacja ma odświeżyć się od razu).
                        libluna_core::icons::set_active_icon_set(self.icons.external.clone());
                        self.show_status(tr("Icon set updated"));
                    }
                    Err(err) => {
                        self.icons = previous;
                        self.icons.apply_current_global();
                        self.show_status(format!("{}: {err}", tr("Could not save icon set")))
                    }
                }
            }
            Action::SetLockTheme(preference) => {
                if self.lock.theme == preference {
                    return;
                }
                self.lock.theme = preference;
                self.save_lock();
            }
            Action::SetLockAccent(accent) => {
                if self.lock.accent == Some(accent) {
                    return;
                }
                self.lock.accent = Some(accent);
                self.save_lock();
            }
            Action::ToggleLockClock => {
                self.lock.show_clock = !self.lock.show_clock;
                self.save_lock();
            }
            Action::SetLockWallpaper(wallpaper) => {
                if self.lock.wallpaper == wallpaper {
                    return;
                }
                self.lock.wallpaper = wallpaper;
                self.save_lock();
            }
            Action::SetNotificationPlacement(placement) => {
                if self.notification_placement == placement {
                    return;
                }
                self.notification_placement = placement;
                match notifications::save_placement(placement) {
                    Ok(()) => self.show_status(tr("Notification position updated")),
                    Err(err) => self.show_status(format!(
                        "{}: {err}",
                        tr("Could not save notification position")
                    )),
                }
            }
            Action::SetWorkspaceCount(count) => {
                if self.workspace_layout.count() == count {
                    return;
                }
                self.workspace_layout.set_count(count);
                match self.workspace_layout.save() {
                    Ok(()) => {
                        // Selene dodaje/usuwa uchwyty `ext_workspace_v1` od razu,
                        // więc panel i podgląd `Super+Tab` widzą nowy zestaw.
                        let live = control_send(&format!("workspace-count {count}"));
                        self.show_status(if live {
                            tr("Workspace count updated")
                        } else {
                            tr("Workspace count saved — restart Selene to apply")
                        });
                    }
                    Err(err) => {
                        self.show_status(format!("{}: {err}", tr("Could not save workspaces")))
                    }
                }
            }
            Action::EditWorkspaceName(index) => self.open_workspace_editor(index),
            Action::ToggleWorkspaceCorner => {
                let enabled = !self.workspace_layout.corner_switch();
                self.workspace_layout.set_corner_switch(enabled);
                match self.workspace_layout.save() {
                    Ok(()) => {
                        let live = control_send(&format!("workspace-corner {}", u8::from(enabled)));
                        self.show_status(if !live {
                            tr("Corner switching saved — restart Selene to apply")
                        } else if enabled {
                            tr("Workspaces switch at screen corners")
                        } else {
                            tr("Corner switching turned off")
                        });
                    }
                    Err(err) => {
                        self.workspace_layout.set_corner_switch(!enabled);
                        self.show_status(format!("{}: {err}", tr("Could not save workspaces")));
                    }
                }
            }
            Action::ToggleOverviewCorner => {
                let enabled = !self.workspace_layout.overview_corner();
                self.workspace_layout.set_overview_corner(enabled);
                match self.workspace_layout.save() {
                    Ok(()) => {
                        let live = control_send(&format!(
                            "workspace-overview-corner {}",
                            u8::from(enabled)
                        ));
                        self.show_status(if !live {
                            tr("Overview corner saved — restart Selene to apply")
                        } else if enabled {
                            tr("Overview opens at the top-left corner")
                        } else {
                            tr("Overview corner turned off")
                        });
                    }
                    Err(err) => {
                        self.workspace_layout.set_overview_corner(!enabled);
                        self.show_status(format!("{}: {err}", tr("Could not save workspaces")));
                    }
                }
            }
            Action::OpenWallpaperPicker => self.open_wallpaper_picker(),
            Action::OpenDesktopWallpaperPicker => self.open_desktop_wallpaper_picker(),
            Action::SetDesktopWallpaperPath(path) => self.set_desktop_wallpaper(path),
            Action::ClearDesktopWallpaper => self.clear_desktop_wallpaper(),
            Action::SelectMonitor(index) => {
                if index < self.monitors.len() {
                    self.selected_monitor = index;
                }
            }
            Action::SetNightLight(percent) => self.set_night_light(percent),
            Action::AutoArrangeMonitors => self.auto_arrange_monitors(),
            Action::SetGreeterWallpaperPath(path) => {
                if !path.is_file() {
                    self.show_status(tr("The selected file does not exist"));
                    return;
                }
                if self.greeter.wallpaper.as_deref() == Some(path.as_path()) {
                    return;
                }
                if !self.wallpapers.iter().any(|item| item == &path) {
                    self.wallpapers.insert(0, path.clone());
                }
                self.greeter.wallpaper = Some(path);
                self.greeter_dirty = true;
            }
            Action::ToggleNtp => {
                let enabled = !self.ntp.unwrap_or(false);
                self.ntp = Some(enabled);
                match std::process::Command::new("pkexec")
                    .args([
                        "timedatectl",
                        "set-ntp",
                        if enabled { "true" } else { "false" },
                    ])
                    .spawn()
                {
                    Ok(_) => {
                        self.show_status(tr("Authorization requested — check the system dialog"))
                    }
                    Err(err) => self
                        .show_status(format!("{}: {err}", tr("Could not change automatic time"))),
                }
            }
            Action::ToggleWifi => {
                let enabled = !self.network.wifi_enabled;
                match network::set_wifi_enabled(enabled) {
                    Ok(()) => {
                        self.network.wifi_enabled = enabled;
                        self.network_monitor.scan();
                        self.network_next = std::time::Instant::now() + Duration::from_secs(2);
                    }
                    Err(err) => {
                        self.show_status(format!("{}: {err}", tr("Could not change Wi-Fi")))
                    }
                }
            }
            Action::RefreshWifi => {
                self.network_monitor.scan();
                self.network_next = std::time::Instant::now() + Duration::from_secs(2);
                self.show_status(tr("Scanning for networks…"));
            }
            Action::ConnectWifi {
                device,
                ssid,
                security,
                hidden,
                known,
            } => self.connect_wifi(device, ssid, security, hidden, known),
            Action::ConnectEthernet(device) => self.connect_ethernet(device),
            Action::DisconnectDevice(device) => self.disconnect_device(device),
            Action::ForgetWifi(ssid) => self.forget_wifi(ssid),
            Action::ForgetProfile(profile) => self.forget_profile(profile),
            Action::SetAutoconnect { profile, enabled } => self.set_autoconnect(profile, enabled),
            Action::OpenWiredEditor(device) => self.open_wired_editor(device),
            Action::OpenHiddenNetwork => self.open_hidden_network(),
            Action::SetRepeatDelay(delay) => {
                self.input.set_repeat_delay(delay);
                self.save_input();
            }
            Action::SetRepeatRate(rate) => {
                self.input.set_repeat_rate(rate);
                self.save_input();
            }
            Action::ToggleNaturalScroll => {
                let enabled = !self.input.natural_scroll();
                self.input.set_natural_scroll(enabled);
                self.save_input();
            }
            Action::ApplySystemTimezone => self.apply_system_timezone(),
            Action::SetVolume(percent) => {
                let muted = self.volume.map(|(_, m)| m).unwrap_or(false);
                self.volume = Some((percent, muted));
                system::set_volume(percent);
            }
            Action::ToggleMute => {
                system::toggle_mute();
                if let Some((volume, muted)) = self.volume {
                    self.volume = Some((volume, !muted));
                }
            }
            Action::SetSink(id) => {
                if self.eq.enabled {
                    // Zmiana wyjścia przy włączonym korektorze: najpierw ustawiamy
                    // sprzęt jako domyślny, a `apply` zapamiętuje go i przepina łańcuch.
                    equalizer::set_default(id);
                    match equalizer::apply(&self.eq) {
                        Ok(()) => self.eq_error = None,
                        Err(err) => self.eq_error = Some(err),
                    }
                } else {
                    equalizer::set_default(id);
                }
                self.audio = equalizer::status();
                self.audio_next = std::time::Instant::now() + Duration::from_millis(500);
            }
            Action::SetSource(id) => {
                equalizer::set_default(id);
                self.audio = equalizer::status();
                self.audio_next = std::time::Instant::now() + Duration::from_millis(500);
            }
            Action::SetIdleScreenOff(secs) => {
                self.update_idle(|idle| idle.screen_off_secs = secs);
            }
            Action::SetIdleLock(secs) => {
                self.update_idle(|idle| idle.lock_secs = secs);
            }
            Action::SetIdleSuspend(secs) => {
                self.update_idle(|idle| idle.suspend_secs = secs);
            }
            Action::OpenAddAccount => self.open_account_form(AccountMode::AddUser),
            Action::OpenPasswordChange(login) => {
                self.open_account_form(AccountMode::ChangePassword { login })
            }
            Action::ToggleLockOnBlank => {
                let enabled = !self.idle.lock_on_blank;
                self.update_idle(|idle| idle.lock_on_blank = enabled);
            }
            Action::ToggleEqualizer => {
                // Wyłączyć można zawsze (np. gdy wtyczka zniknęła przy aktualizacji);
                // włączenie wymaga zainstalowanej wtyczki.
                if !self.eq.enabled && !self.eq_available {
                    self.show_status(tr("The luna-eq plugin is not installed"));
                    return;
                }
                self.eq.enabled = !self.eq.enabled;
                self.eq_dirty = false;
                match equalizer::save_and_apply(&self.eq) {
                    Ok(()) => {
                        self.eq_error = None;
                        self.show_status(if self.eq.enabled {
                            tr("Equalizer enabled")
                        } else {
                            tr("Equalizer disabled")
                        });
                    }
                    Err(err) => {
                        self.eq_error = Some(err.clone());
                        self.show_status(err);
                    }
                }
                self.audio_next = std::time::Instant::now() + Duration::from_millis(500);
            }
            Action::SetEqPreset(preset) => {
                if preset != Preset::Custom {
                    self.eq.set_preset(preset);
                    self.mark_eq_dirty();
                }
            }
            Action::SetEqPreamp(db) => {
                self.eq.set_preamp(db as f32);
                self.mark_eq_dirty();
            }
            Action::SetEqBand(index, db) => {
                self.eq.set_band(index, db as f32);
                self.mark_eq_dirty();
            }
            Action::SetBrightness(percent) => {
                self.brightness = Some(percent);
                system::set_brightness(percent);
            }
            Action::TogglePowerSaver => {
                self.power_saver = !self.power_saver;
                system::set_power_saver(self.power_saver);
                if system::ppd_available() {
                    let profile = if self.power_saver {
                        "power-saver"
                    } else {
                        "balanced"
                    };
                    system::set_power_profile(profile);
                    self.power_profile = Some(profile.to_owned());
                } else {
                    control_send(&format!("power-saver {}", u8::from(self.power_saver)));
                }
            }
            Action::ToggleDnd => {
                self.dnd = !self.dnd;
                system::set_dnd(self.dnd);
            }
            Action::SetGreeterWallpaper(index) => {
                let Some(path) = self.wallpapers.get(index).cloned() else {
                    return;
                };
                if self.greeter.wallpaper.as_deref() == Some(path.as_path()) {
                    return;
                }
                self.greeter.wallpaper = Some(path);
                self.greeter_dirty = true;
            }
            Action::ClearGreeterWallpaper => {
                if self.greeter.wallpaper.is_none() {
                    return;
                }
                self.greeter.wallpaper = None;
                self.greeter_dirty = true;
            }
            Action::UseDesktopWallpaper => match libluna_render::wallpaper::configured_path() {
                Some(path) if self.greeter.wallpaper.as_deref() != Some(path.as_path()) => {
                    self.greeter.wallpaper = Some(path);
                    self.greeter_dirty = true;
                }
                Some(_) => {}
                None => self.show_status(tr("No desktop wallpaper is set")),
            },
            Action::SetGreeterTheme(preference) => {
                if self.greeter.theme == preference {
                    return;
                }
                self.greeter.theme = preference;
                self.greeter_dirty = true;
            }
            Action::SetGreeterAccent(accent) => {
                if self.greeter.accent == Some(accent) {
                    return;
                }
                self.greeter.accent = Some(accent);
                self.greeter_dirty = true;
            }
            Action::ToggleGreeterClock => {
                self.greeter.show_clock = !self.greeter.show_clock;
                self.greeter_dirty = true;
            }
            Action::ApplyGreeter => self.apply_greeter(),
            Action::TestNotification => self.test_notification(),
            Action::SetBluetoothPowered { adapter, powered } => self
                .run_task(PendingKind::Bluetooth, move || {
                    bluetooth::set_powered(&adapter, powered)
                }),
            Action::SetBluetoothDiscovery { adapter, on } => self
                .run_task(PendingKind::Bluetooth, move || {
                    bluetooth::set_discovering(&adapter, on)
                }),
            Action::BluetoothPair(device) => {
                self.run_task(PendingKind::Bluetooth, move || bluetooth::pair(&device))
            }
            Action::BluetoothConnect(device) => {
                self.run_task(PendingKind::Bluetooth, move || bluetooth::connect(&device))
            }
            Action::BluetoothDisconnect(device) => self
                .run_task(PendingKind::Bluetooth, move || {
                    bluetooth::disconnect(&device)
                }),
            Action::BluetoothRemove { adapter, device } => self
                .run_task(PendingKind::Bluetooth, move || {
                    bluetooth::remove(&adapter, &device)
                }),
            Action::RefreshBluetooth => self.refresh_bluetooth(),
            Action::SetPowerProfile(name) => {
                self.power_profiles.active = name.clone();
                self.power_saver = name == "power-saver";
                self.power_profile = Some(name.clone());
                self.run_task(PendingKind::Power, move || {
                    power_profile::set_profile(&name)
                });
            }
            Action::VpnConnect(uuid) => {
                self.run_task(PendingKind::Vpn, move || vpn::connect(&uuid))
            }
            Action::VpnDisconnect(uuid) => {
                self.run_task(PendingKind::Vpn, move || vpn::disconnect(&uuid))
            }
            Action::VpnRemove(uuid) => self.run_task(PendingKind::Vpn, move || vpn::remove(&uuid)),
            Action::OpenVpnImport => self.open_vpn_import(),
            Action::PrinterDefault(name) => {
                self.run_task(PendingKind::Printer, move || printers::set_default(&name))
            }
            Action::PrinterEnabled { name, enabled } => self
                .run_task(PendingKind::Printer, move || {
                    printers::set_enabled(&name, enabled)
                }),
            Action::PrinterRemove(name) => {
                self.run_task(PendingKind::Printer, move || printers::remove(&name))
            }
            Action::OpenPrinterAdd => self.open_printer_add(),
            Action::SwitchSession(id) => {
                self.run_task(PendingKind::Session, move || sessions::switch_to(&id));
            }
            Action::OpenColorPicker(device) => {
                self.color_target = Some(device);
                self.open_picker(PickerKind::ColorProfile);
            }
            Action::OpenColorImport => self.open_color_import(),
            Action::SetColorProfile(profile) => {
                let Some(device) = self.color_target.take() else {
                    return;
                };
                self.run_task(PendingKind::Color, move || {
                    colors::assign(&device, &profile)
                });
            }
            Action::SetTextScale(percent) => {
                self.accessibility.text_scale =
                    libluna_core::AccessibilitySettings::clamp_scale(percent);
                self.save_accessibility();
            }
            Action::ToggleHighContrast => {
                self.accessibility.high_contrast = !self.accessibility.high_contrast;
                self.save_accessibility();
            }
            Action::ToggleReduceMotion => {
                self.accessibility.reduce_motion = !self.accessibility.reduce_motion;
                self.save_accessibility();
            }
            Action::OpenDefaultAppPicker(index) => {
                if index >= default_apps::CATEGORIES.len() {
                    return;
                }
                self.default_app_target = Some(index);
                self.open_picker(PickerKind::DefaultApp);
            }
            Action::SetDefaultApp(desktop_id) => {
                let Some(index) = self.default_app_target.take() else {
                    return;
                };
                if self
                    .defaults
                    .current
                    .get(index)
                    .is_some_and(|current| current.as_deref() == Some(desktop_id.as_str()))
                {
                    return;
                }
                self.defaults.current[index] = Some(desktop_id.clone());
                self.run_task(PendingKind::DefaultApps, move || {
                    default_apps::set_default(index, &desktop_id)
                });
            }
            Action::RecordShortcut(action) => {
                self.shortcut_record = Some(action);
            }
            Action::AboutVersionClick => self.note_about_click(),
            Action::ResetShortcuts => self.reset_shortcuts(),
        }
    }

    /// Liczy szybkie kliknięcia w wersję (5 w 1,5 s = easter egg).
    pub fn note_about_click(&mut self) {
        let now = std::time::Instant::now();
        let quick = self
            .about_clicked_at
            .is_some_and(|at| now.duration_since(at) < Duration::from_millis(1500));
        self.about_clicks = if quick {
            self.about_clicks.saturating_add(1)
        } else {
            1
        };
        self.about_clicked_at = Some(now);
        if self.about_clicks >= 5 {
            self.about_clicks = 0;
            if !self.easter_egg {
                self.easter_egg = true;
                self.show_status(tr("You found the moon egg"));
            }
        }
    }

    /// Zapisuje nową kombinację dla nagrywanej akcji (ostrzega o konflikcie).
    pub fn assign_shortcut(&mut self, binding: Binding) {
        let Some(action) = self.shortcut_record else {
            return;
        };
        let conflict = self.shortcuts.conflicts_with(binding, action);
        self.shortcuts.assign(action, binding);
        let saved = if self.demo {
            Ok(())
        } else {
            self.shortcuts.save()
        };
        match saved {
            Ok(()) => {
                let label = crate::pages::shortcut_action_label(action);
                match conflict {
                    Some(other) => self.show_status(tr_args(
                        "Shortcut reassigned from {action}",
                        &[("action", &crate::pages::shortcut_action_label(other))],
                    )),
                    None => self.show_status(tr_args(
                        "Shortcut saved for {action}",
                        &[("action", &label)],
                    )),
                }
            }
            Err(err) => self.show_status(format!("{}: {err}", tr("Could not save shortcuts"))),
        }
    }

    fn reset_shortcuts(&mut self) {
        self.shortcuts = ShortcutSettings::default();
        let saved = if self.demo {
            Ok(())
        } else {
            self.shortcuts.save()
        };
        match saved {
            Ok(()) => self.show_status(tr("Shortcuts restored to defaults")),
            Err(err) => self.show_status(format!("{}: {err}", tr("Could not save shortcuts"))),
        }
    }

    /// Zapisuje ustawienia dostępności (motyw nakłada je przy następnej klatce).
    fn save_accessibility(&mut self) {
        self.accessibility = self.accessibility.normalized();
        self.theme_reload = true;
        if let Err(err) = self.accessibility.save() {
            self.show_status(format!(
                "{}: {err}",
                tr("Could not save accessibility settings")
            ));
        }
    }

    /// Zapisuje ustawienia ekranu blokady (per użytkownik, bez roota).
    fn save_lock(&mut self) {
        match self.lock.save() {
            Ok(()) => self.show_status(tr("Lock screen updated")),
            Err(err) => {
                self.show_status(format!("{}: {err}", tr("Could not save the lock screen")))
            }
        }
    }

    /// Prosi system o zmianę strefy przez polkit (może pokazać okno autoryzacji).
    fn apply_system_timezone(&mut self) {
        let timezone = self.region.effective_timezone();
        match std::process::Command::new("pkexec")
            .args(["timedatectl", "set-timezone", &timezone])
            .spawn()
        {
            Ok(_) => self.show_status(tr("Authorization requested — check the system dialog")),
            Err(err) => self.show_status(format!(
                "{}: {err}",
                tr("Could not start system authorization")
            )),
        }
    }

    /// Otwiera formularz hasła administratora dla zmian ekranu logowania.
    fn apply_greeter(&mut self) {
        if std::env::var_os("LUNA_GREETER_CONFIG").is_some() {
            // Tryb podglądu/testów — zapis bez autoryzacji.
            self.save_greeter();
            return;
        }
        if self.admin.as_ref().is_some_and(AdminPrompt::is_pending) {
            return;
        }
        self.admin = Some(AdminPrompt::new());
    }

    /// Zatwierdza hasło z formularza; zapis idzie w wątku tła (sudo/PAM).
    pub fn submit_admin_password(&mut self) {
        let Some(prompt) = &mut self.admin else {
            return;
        };
        if prompt.is_pending() {
            return;
        }
        let password = prompt.password.text().to_owned();
        if password.is_empty() {
            prompt.error = Some(tr("Enter the administrator password"));
            return;
        }
        let settings = self.greeter.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = crate::greeter_admin::save_with_password(&settings, &password);
            let _ = tx.send(result);
        });
        prompt.password.clear();
        prompt.error = None;
        prompt.pending = Some(rx);
    }

    /// Odbiera wynik zapisu z formularza hasła (bez blokowania UI).
    pub fn poll_admin(&mut self) {
        let result = {
            let Some(prompt) = &mut self.admin else {
                return;
            };
            let Some(rx) = prompt.pending.as_ref() else {
                return;
            };
            match rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err(tr("Administrator authorization failed")))
                }
            }
        };
        match result {
            Some(Ok(())) => {
                self.admin = None;
                self.greeter_dirty = false;
                self.reload_greeter();
                self.show_status(tr("Login screen updated"));
            }
            Some(Err(err)) => {
                if let Some(prompt) = &mut self.admin {
                    prompt.pending = None;
                    prompt.error = Some(err);
                }
            }
            None => {}
        }
    }

    /// Wysyła testowe powiadomienie przez D-Bus (`luna-shell` jest daemonem).
    fn test_notification(&mut self) {
        let title = tr("Luna test notification");
        let body = tr("If you can see this, notifications work");
        match crate::notify::send_test_notification(&title, &body) {
            Ok(()) => {
                if let Ok(info) = crate::notify::server_status() {
                    self.notification_daemon = info;
                }
                self.show_status(tr("Test notification sent"));
            }
            Err(err) => {
                self.notification_daemon = format!("{} ({err})", tr("Not running"));
                self.show_status(format!(
                    "{}: {err}",
                    tr("Could not send the test notification")
                ));
            }
        }
    }

    /// Wczytuje konfigurację greetera z dysku (po udanym zapisie).
    fn reload_greeter(&mut self) {
        self.greeter = GreeterSettings::load();
        self.wallpapers = pages::wallpapers();
    }

    /// Zapisuje ustawienia ekranu logowania i odświeża stan z dysku.
    ///
    /// Pomocnik przez `pkexec` może skopiować tapetę do `/var/lib/luna/wallpapers`,
    /// więc po zapisie czytamy plik ponownie — UI pokazuje to, co realnie zobaczy greeter.
    fn save_greeter(&mut self) {
        match crate::greeter_admin::save(&self.greeter) {
            Ok(()) => {
                self.greeter_dirty = false;
                self.reload_greeter();
                self.show_status(tr("Login screen updated"));
            }
            Err(err) => {
                // Cofamy zmianę w UI, żeby nie pokazywać stanu, którego nie ma na dysku.
                self.reload_greeter();
                self.show_status(format!(
                    "{}: {err}",
                    tr("Could not update the login screen")
                ));
            }
        }
    }
}

/// Tytuł aplikacji z nazwą systemu z `/etc/os-release`.
///
/// Przykład: `Pagan Linux Settings` / `Ustawienia — Pagan Linux`.
pub fn app_title() -> String {
    let system = libluna_core::os_name();
    tr_args("{system} Settings", &[("system", &system)])
}

fn nav_entries() -> (NavList, Vec<Option<Page>>) {
    let sections: Vec<(&str, Vec<Page>)> = vec![
        (
            "Environment",
            vec![
                Page::Appearance,
                Page::Login,
                Page::LockScreen,
                Page::Notifications,
                Page::Language,
                Page::Workspaces,
                Page::Accessibility,
                Page::DefaultApps,
            ],
        ),
        ("People", vec![Page::Users]),
        (
            "Hardware",
            vec![
                Page::Network,
                Page::Bluetooth,
                Page::Input,
                Page::Shortcuts,
                Page::Displays,
                Page::Sound,
                Page::Power,
                Page::Printers,
            ],
        ),
        ("Information", vec![Page::System, Page::About]),
    ];
    let mut entries = Vec::new();
    let mut pages = Vec::new();
    let mut icons = Vec::new();
    for (section, items) in sections {
        entries.push(NavEntry::Section(tr(section)));
        pages.push(None);
        icons.push(None);
        for page in items {
            entries.push(NavEntry::Item(tr(page.title_key())));
            pages.push(Some(page));
            icons.push(Some(page.icon()));
        }
    }
    let mut nav = NavList::new(entries).monochrome(true);
    nav.set_icons(icons);
    (nav, pages)
}

/// Uruchamia operację sieciową w tle i zwraca kanał z wynikiem.
fn spawn_network<F>(operation: F) -> mpsc::Receiver<Result<(), String>>
where
    F: FnOnce() -> Result<(), String> + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(operation());
    });
    rx
}

/// Nazwa drukarki wyliczona z adresu IPP (np. `ipp://biuro.local:631/ipp/print` → `biuro.local`).
fn printer_name_from_uri(uri: &str) -> Option<String> {
    let rest = uri.split_once("://")?.1;
    let authority = rest.split(['/', '?']).next()?;
    let host = authority.rsplit('@').next()?.split(':').next()?;
    if host.is_empty() {
        return None;
    }
    let name: String = host
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    (!name.is_empty() && name.len() <= 100).then_some(name)
}

/// Przyciąga pozycję monitora do krawędzi innych ekranów (tolerancja w px logicznych).
fn snap_position(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    others: &[(i32, i32, i32, i32)],
) -> (i32, i32) {
    const TOUCH: i32 = 24;
    let (mut x, mut y) = (x, y);
    for &(ox, oy, ow, oh) in others {
        // Ten sam pas w pionie — przyciągnij lewą/prawą krawędź.
        if y < oy + oh + TOUCH && y + height > oy - TOUCH {
            if (x - (ox + ow)).abs() <= TOUCH {
                x = ox + ow;
            }
            if ((x + width) - ox).abs() <= TOUCH {
                x = ox - width;
            }
        }
        // Ten sam pas w poziomie — przyciągnij górną/dolną krawędź i wyrównaj górę.
        if x < ox + ow + TOUCH && x + width > ox - TOUCH {
            if (y - (oy + oh)).abs() <= TOUCH {
                y = oy + oh;
            }
            if ((y + height) - oy).abs() <= TOUCH {
                y = oy - height;
            }
            if (y - oy).abs() <= TOUCH {
                y = oy;
            }
        }
    }
    (x, y)
}

/// Zamienia techniczne błędy NetworkManagera na komunikat z podpowiedzią.
fn friendly_network_error(err: &str) -> String {
    let lower = err.to_lowercase();
    if lower.contains("insufficient privileges") || lower.contains("not authorized") {
        return tr("NetworkManager requires administrator authorization for this change");
    }
    if lower.contains("secrets were required") || lower.contains("no secrets") {
        return tr("The password was rejected — check it and try again");
    }
    err.to_owned()
}

/// Wysyła polecenie do Selene (`$XDG_RUNTIME_DIR/luna-control`).
///
/// `false` = brak gniazda/połączenia (np. Selene nie działa), więc wywołujący
/// może powiedzieć użytkownikowi, że zmiana zadziała po restarcie kompozytora.
fn control_send(line: &str) -> bool {
    use std::io::Write;
    let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") else {
        return false;
    };
    let path = std::path::Path::new(&runtime).join("luna-control");
    match std::os::unix::net::UnixStream::connect(path) {
        Ok(mut stream) => writeln!(stream, "{line}").is_ok(),
        Err(_) => false,
    }
}

/// Zamienia wielolinijkowy tekst na jedną linię (protokół `luna-control`).
fn one_line(text: &str) -> String {
    text.replace(['\n', '\r'], " ").trim().to_owned()
}

/// Uruchamia centrum ustawień.
pub fn run() -> Result<(), Box<dyn Error>> {
    // Język z Ustawień (jeśli wybrany) ma pierwszeństwo przed `LANG`.
    set_locale(libluna_i18n::preferred_locale());
    libluna_i18n::load_catalogs_from_dirs();

    let settings = ThemeSettings::load();
    let system_dark = std::env::var("LUNA_SYSTEM_DARK")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);
    let mut app = SettingsApp::new(&settings);
    // `luna-settings --page network` — panel otwiera konkretną stronę.
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--page" {
            if let Some(page) = args.next().as_deref().and_then(pages::Page::by_name) {
                app.go_to(page);
            }
        } else if let Some(page) = arg.strip_prefix("--page=").and_then(pages::Page::by_name) {
            app.go_to(page);
        }
    }
    // Tytuł okna pochodzi z nazwy systemu (`/etc/os-release`), nie z „Luna”.
    let config = AppConfig::new(app.title.clone())
        .app_id("eu.luna.Settings")
        .initial_size(1080, 760)
        .settings(settings.clone())
        .system_dark(system_dark)
        // Esc zamyka modale/panele, ale nie całe Ustawienia.
        .escape_to_exit(false);

    app.apply_icon_theme_globally();
    // Język może zmienić inna aplikacja (np. luna-files) — nawigacja trzyma
    // gotowe etykiety, więc odświeżamy ją po zmianie katalogu.
    let mut ui_language = libluna_i18n::locale().language().to_owned();
    run_app(config, move |frame: &mut Frame| {
        let language = libluna_i18n::locale().language().to_owned();
        if language != ui_language {
            ui_language = language.clone();
            app.language = language;
            app.title = app_title();
            app.rebuild_nav();
            frame.set_title(app.title.clone());
        }
        app.apply_pending(frame);
        app.sync_system();
        app.poll_admin();
        app.poll_wifi();
        app.poll_account_form();
        app.poll_bluetooth();
        app.poll_action_tasks();

        // Bluetooth: odświeżanie co 2 s, dopóki strona jest otwarta (skanowanie).
        if app.page == Page::Bluetooth && app.bluetooth_next.elapsed() >= Duration::from_secs(2) {
            app.bluetooth_next = std::time::Instant::now() + Duration::from_secs(2);
            app.refresh_bluetooth();
        }

        // Motyw z bieżących ustawień; zmiana dostępności oznacza go do przeliczenia.
        ui::handle_events(&mut app, frame.events, frame.size, &frame.theme, frame.text);

        // Ctrl+C/X w polach tekstowych → systemowy schowek; Ctrl+V → odczyt.
        if let Some(text) = app.clipboard_text.take() {
            frame.copy_to_clipboard(text);
        }
        if app.paste_requested {
            app.paste_requested = false;
            frame.paste_from_clipboard();
        }

        ui::paint(&app, frame.scene, frame.size, &frame.theme, frame.text);

        if app.status.is_animating() {
            app.status.advance(frame.delta_time(), 2.5);
            frame.request_frame();
        }
        if app.admin.as_ref().is_some_and(|admin| admin.is_pending()) {
            frame.request_frame_after(Duration::from_millis(50));
        }
        if app.wifi_prompt.as_ref().is_some_and(WifiPrompt::is_pending)
            || app.network_pending.is_some()
        {
            frame.request_frame_after(Duration::from_millis(100));
        }
        if !app.action_tasks.is_empty() || app.bluetooth_snapshot.is_some() {
            frame.request_frame_after(Duration::from_millis(100));
        }
        if app.info_sheet.is_animating() {
            app.info_sheet.advance(frame.delta_time(), 0.22);
            frame.request_frame();
        }
        if let Some(until) = app.status_until {
            if std::time::Instant::now() >= until {
                app.status.set_open_animated(false);
                app.status_until = None;
            }
            frame.request_frame();
        }
        // Stan systemu (bateria, głośność) zmienia się poza UI.
        frame.request_frame_after(Duration::from_secs(1));
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> SettingsApp {
        SettingsApp::new(&ThemeSettings::load())
    }

    /// Testy współdzielą `LUNA_DISPLAYS_CONF` — serializujemy je, żeby nie kasowały
    /// sobie nawzajem zmiennej środowiskowej.
    static DISPLAYS_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn lock_displays_env() -> std::sync::MutexGuard<'static, ()> {
        DISPLAYS_ENV.lock().unwrap_or_else(|err| err.into_inner())
    }

    #[test]
    fn picker_filters_and_preselects_current_language() {
        let mut app = app();
        app.language = "de".to_owned();
        app.open_picker(PickerKind::Language);
        let picker = app.picker.as_ref().expect("picker");
        let filtered = app.picker_filtered();
        assert!(!filtered.is_empty());
        assert_eq!(filtered[picker.selected].value, "de");

        app.picker.as_mut().unwrap().query.set_text("de");
        let filtered = app.picker_filtered();
        assert!(filtered.iter().all(|item| item.value == "de"));
    }

    #[test]
    fn workspace_editor_starts_with_current_name() {
        let mut app = app();
        app.workspace_layout.set_name(0, Some("Kod"));
        app.open_workspace_editor(0);
        let editor = app.editor.as_ref().expect("editor");
        assert_eq!(editor.entry.text(), "Kod");
        assert!(editor.title.contains('1'));
    }

    #[test]
    fn every_page_has_nav_entry() {
        let app = app();
        assert_eq!(
            app.nav_pages.iter().filter(|p| p.is_some()).count(),
            Page::ALL.len()
        );
        assert!(app
            .nav
            .entries()
            .iter()
            .any(|e| matches!(e, NavEntry::Section(_))));
    }

    #[test]
    fn go_to_selects_page_without_losing_nav_selection() {
        let mut app = app();
        app.go_to(Page::Sound);
        assert_eq!(app.page, Page::Sound);
        let selected = app.nav.selected().expect("selection");
        assert_eq!(app.nav_pages[selected], Some(Page::Sound));
    }

    #[test]
    fn style_and_language_actions_update_state() {
        let mut app = app();
        app.on_action(Action::SetPreference(ThemePreference::Dark));
        assert_eq!(app.theme_pref, ThemePreference::Dark);
        app.on_action(Action::SetAccent(Accent::Purple));
        assert_eq!(app.accent, Accent::Purple);
        app.on_action(Action::SetStyle(0));
        assert_eq!(app.style, None);
    }

    #[test]
    fn wallpaper_tree_lists_dirs_then_png_files() {
        let dir = std::env::temp_dir().join(format!(
            "luna-wallpaper-picker-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let sub = dir.join("Obrazy");
        std::fs::create_dir_all(&sub).expect("katalog");
        std::fs::write(dir.join("lato.png"), b"png").expect("png");
        std::fs::write(dir.join("zima.jpg"), b"jpg").expect("jpg");

        let entries = scan_wallpaper_dir(&dir);
        assert!(entries.first().is_some_and(|path| path.is_dir()));
        assert!(entries.iter().any(|path| path.ends_with("Obrazy")));
        assert!(entries.iter().any(|path| path.ends_with("lato.png")));
        assert!(!entries.iter().any(|path| path.ends_with("zima.jpg")));

        let mut picker = WallpaperPicker {
            target: WallpaperTarget::Greeter,
            dir,
            entries,
            selected: 0,
            scroll: 0.0,
            hover: None,
            browse: false,
            more: false,
            more_hover: None,
            sources: Vec::new(),
            original: None,
            previewed: false,
        };
        let png = picker
            .entries
            .iter()
            .position(|path| path.ends_with("lato.png"))
            .expect("png");
        picker.selected = png;
        assert!(picker.selected_path().is_some_and(|path| {
            path.file_name().and_then(|name| name.to_str()) == Some("lato.png")
        }));
        let _ = std::fs::remove_dir_all(&picker.dir);
    }

    #[test]
    fn wired_editor_prefills_the_profile() {
        use luna::network::{Device, DeviceKind, DeviceState};

        let mut app = app();
        app.network.devices = vec![Device {
            path: "/org/freedesktop/NetworkManager/Devices/2".to_owned(),
            interface: "enp9s0".to_owned(),
            kind: DeviceKind::Ethernet,
            state: DeviceState::Disconnected,
            driver: "r8169".to_owned(),
            hw_address: "aa:bb".to_owned(),
            connection: None,
            ip4: None,
            wired: Some(WiredConfig {
                method: Ipv4Method::Manual,
                address: "192.168.1.20/24".to_owned(),
                gateway: "192.168.1.1".to_owned(),
                dns: vec!["1.1.1.1".to_owned(), "8.8.8.8".to_owned()],
                profile: Some("/org/freedesktop/NetworkManager/Settings/9".to_owned()),
                autoconnect: true,
            }),
            metered: false,
        }];
        app.open_wired_editor("/org/freedesktop/NetworkManager/Devices/2".to_owned());
        let editor = app.wired_editor.as_ref().expect("editor");
        assert_eq!(editor.address.text(), "192.168.1.20/24");
        assert_eq!(editor.gateway.text(), "192.168.1.1");
        assert_eq!(editor.dns.text(), "1.1.1.1, 8.8.8.8");
        assert!(editor.profile.is_some());
        assert!(editor.error.is_none());
    }

    #[test]
    fn wired_editor_keeps_invalid_address_open() {
        use luna::network::{Device, DeviceKind, DeviceState};

        let mut app = app();
        app.network.devices = vec![Device {
            path: "/dev/2".to_owned(),
            interface: "enp9s0".to_owned(),
            kind: DeviceKind::Ethernet,
            state: DeviceState::Disconnected,
            driver: String::new(),
            hw_address: String::new(),
            connection: None,
            ip4: None,
            wired: None,
            metered: false,
        }];
        app.open_wired_editor("/dev/2".to_owned());
        app.wired_editor
            .as_mut()
            .expect("editor")
            .address
            .set_text("nie-adres");
        app.save_wired_editor();
        let editor = app.wired_editor.as_ref().expect("editor stays open");
        assert!(editor.error.is_some());
        assert!(app.network_pending.is_none());
    }

    #[test]
    fn hidden_network_opens_the_password_prompt() {
        use luna::network::{Device, DeviceKind, DeviceState};

        let mut app = app();
        app.network.devices = vec![Device {
            path: "/dev/wifi".to_owned(),
            interface: "wlp1".to_owned(),
            kind: DeviceKind::Wifi,
            state: DeviceState::Disconnected,
            driver: String::new(),
            hw_address: String::new(),
            connection: None,
            ip4: None,
            wired: None,
            metered: false,
        }];
        app.open_hidden_network();
        app.editor
            .as_mut()
            .expect("editor")
            .entry
            .set_text("Ukryta");
        app.save_editor();
        let prompt = app.wifi_prompt.as_ref().expect("password prompt");
        assert_eq!(prompt.ssid, "Ukryta");
        assert!(prompt.hidden);
        assert_eq!(prompt.device, "/dev/wifi");
    }

    #[test]
    fn account_form_validates_passwords_before_running_sudo() {
        let mut app = SettingsApp::new(&libluna_core::ThemeSettings::load());
        app.open_account_form(AccountMode::AddUser);
        {
            let form = app.account_form.as_mut().expect("formularz");
            form.login.set_text("ala");
            form.password.set_text("tajne123");
            form.confirm.set_text("inne");
            form.admin.set_text("admin");
        }
        app.submit_account_form();
        let form = app.account_form.as_ref().expect("formularz otwarty");
        assert!(form.pending.is_none(), "nic nie może iść do sudo");
        assert_eq!(
            form.error.as_deref(),
            Some(tr("The passwords do not match").as_str())
        );

        // Poprawne hasła, ale brak hasła administratora — nadal bez sudo.
        {
            let form = app.account_form.as_mut().expect("formularz");
            form.confirm.set_text("tajne123");
            form.admin.clear();
        }
        app.submit_account_form();
        let form = app.account_form.as_ref().expect("formularz otwarty");
        assert!(form.pending.is_none());
        assert_eq!(
            form.error.as_deref(),
            Some(tr("Enter the administrator password").as_str())
        );
    }

    #[test]
    fn account_form_rejects_invalid_login() {
        let mut app = SettingsApp::new(&libluna_core::ThemeSettings::load());
        app.open_account_form(AccountMode::AddUser);
        {
            let form = app.account_form.as_mut().expect("formularz");
            form.login.set_text("Ala Kowalska");
            form.password.set_text("tajne123");
            form.confirm.set_text("tajne123");
            form.admin.set_text("admin");
        }
        app.submit_account_form();
        let form = app.account_form.as_ref().expect("formularz");
        assert!(form.pending.is_none());
        assert!(form
            .error
            .as_deref()
            .is_some_and(|err| err.contains("Login")));
    }

    #[test]
    fn account_form_tab_cycles_only_visible_fields() {
        let mut app = SettingsApp::new(&libluna_core::ThemeSettings::load());
        app.open_account_form(AccountMode::ChangePassword {
            login: "ala".to_owned(),
        });
        let form = app.account_form.as_mut().expect("formularz");
        assert_eq!(
            form.fields(),
            vec![
                AccountField::Password,
                AccountField::Confirm,
                AccountField::Admin
            ]
        );
        form.cycle_focus(false);
        assert_eq!(form.focus, AccountField::Confirm);
        form.cycle_focus(false);
        assert_eq!(form.focus, AccountField::Admin);
        form.cycle_focus(false);
        assert_eq!(form.focus, AccountField::Password);
        form.cycle_focus(true);
        assert_eq!(form.focus, AccountField::Admin);
    }

    #[test]
    fn monitor_mode_picker_saves_resolution_and_refresh() {
        let _env = lock_displays_env();
        use crate::pages::Monitor;
        use libluna_core::{DisplaySettings, OutputMode};

        let path = std::env::temp_dir().join(format!(
            "luna-settings-displays-{}.conf",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        std::env::set_var("LUNA_DISPLAYS_CONF", &path);

        let mut app = app();
        app.displays = DisplaySettings::default();
        app.monitors = vec![Monitor {
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
        }];
        app.selected_monitor = 0;

        // Ta sama rozdzielczość — zostaje bieżące odświeżanie.
        app.set_monitor_resolution("1920x1080");
        assert_eq!(
            app.displays.mode_for("eDP-1"),
            Some(OutputMode::new(1920, 1080, 60))
        );

        // Nowa rozdzielczość — bierzemy najwyższe dostępne Hz.
        app.set_monitor_resolution("1280x720");
        assert_eq!(
            app.displays.mode_for("eDP-1"),
            Some(OutputMode::new(1280, 720, 60))
        );

        app.set_monitor_refresh("144");
        assert_eq!(
            app.displays.mode_for("eDP-1"),
            Some(OutputMode::new(1280, 720, 144))
        );

        let saved = std::fs::read_to_string(&path).expect("displays.conf");
        assert!(
            saved.contains("output eDP-1 mode = 1280x720@144"),
            "{saved}"
        );

        // „Auto” czyści zapis.
        app.set_monitor_resolution("auto");
        assert!(app.displays.mode_for("eDP-1").is_none());

        std::env::remove_var("LUNA_DISPLAYS_CONF");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn night_light_updates_immediately_and_marks_the_config_dirty() {
        let mut app = app();
        app.set_night_light(45);
        assert_eq!(app.displays.night_light, 45);
        assert!(app.night_light_dirty);
    }

    #[test]
    fn desktop_wallpaper_rejects_a_missing_file() {
        let mut app = app();
        app.set_desktop_wallpaper(std::path::PathBuf::from("/tmp/__luna_brak_tapety__.png"));
        assert_eq!(app.status.message(), tr("The selected file does not exist"));
    }

    #[test]
    fn printer_name_comes_from_the_ipp_address() {
        assert_eq!(
            printer_name_from_uri("ipp://biuro.local:631/ipp/print").as_deref(),
            Some("biuro.local")
        );
        assert_eq!(
            printer_name_from_uri("ipps://user@10.0.0.5/ipp/print").as_deref(),
            Some("10.0.0.5")
        );
        assert_eq!(printer_name_from_uri("bez-adresu"), None);
    }

    #[test]
    fn five_quick_clicks_reveal_the_easter_egg() {
        let mut app = app();
        app.demo = true;
        for _ in 0..4 {
            app.note_about_click();
        }
        assert!(!app.easter_egg, "cztery kliknięcia to za mało");
        app.note_about_click();
        assert!(app.easter_egg, "piąte kliknięcie odkrywa jajko");
    }

    #[test]
    fn live_preview_is_restored_only_for_the_desktop_picker() {
        let mut app = app();
        // Ekran logowania nie zmienia pulpitu, więc nie ma czego przywracać.
        app.open_wallpaper_picker();
        {
            let picker = app.wallpaper_picker.as_mut().unwrap();
            picker.previewed = true;
            assert!(picker.restore_target().is_none());
        }
        app.cancel_wallpaper_picker();
        assert!(app.wallpaper_picker.is_none());

        app.open_desktop_wallpaper_picker();
        {
            let picker = app.wallpaper_picker.as_mut().unwrap();
            assert!(
                picker.restore_target().is_none(),
                "bez podglądu nie ruszamy wallpaper.conf"
            );
            picker.previewed = true;
            assert_eq!(picker.restore_target(), Some(picker.original.clone()));
        }
    }

    #[test]
    fn snapping_attaches_to_neighbour_edges() {
        let other = [(0, 0, 1920, 1080)];
        assert_eq!(snap_position(1926, 0, 1920, 1080, &other), (1920, 0));
        assert_eq!(snap_position(-1910, 0, 1920, 1080, &other), (-1920, 0));
        assert_eq!(snap_position(0, 1074, 1920, 1080, &other), (0, 1080));
        assert_eq!(snap_position(5000, 5000, 1920, 1080, &other), (5000, 5000));
    }

    fn demo_monitor(name: &str, width: u32, height: u32, x: i32, y: i32) -> pages::Monitor {
        pages::Monitor {
            name: name.to_owned(),
            connected: true,
            enabled: true,
            width,
            height,
            refresh: 60,
            x,
            y,
            modes: vec![OutputMode::new(width as u16, height as u16, 60)],
        }
    }

    #[test]
    fn dragging_a_monitor_moves_it_and_saves_the_layout() {
        let _env = lock_displays_env();
        let path =
            std::env::temp_dir().join(format!("luna-settings-drag-{}.conf", std::process::id()));
        let _ = std::fs::remove_file(&path);
        std::env::set_var("LUNA_DISPLAYS_CONF", &path);

        let mut app = app();
        app.displays = DisplaySettings::default();
        app.monitors = vec![
            demo_monitor("A", 1920, 1080, 0, 0),
            demo_monitor("B", 1920, 1080, 1920, 0),
        ];
        app.begin_monitor_drag(1, Point::new(100.0, 100.0), 0.5);
        // 400 px w lewo na ekranie = 800 px logicznie; z dala od krawędzi.
        app.drag_monitor(Point::new(-300.0, 100.0));
        assert_eq!((app.monitors[1].x, app.monitors[1].y), (1120, 0));
        app.end_monitor_drag();

        let saved = std::fs::read_to_string(&path).expect("displays.conf");
        assert!(saved.contains("output A position = 0,0"), "{saved}");
        assert!(saved.contains("output B position = 1120,0"), "{saved}");

        std::env::remove_var("LUNA_DISPLAYS_CONF");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn auto_arrange_lays_monitors_left_to_right() {
        let _env = lock_displays_env();
        let path =
            std::env::temp_dir().join(format!("luna-settings-arrange-{}.conf", std::process::id()));
        let _ = std::fs::remove_file(&path);
        std::env::set_var("LUNA_DISPLAYS_CONF", &path);

        let mut app = app();
        app.displays = DisplaySettings::default();
        app.monitors = vec![
            demo_monitor("A", 1920, 1080, 900, 400),
            demo_monitor("B", 2560, 1440, -100, 0),
        ];
        app.auto_arrange_monitors();
        assert_eq!((app.monitors[0].x, app.monitors[0].y), (0, 0));
        assert_eq!((app.monitors[1].x, app.monitors[1].y), (1920, 0));

        let saved = std::fs::read_to_string(&path).expect("displays.conf");
        assert!(saved.contains("output B position = 1920,0"), "{saved}");

        std::env::remove_var("LUNA_DISPLAYS_CONF");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn status_toast_hides_after_deadline() {
        let mut app = app();
        app.show_status("test");
        // Skracamy termin, żeby test nie czekał 3 s; dalej idziemy tą samą ścieżką co pętla.
        app.status_until = Some(std::time::Instant::now());
        let mut frames = 0;
        while app.status.is_animating() || app.status_until.is_some() {
            if app.status.is_animating() {
                app.status.advance(0.016, 2.5);
            }
            if let Some(until) = app.status_until {
                if std::time::Instant::now() >= until {
                    app.status.set_open_animated(false);
                    app.status_until = None;
                }
            }
            frames += 1;
            assert!(frames < 1000, "toast statusu nie zniknął");
        }
        assert!(!app.status.is_open());
        assert!(app.status.progress() < 0.01);
    }

    #[test]
    fn rejected_saved_secrets_open_the_password_prompt() {
        let mut app = app();
        app.wifi_retry = Some((
            "/dev/mock".to_owned(),
            "Dom".to_owned(),
            Security::Wpa2,
            false,
        ));
        app.finish_network(
            Err("secrets were required: the Wi-Fi password was rejected".to_owned()),
            true,
        );
        let prompt = app.wifi_prompt.as_ref().expect("formularz hasła");
        assert_eq!(prompt.ssid, "Dom");
        assert!(app.wifi_retry.is_none());
    }

    #[test]
    fn other_network_failure_does_not_open_the_prompt() {
        let mut app = app();
        app.wifi_retry = Some((
            "/dev/mock".to_owned(),
            "Dom".to_owned(),
            Security::Wpa2,
            false,
        ));
        app.finish_network(
            Err("NetworkManager: activation failed (reason 53)".to_owned()),
            true,
        );
        assert!(app.wifi_prompt.is_none());
        assert!(app.wifi_retry.is_none());
    }
}
