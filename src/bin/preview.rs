//! Headless-podglądy centrum ustawień do dokumentacji (bez kompozytora).
//!
//! Uruchom z katalogu repo:
//! `cargo run -p luna-settings --bin preview`
//! → `docs/images/luna-settings-*.png`.

use std::error::Error;

use libluna_core::{
    Color, DisplaySettings, OutputMode, Size, Theme, ThemePreference, ThemeSettings,
};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use luna::equalizer::AudioStatus;
use luna::network::{
    Connectivity, Device, DeviceKind, DeviceState, Ipv4Method, KnownWifi, Security, Snapshot,
    WifiNetwork, WiredConfig,
};
use luna::system::BatteryStatus;
use luna_settings::app::{AccountField, AccountMode, SettingsApp};
use luna_settings::pages::{Monitor, Page, PickerKind};
use luna_settings::ui;

const W: u32 = 1080;
const H: u32 = 760;

fn main() -> Result<(), Box<dyn Error>> {
    // Podglądy mają pokazywać także style z repo, niezależnie od instalacji systemowej.
    std::env::set_var(
        "LUNA_THEMES_DIR",
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/luna/themes"),
    );
    // Zrzuty nie mogą zależeć od osobistej tapety autora.
    std::env::set_var(
        "LUNA_WALLPAPER",
        "/usr/share/wallpapers/paganlinux/paganlinux-wallpaper.png",
    );
    // Ani od osobistych ustawień dostępności (skala tekstu, kontrast).
    std::env::set_var(
        "LUNA_ACCESSIBILITY_CONF",
        "/nonexistent/luna-accessibility.conf",
    );
    let icons = libluna_core::IconSettings::load();
    libluna_core::icons::set_active_icon_set(icons.external);
    libluna_core::icons::set_active_icon_theme(icons.theme);
    let theme = Theme::dark();
    let text = TextEngine::system();
    let settings = ThemeSettings::load();
    let mut app = SettingsApp::new(&settings);
    app.demo = true;
    app.system_ready = true;
    app.volume = Some((65, false));
    app.audio_sink = Some("Głośniki wbudowane".to_owned());
    app.brightness = Some(80);
    app.battery = Some(BatteryStatus {
        percent: 82,
        charging: false,
        minutes: Some(200),
    });
    app.dnd = false;
    app.power_saver = false;
    app.notification_daemon = "Luna 0.0.4 (libLuna)".to_owned();
    let demo_monitors = vec![
        Monitor {
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
                OutputMode::new(1920, 1080, 48),
                OutputMode::new(1680, 1050, 60),
                OutputMode::new(1280, 720, 60),
            ],
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
                OutputMode::new(1920, 1080, 120),
            ],
        },
    ];
    app.monitors = demo_monitors.clone();
    let mut displays = DisplaySettings::default();
    displays.set_mode("HDMI-A-1", Some(OutputMode::new(2560, 1440, 144)));
    displays.night_light = 20;
    app.displays = displays;
    let demo_network = Snapshot {
        available: true,
        error: None,
        wifi_enabled: true,
        connectivity: Connectivity::Full,
        devices: vec![
            Device {
                path: "/org/freedesktop/NetworkManager/Devices/2".to_owned(),
                interface: "enp9s0".to_owned(),
                kind: DeviceKind::Ethernet,
                state: DeviceState::Disconnected,
                driver: "r8169".to_owned(),
                hw_address: "A8:A1:59:18:3C:7A".to_owned(),
                connection: None,
                ip4: None,
                wired: Some(WiredConfig {
                    method: Ipv4Method::Manual,
                    address: "192.168.8.20/24".to_owned(),
                    gateway: "192.168.8.1".to_owned(),
                    dns: vec!["1.1.1.1".to_owned(), "8.8.8.8".to_owned()],
                    profile: Some("/org/freedesktop/NetworkManager/Settings/9".to_owned()),
                    autoconnect: true,
                }),
                metered: false,
            },
            Device {
                path: "/org/freedesktop/NetworkManager/Devices/3".to_owned(),
                interface: "wlp12s0f3u2".to_owned(),
                kind: DeviceKind::Wifi,
                state: DeviceState::Activated,
                driver: "rtw88_8821au".to_owned(),
                hw_address: "8C:86:DD:48:5C:3E".to_owned(),
                connection: Some("Dom".to_owned()),
                ip4: Some("192.168.8.9/24".to_owned()),
                wired: None,
                metered: false,
            },
        ],
        wifi: vec![
            WifiNetwork {
                ssid: "Dom".to_owned(),
                signal: 92,
                security: Security::Wpa2,
                active: true,
                device: "/org/freedesktop/NetworkManager/Devices/3".to_owned(),
                device_name: "wlp12s0f3u2".to_owned(),
                ap_path: "/org/freedesktop/NetworkManager/AccessPoint/4".to_owned(),
                frequency: 5220,
                known: true,
            },
            WifiNetwork {
                ssid: "PaganNet".to_owned(),
                signal: 74,
                security: Security::Wpa3,
                active: false,
                device: "/org/freedesktop/NetworkManager/Devices/3".to_owned(),
                device_name: "wlp12s0f3u2".to_owned(),
                ap_path: "/org/freedesktop/NetworkManager/AccessPoint/3".to_owned(),
                frequency: 2412,
                known: true,
            },
            WifiNetwork {
                ssid: "Kawiarnia".to_owned(),
                signal: 41,
                security: Security::Open,
                active: false,
                device: "/org/freedesktop/NetworkManager/Devices/3".to_owned(),
                device_name: "wlp12s0f3u2".to_owned(),
                ap_path: "/org/freedesktop/NetworkManager/AccessPoint/2".to_owned(),
                frequency: 2437,
                known: false,
            },
        ],
        known_wifi: vec![
            KnownWifi {
                ssid: "Dom".to_owned(),
                profile: "/org/freedesktop/NetworkManager/Settings/1".to_owned(),
                autoconnect: true,
            },
            KnownWifi {
                ssid: "PaganNet".to_owned(),
                profile: "/org/freedesktop/NetworkManager/Settings/2".to_owned(),
                autoconnect: true,
            },
            KnownWifi {
                ssid: "Pracownia".to_owned(),
                profile: "/org/freedesktop/NetworkManager/Settings/3".to_owned(),
                autoconnect: false,
            },
        ],
    };
    app.network = demo_network.clone();

    // Konta lokalne są zależne od maszyny — zrzut używa profili demo.
    app.accounts = vec![
        libluna_core::accounts::UserAccount {
            login: "patryks".to_owned(),
            full_name: "Patryk Szkudlarek".to_owned(),
            uid: 1000,
            gid: 1000,
            home: "/home/patryks".to_owned(),
            shell: "/bin/bash".to_owned(),
            admin: true,
            current: true,
        },
        libluna_core::accounts::UserAccount {
            login: "gosc".to_owned(),
            full_name: String::new(),
            uid: 1001,
            gid: 1001,
            home: "/home/gosc".to_owned(),
            shell: "/bin/sh".to_owned(),
            admin: false,
            current: false,
        },
    ];

    // Urządzenia audio zależą od maszyny — zrzut strony Dźwięk używa danych demo.
    let demo_audio = AudioStatus {
        sinks: vec![
            luna::equalizer::Device {
                id: 46,
                description: "Starship/Matisse HD Audio — Analogowe stereo".to_owned(),
                is_default: false,
            },
            luna::equalizer::Device {
                id: 57,
                description: "TU116 HDMI — Digital Stereo".to_owned(),
                is_default: false,
            },
        ],
        sources: vec![luna::equalizer::Device {
            id: 35,
            description: "SM900T Microphone".to_owned(),
            is_default: true,
        }],
        eq_id: Some(73),
    };

    // Nowe moduły: dane demo (zrzuty nie mogą zależeć od usług autora).
    app.bluetooth = luna::bluetooth::Snapshot {
        available: true,
        adapters: vec![luna::bluetooth::Adapter {
            path: "/org/bluez/hci0".to_owned(),
            name: "hci0".to_owned(),
            address: "A4:5E:60:12:34:56".to_owned(),
            powered: true,
            discovering: true,
        }],
        devices: vec![
            luna::bluetooth::Device {
                path: "/org/bluez/hci0/dev_58_AB_12_34_56_78".to_owned(),
                adapter: "/org/bluez/hci0".to_owned(),
                address: "58:AB:12:34:56:78".to_owned(),
                name: "Luna Headphones".to_owned(),
                paired: true,
                connected: true,
                trusted: true,
                rssi: Some(-42),
                icon: Some("audio-headphones".to_owned()),
            },
            luna::bluetooth::Device {
                path: "/org/bluez/hci0/dev_90_11_22_33_44_55".to_owned(),
                adapter: "/org/bluez/hci0".to_owned(),
                address: "90:11:22:33:44:55".to_owned(),
                name: "Klawiatura Pagan".to_owned(),
                paired: true,
                connected: false,
                trusted: false,
                rssi: Some(-61),
                icon: Some("input-keyboard".to_owned()),
            },
            luna::bluetooth::Device {
                path: "/org/bluez/hci0/dev_24_6F_AA_BB_CC_DD".to_owned(),
                adapter: "/org/bluez/hci0".to_owned(),
                address: "24:6F:AA:BB:CC:DD".to_owned(),
                name: "Telefon".to_owned(),
                paired: false,
                connected: false,
                trusted: false,
                rssi: Some(-78),
                icon: Some("phone".to_owned()),
            },
        ],
        error: None,
    };
    app.power_profiles = luna::power_profile::Profiles {
        available: true,
        active: "balanced".to_owned(),
        choices: vec![
            "power-saver".to_owned(),
            "balanced".to_owned(),
            "performance".to_owned(),
        ],
        error: None,
    };
    app.vpn = luna::vpn::Snapshot {
        available: true,
        profiles: vec![
            luna::vpn::VpnProfile {
                name: "Biuro (WireGuard)".to_owned(),
                uuid: "4f2a6a3e-1b1d-4c6e-9ab0-000000000001".to_owned(),
                path: "/org/freedesktop/NetworkManager/Settings/8".to_owned(),
                kind: "wireguard".to_owned(),
                active: true,
            },
            luna::vpn::VpnProfile {
                name: "Dom (OpenVPN)".to_owned(),
                uuid: "4f2a6a3e-1b1d-4c6e-9ab0-000000000002".to_owned(),
                path: "/org/freedesktop/NetworkManager/Settings/9".to_owned(),
                kind: "vpn".to_owned(),
                active: false,
            },
        ],
        error: None,
    };
    app.printers = luna::printers::Snapshot {
        available: true,
        printers: vec![luna::printers::Printer {
            name: "biuro.local".to_owned(),
            info: "is idle.".to_owned(),
            is_default: true,
            enabled: true,
            uri: Some("ipp://biuro.local:631/ipp/print".to_owned()),
        }],
        default: Some("biuro.local".to_owned()),
        error: None,
    };
    app.sessions = luna::sessions::Snapshot {
        available: true,
        sessions: vec![
            luna::sessions::Session {
                id: "2".to_owned(),
                user: "patryks".to_owned(),
                uid: 1000,
                seat: "seat0".to_owned(),
                kind: "wayland".to_owned(),
                active: true,
                current: true,
            },
            luna::sessions::Session {
                id: "4".to_owned(),
                user: "gosc".to_owned(),
                uid: 1001,
                seat: "seat0".to_owned(),
                kind: "wayland".to_owned(),
                active: false,
                current: false,
            },
        ],
        error: None,
    };
    app.colors = luna::colors::Snapshot {
        available: true,
        devices: vec![luna::colors::ColorDevice {
            id: "xrandr-HDMI-A-1".to_owned(),
            model: "HDMI-A-1".to_owned(),
            kind: "display".to_owned(),
            profiles: vec!["icc-srgb".to_owned()],
            enabled: true,
        }],
        profiles: vec![
            luna::colors::ColorProfile {
                id: "icc-srgb".to_owned(),
                title: "sRGB".to_owned(),
                filename: Some("/usr/share/color/icc/sRGB.icc".to_owned()),
                kind: Some("display".to_owned()),
            },
            luna::colors::ColorProfile {
                id: "icc-biuro".to_owned(),
                title: "Biuro — kalibracja".to_owned(),
                filename: Some("/home/patryks/.local/share/icc/biuro.icc".to_owned()),
                kind: Some("display".to_owned()),
            },
        ],
        error: None,
    };

    app.defaults = luna::default_apps::Defaults {
        available: true,
        apps: vec![
            luna::default_apps::DesktopApp {
                desktop_id: "firefox.desktop".to_owned(),
                name: "Firefox".to_owned(),
                exec: "firefox %u".to_owned(),
                categories: vec!["WebBrowser".to_owned()],
                mime_types: vec!["text/html".to_owned()],
            },
            luna::default_apps::DesktopApp {
                desktop_id: "thunderbird.desktop".to_owned(),
                name: "Thunderbird".to_owned(),
                exec: "thunderbird %u".to_owned(),
                categories: vec!["MailClient".to_owned()],
                mime_types: vec!["x-scheme-handler/mailto".to_owned()],
            },
            luna::default_apps::DesktopApp {
                desktop_id: "luna-files.desktop".to_owned(),
                name: "Luna Files".to_owned(),
                exec: "luna-files %U".to_owned(),
                categories: vec!["FileManager".to_owned()],
                mime_types: vec!["inode/directory".to_owned()],
            },
            luna::default_apps::DesktopApp {
                desktop_id: "loupe.desktop".to_owned(),
                name: "Loupe".to_owned(),
                exec: "loupe %U".to_owned(),
                categories: vec!["ImageViewer".to_owned()],
                mime_types: vec!["image/png".to_owned()],
            },
            luna::default_apps::DesktopApp {
                desktop_id: "celluloid.desktop".to_owned(),
                name: "Celluloid".to_owned(),
                exec: "celluloid %U".to_owned(),
                categories: vec!["VideoPlayer".to_owned()],
                mime_types: vec!["video/mp4".to_owned()],
            },
            luna::default_apps::DesktopApp {
                desktop_id: "lollypop.desktop".to_owned(),
                name: "Lollypop".to_owned(),
                exec: "lollypop".to_owned(),
                categories: vec!["AudioPlayer".to_owned()],
                mime_types: vec!["audio/mpeg".to_owned()],
            },
            luna::default_apps::DesktopApp {
                desktop_id: "papers.desktop".to_owned(),
                name: "Papers".to_owned(),
                exec: "papers %U".to_owned(),
                categories: vec!["Office".to_owned()],
                mime_types: vec!["application/pdf".to_owned()],
            },
            luna::default_apps::DesktopApp {
                desktop_id: "luna-terminal.desktop".to_owned(),
                name: "Luna Terminal".to_owned(),
                exec: "luna-terminal".to_owned(),
                categories: vec!["TextEditor".to_owned()],
                mime_types: vec!["text/plain".to_owned()],
            },
        ],
        current: vec![
            Some("firefox.desktop".to_owned()),
            Some("thunderbird.desktop".to_owned()),
            Some("luna-files.desktop".to_owned()),
            Some("loupe.desktop".to_owned()),
            Some("celluloid.desktop".to_owned()),
            Some("lollypop.desktop".to_owned()),
            Some("papers.desktop".to_owned()),
            Some("luna-terminal.desktop".to_owned()),
        ],
        error: None,
    };
    app.shortcuts = libluna_core::ShortcutSettings::default();

    for (name, page) in [
        ("appearance", Page::Appearance),
        ("notifications", Page::Notifications),
        ("language", Page::Language),
        ("login", Page::Login),
        ("lock-screen", Page::LockScreen),
        ("workspaces", Page::Workspaces),
        ("users", Page::Users),
        ("accessibility", Page::Accessibility),
        ("default-apps", Page::DefaultApps),
        ("network", Page::Network),
        ("bluetooth", Page::Bluetooth),
        ("input", Page::Input),
        ("shortcuts", Page::Shortcuts),
        ("displays", Page::Displays),
        ("sound", Page::Sound),
        ("power", Page::Power),
        ("printers", Page::Printers),
        ("system", Page::System),
        ("about", Page::About),
    ] {
        app.go_to(page);
        if page == Page::Network {
            // Monitor sieci w tle czytą prawdziwe dane — do zrzutów używamy demo.
            app.network = demo_network.clone();
        }
        if page == Page::Sound {
            app.audio = demo_audio.clone();
            app.eq_available = true;
            app.eq.enabled = true;
            app.eq.set_preset(luna::equalizer::Preset::Rock);
            app.eq_output_id = Some(46);
            app.audio_sink = Some("Luna Equalizer".to_owned());
            app.volume = Some((65, false));
        }
        if page == Page::Displays {
            // Strona odświeża monitory z /sys; zrzut ma pokazywać układ demo.
            app.monitors = demo_monitors.clone();
            app.selected_monitor = 1;
        }

        let mut scene = Scene::new();
        ui::paint(
            &app,
            &mut scene,
            Size::new(W as f64, H as f64),
            &theme,
            &text,
        );

        let mut renderer = SoftwareRenderer::new(W, H);
        renderer.draw(&scene, 1.0, Color::from_rgba8(0x1a, 0x1d, 0x27, 0xff));
        let png = renderer.encode_png().ok_or("failed to encode PNG")?;
        let path = format!("docs/images/luna-settings-{name}.png");
        std::fs::write(&path, png)?;
        println!("saved {path} ({W}x{H})");
    }

    // Nagrywanie skrótu (modal „Naciśnij kombinację”).
    app.go_to(Page::Shortcuts);
    app.shortcut_record = Some(libluna_core::ShortcutAction::Overview);
    let mut scene = Scene::new();
    ui::paint(
        &app,
        &mut scene,
        Size::new(W as f64, H as f64),
        &theme,
        &text,
    );
    let mut renderer = SoftwareRenderer::new(W, H);
    renderer.draw(&scene, 1.0, Color::from_rgba8(0x1a, 0x1d, 0x27, 0xff));
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write("docs/images/luna-settings-shortcut-record.png", png)?;
    println!("saved docs/images/luna-settings-shortcut-record.png ({W}x{H})");
    app.shortcut_record = None;

    // Easter egg: 5 szybkich kliknięć w wersję na „O Lunie”.
    app.easter_egg = true;
    app.go_to(Page::About);
    let mut scene = Scene::new();
    ui::paint(
        &app,
        &mut scene,
        Size::new(W as f64, H as f64),
        &theme,
        &text,
    );
    let mut renderer = SoftwareRenderer::new(W, H);
    renderer.draw(&scene, 1.0, Color::from_rgba8(0x1a, 0x1d, 0x27, 0xff));
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write("docs/images/luna-settings-about-secret.png", png)?;
    println!("saved docs/images/luna-settings-about-secret.png ({W}x{H})");
    app.easter_egg = false;

    // Dostępność: 150 % tekstu + wysoki kontrast (ten sam mechanizm co w sesji).
    app.go_to(Page::Accessibility);
    let mut accessible_settings = libluna_core::AccessibilitySettings::default();
    accessible_settings.text_scale = 150;
    accessible_settings.high_contrast = true;
    app.accessibility = accessible_settings;
    let accessible_theme = Theme::dark().with_accessibility(&app.accessibility);
    let mut scene = Scene::new();
    ui::paint(
        &app,
        &mut scene,
        Size::new(W as f64, H as f64),
        &accessible_theme,
        &text,
    );
    let mut renderer = SoftwareRenderer::new(W, H);
    renderer.draw(&scene, 1.0, Color::from_rgba8(0x00, 0x00, 0x00, 0xff));
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write("docs/images/luna-settings-accessibility-contrast.png", png)?;
    println!("saved docs/images/luna-settings-accessibility-contrast.png ({W}x{H})");
    app.accessibility = libluna_core::AccessibilitySettings::default();

    // Wersja jasna (sprawdza, czy strona nie polega na ciemnych kolorach).
    app.go_to(Page::Appearance);
    app.theme_pref = ThemePreference::Light;
    let light = Theme::light();
    let mut scene = Scene::new();
    ui::paint(
        &app,
        &mut scene,
        Size::new(W as f64, H as f64),
        &light,
        &text,
    );
    let mut renderer = SoftwareRenderer::new(W, H);
    renderer.draw(&scene, 1.0, Color::WHITE);
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write("docs/images/luna-settings-appearance-light.png", png)?;
    println!("saved docs/images/luna-settings-appearance-light.png ({W}x{H})");

    // Style z plików — dowód, że zmieniają całą paletę i geometrię, nie tylko akcent.
    for (name, style, preference) in [
        ("appearance-nord", "nord", ThemePreference::Dark),
        ("appearance-paper", "paper", ThemePreference::Light),
    ] {
        let mut styled = ThemeSettings::load();
        styled.set_style(Some(style.to_owned()));
        let styled_theme = styled.resolve_themed(false);
        app.go_to(Page::Appearance);
        app.style = Some(style.to_owned());
        app.theme_pref = preference;
        let mut scene = Scene::new();
        ui::paint(
            &app,
            &mut scene,
            Size::new(W as f64, H as f64),
            &styled_theme,
            &text,
        );
        let mut renderer = SoftwareRenderer::new(W, H);
        renderer.draw(&scene, 1.0, Color::from_rgba8(0x1a, 0x1d, 0x27, 0xff));
        let png = renderer.encode_png().ok_or("failed to encode PNG")?;
        let path = format!("docs/images/luna-settings-{name}.png");
        std::fs::write(&path, png)?;
        println!("saved {path} ({W}x{H})");
    }

    // Wyszukiwany wybór języka (splitbutton + lista z filtrem).
    app.go_to(Page::Language);
    app.open_picker(PickerKind::Language);
    let mut scene = Scene::new();
    ui::paint(
        &app,
        &mut scene,
        Size::new(W as f64, H as f64),
        &theme,
        &text,
    );
    let mut renderer = SoftwareRenderer::new(W, H);
    renderer.draw(&scene, 1.0, Color::from_rgba8(0x1a, 0x1d, 0x27, 0xff));
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write("docs/images/luna-settings-language-picker.png", png)?;
    println!("saved docs/images/luna-settings-language-picker.png ({W}x{H})");

    // Panel Info (techniczne szczegóły strony w dolnym panelu).
    app.picker = None;
    app.go_to(Page::Workspaces);
    app.open_info_sheet();
    app.info_sheet.set_open(true);
    let mut scene = Scene::new();
    ui::paint(
        &app,
        &mut scene,
        Size::new(W as f64, H as f64),
        &theme,
        &text,
    );
    let mut renderer = SoftwareRenderer::new(W, H);
    renderer.draw(&scene, 1.0, Color::from_rgba8(0x1a, 0x1d, 0x27, 0xff));
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write("docs/images/luna-settings-info-sheet.png", png)?;
    println!("saved docs/images/luna-settings-info-sheet.png ({W}x{H})");

    // Wybór tapety z dysku — siatka miniatur.
    app.info_sheet.set_open(false);
    app.go_to(Page::Login);
    app.open_wallpaper_picker();
    if let Some(picker) = &mut app.wallpaper_picker {
        let dir = std::path::PathBuf::from("/usr/share/wallpapers/paganlinux");
        if dir.is_dir() {
            picker.navigate(dir);
            picker.selected = 3;
        }
    }
    let mut scene = Scene::new();
    ui::paint(
        &app,
        &mut scene,
        Size::new(W as f64, H as f64),
        &theme,
        &text,
    );
    let mut renderer = SoftwareRenderer::new(W, H);
    renderer.draw(&scene, 1.0, Color::from_rgba8(0x1a, 0x1d, 0x27, 0xff));
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write("docs/images/luna-settings-wallpaper-picker.png", png)?;
    println!("saved docs/images/luna-settings-wallpaper-picker.png ({W}x{H})");

    // Arkusz „Więcej tapet” (źródła, losowa tapeta, przeglądanie dysku).
    if let Some(picker) = &mut app.wallpaper_picker {
        picker.more = true;
        picker.more_hover = Some(1);
    }
    let mut scene = Scene::new();
    ui::paint(
        &app,
        &mut scene,
        Size::new(W as f64, H as f64),
        &theme,
        &text,
    );
    let mut renderer = SoftwareRenderer::new(W, H);
    renderer.draw(&scene, 1.0, Color::from_rgba8(0x1a, 0x1d, 0x27, 0xff));
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write("docs/images/luna-settings-wallpaper-more.png", png)?;
    println!("saved docs/images/luna-settings-wallpaper-more.png ({W}x{H})");

    // Podgląd na żywo tapety pulpitu (flaga ustawiona ręcznie — zrzut nic nie zapisuje).
    app.wallpaper_picker = None;
    app.go_to(Page::Appearance);
    app.open_desktop_wallpaper_picker();
    if let Some(picker) = &mut app.wallpaper_picker {
        let dir = std::path::PathBuf::from("/usr/share/wallpapers/paganlinux");
        if dir.is_dir() {
            picker.navigate(dir);
            picker.selected = 3;
        }
        picker.previewed = true;
    }
    let mut scene = Scene::new();
    ui::paint(
        &app,
        &mut scene,
        Size::new(W as f64, H as f64),
        &theme,
        &text,
    );
    let mut renderer = SoftwareRenderer::new(W, H);
    renderer.draw(&scene, 1.0, Color::from_rgba8(0x1a, 0x1d, 0x27, 0xff));
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write("docs/images/luna-settings-wallpaper-live.png", png)?;
    println!("saved docs/images/luna-settings-wallpaper-live.png ({W}x{H})");
    app.wallpaper_picker = None;

    // Formularz IPv4 połączenia przewodowego.
    app.wallpaper_picker = None;
    app.go_to(Page::Network);
    app.network = demo_network;
    app.open_wired_editor("/org/freedesktop/NetworkManager/Devices/2".to_owned());
    let mut scene = Scene::new();
    ui::paint(
        &app,
        &mut scene,
        Size::new(W as f64, H as f64),
        &theme,
        &text,
    );
    let mut renderer = SoftwareRenderer::new(W, H);
    renderer.draw(&scene, 1.0, Color::from_rgba8(0x1a, 0x1d, 0x27, 0xff));
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write("docs/images/luna-settings-wired-ipv4.png", png)?;
    println!("saved docs/images/luna-settings-wired-ipv4.png ({W}x{H})");

    // Wybór rozdzielczości monitora.
    app.wired_editor = None;
    app.go_to(Page::Displays);
    app.monitors = demo_monitors;
    app.selected_monitor = 1;
    app.open_picker(PickerKind::Resolution);
    let mut scene = Scene::new();
    ui::paint(
        &app,
        &mut scene,
        Size::new(W as f64, H as f64),
        &theme,
        &text,
    );
    let mut renderer = SoftwareRenderer::new(W, H);
    renderer.draw(&scene, 1.0, Color::from_rgba8(0x1a, 0x1d, 0x27, 0xff));
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write("docs/images/luna-settings-resolution-picker.png", png)?;
    println!("saved docs/images/luna-settings-resolution-picker.png ({W}x{H})");

    // Formularz dodawania konta (tylko podgląd; nie uruchamia pomocnika).
    app.picker = None;
    app.go_to(Page::Users);
    app.open_account_form(AccountMode::AddUser);
    if let Some(form) = &mut app.account_form {
        form.login.set_text("nowak");
        form.full_name.set_text("Anna Nowak");
        form.password.set_text("••••••••");
        form.confirm.set_text("••••••••");
        form.focus = AccountField::Login;
    }
    let mut scene = Scene::new();
    ui::paint(
        &app,
        &mut scene,
        Size::new(W as f64, H as f64),
        &theme,
        &text,
    );
    let mut renderer = SoftwareRenderer::new(W, H);
    renderer.draw(&scene, 1.0, Color::from_rgba8(0x1a, 0x1d, 0x27, 0xff));
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write("docs/images/luna-settings-account-form.png", png)?;
    println!("saved docs/images/luna-settings-account-form.png ({W}x{H})");

    Ok(())
}
