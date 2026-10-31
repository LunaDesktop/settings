//! Rysowanie i obsługa wejścia centrum ustawień.
//!
//! Układ jest liczony w jednym miejscu ([`layout`]), więc trafienia klawiszy/myszy
//! i piksele na ekranie nie mogą się rozjechać. Kontrolki pochodzą z
//! `libluna-widgets` (`HeaderBar`, `NavList`, `PreferencesPage`, `Switch`, `Slider`…).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use libluna_core::style::weight;
use libluna_core::{ButtonState, Color, Event, IconTheme, Point, PointerButton, Rect, Size, Theme};
use libluna_i18n::tr;
use libluna_render::wallpaper::decode_png;
use libluna_render::{ImageData, Scene, TextEngine};
use libluna_widgets::{
    clipboard_command, paint_icon, ActionRow, Button, ButtonVariant, CheckButton, ControlsSide,
    Entry, HeaderBar, IconName, PaintCtx, PreferencesGroup, PreferencesPage, Slider, Switch,
    TextClipboardCommand, TitleAlign, ViewSwitcher, WindowControl,
};

use crate::app::{AccountField, AccountForm, SettingsApp, TextPasteTarget};
use crate::app::{WallpaperPicker, WallpaperTarget};
use crate::pages::{self, AccentTarget, Action, Control, Group, Page, SliderKind};

/// Wysokość wiersza ustawień (spójna dla rysowania i trafień).
pub const ROW_H: f64 = 56.0;

/// Wiersze rosną razem z tekstem (dostępność: większa skala bez nachodzenia).
fn row_height(theme: &Theme) -> f64 {
    let factor = (theme.typography.body.size / 14.0).clamp(1.0, 1.5);
    (ROW_H * factor).round()
}
/// Przewinięcie za jeden „ząbek” kółka.
const SCROLL_STEP: f64 = 64.0;
/// Jeden „ząbek” kółka w jednostkach powierzchni (spójne z `selene::seat::WHEEL_STEP_PX`).
/// Dzięki temu rolka przewija o stały krok także wtedy, gdy Selene wysyła delty
/// w pikselach (jeden ząbek = [`SCROLL_UNIT`] px), a nie ±1.
const SCROLL_UNIT: f64 = 20.0;
/// Szerokość sidebara.
const SIDEBAR_W: f64 = 236.0;

/// Miniatury tapet (dekodowane raz, przycięte do 16:9).
static THUMBS: OnceLock<Mutex<HashMap<PathBuf, Option<Arc<ImageData>>>>> = OnceLock::new();

fn wallpaper_thumbnail(path: &Path) -> Option<Arc<ImageData>> {
    let cache = THUMBS.get_or_init(Default::default);
    if let Ok(cache) = cache.lock() {
        if let Some(hit) = cache.get(path) {
            return hit.clone();
        }
    }
    let image = std::fs::read(path)
        .ok()
        .and_then(|bytes| decode_png(&bytes))
        .map(|image| Arc::new(cover_thumbnail(&image, 384, 216)));
    if let Ok(mut cache) = cache.lock() {
        cache.insert(path.to_path_buf(), image.clone());
    }
    image
}

/// Skaluje i przycina obraz „cover” do `width × height` (najbliższy sąsiad).
fn cover_thumbnail(image: &ImageData, width: u32, height: u32) -> ImageData {
    let mut out = vec![0u8; (width * height * 4) as usize];
    if image.width == 0 || image.height == 0 {
        return ImageData::new(width, height, out);
    }
    let scale = (width as f64 / image.width as f64).max(height as f64 / image.height as f64);
    let src_w = width as f64 / scale;
    let src_h = height as f64 / scale;
    let off_x = (image.width as f64 - src_w) / 2.0;
    let off_y = (image.height as f64 - src_h) / 2.0;
    for y in 0..height {
        for x in 0..width {
            let sx = (off_x + (x as f64 + 0.5) / scale).clamp(0.0, image.width as f64 - 1.0) as u32;
            let sy =
                (off_y + (y as f64 + 0.5) / scale).clamp(0.0, image.height as f64 - 1.0) as u32;
            let src = ((sy * image.width + sx) * 4) as usize;
            let dst = ((y * width + x) * 4) as usize;
            out[dst..dst + 4].copy_from_slice(&image.rgba[src..src + 4]);
        }
    }
    ImageData::new(width, height, out)
}

fn paint_wallpaper_control(
    ctx: &mut PaintCtx,
    control: Rect,
    path: &Path,
    selected: bool,
    theme: &Theme,
) {
    ctx.scene
        .fill_rounded_rect(control, theme.radius.small, theme.colors.shade);
    if let Some(image) = wallpaper_thumbnail(path) {
        ctx.scene.image(control, image);
    }
    ctx.scene.stroke_rounded_rect(
        control,
        theme.radius.small,
        if selected { 2.0 } else { 1.0 },
        if selected {
            theme.colors.accent
        } else {
            theme.colors.border
        },
    );
    if selected {
        let badge = Rect::from_xywh(control.right() - 20.0, control.y() + 4.0, 16.0, 16.0);
        ctx.scene
            .fill_rounded_rect(badge, badge.width() / 2.0, theme.colors.accent);
        paint_icon(
            ctx,
            IconName::Check,
            Rect::from_xywh(badge.x() + 3.0, badge.y() + 3.0, 10.0, 10.0),
            theme.colors.accent_fg,
        );
    }
}

/// Wysokość grafiki układu monitorów na stronie „Ekrany”.
const DISPLAYS_HERO_H: f64 = 156.0;

/// Obszar grafiki układu (tylko strona Ekrany z wykrytymi monitorami).
fn displays_hero_rect(app: &SettingsApp, viewport: Rect) -> Option<Rect> {
    (app.page == Page::Displays && !app.monitors.is_empty()).then(|| {
        Rect::from_xywh(
            viewport.x(),
            viewport.y(),
            viewport.width(),
            DISPLAYS_HERO_H.min(viewport.height() * 0.4),
        )
    })
}

/// Treść przewijana pod grafiką (hero zostaje na górze).
fn content_viewport(app: &SettingsApp, viewport: Rect) -> Rect {
    match displays_hero_rect(app, viewport) {
        Some(hero) => Rect::from_xywh(
            viewport.x(),
            hero.bottom() + 12.0,
            viewport.width(),
            (viewport.height() - hero.height() - 12.0).max(80.0),
        ),
        None => viewport,
    }
}

fn hero_inner(hero: Rect) -> Rect {
    Rect::from_xywh(
        hero.x() + 16.0,
        hero.y() + 34.0,
        (hero.width() - 32.0).max(1.0),
        (hero.height() - 48.0).max(1.0),
    )
}

fn monitor_tiles(app: &SettingsApp) -> Vec<pages::MonitorTile> {
    let selected = app
        .selected_monitor
        .min(app.monitors.len().saturating_sub(1));
    app.monitors
        .iter()
        .enumerate()
        .map(|(index, monitor)| pages::MonitorTile {
            name: monitor.name.clone(),
            width: monitor.width.max(1),
            height: monitor.height.max(1),
            x: monitor.x,
            y: monitor.y,
            enabled: monitor.enabled,
            selected: index == selected,
        })
        .collect()
}

/// Skala grafiki układu: piksele ekranu na jednostkę logiczną.
///
/// Liczona z aktualnych granic układu — używana przy chwytaniu monitora,
/// żeby przeciąganie odpowiadało ruchowi kursora mimo zmieniających się granic.
fn monitor_map_scale(control: Rect, tiles: &[pages::MonitorTile]) -> f64 {
    if tiles.is_empty() {
        return 1.0;
    }
    let min_x = tiles.iter().map(|tile| tile.x).min().unwrap_or(0);
    let min_y = tiles.iter().map(|tile| tile.y).min().unwrap_or(0);
    let max_x = tiles
        .iter()
        .map(|tile| tile.x + tile.width as i32)
        .max()
        .unwrap_or(1);
    let max_y = tiles
        .iter()
        .map(|tile| tile.y + tile.height as i32)
        .max()
        .unwrap_or(1);
    let bound_w = (max_x - min_x).max(1) as f64;
    let bound_h = (max_y - min_y).max(1) as f64;
    ((control.width() - 8.0) / bound_w)
        .min((control.height() - 8.0) / bound_h)
        .max(0.01)
}

/// Prostokąty monitorów na grafice układu — wspólne dla rysowania i trafień myszą.
fn monitor_tile_rects(control: Rect, tiles: &[pages::MonitorTile]) -> Vec<Rect> {
    if tiles.is_empty() {
        return Vec::new();
    }
    let min_x = tiles.iter().map(|tile| tile.x).min().unwrap_or(0);
    let min_y = tiles.iter().map(|tile| tile.y).min().unwrap_or(0);
    let max_x = tiles
        .iter()
        .map(|tile| tile.x + tile.width as i32)
        .max()
        .unwrap_or(1);
    let max_y = tiles
        .iter()
        .map(|tile| tile.y + tile.height as i32)
        .max()
        .unwrap_or(1);
    let bound_w = (max_x - min_x).max(1) as f64;
    let bound_h = (max_y - min_y).max(1) as f64;
    let pad = 4.0;
    let scale = ((control.width() - pad * 2.0) / bound_w)
        .min((control.height() - pad * 2.0) / bound_h)
        .max(0.01);
    let total_w = bound_w * scale;
    let total_h = bound_h * scale;
    let left = control.x() + (control.width() - total_w) / 2.0;
    let top = control.y() + (control.height() - total_h) / 2.0;
    tiles
        .iter()
        .map(|tile| {
            let x = left + (tile.x - min_x) as f64 * scale;
            let y = top + (tile.y - min_y) as f64 * scale;
            Rect::from_xywh(
                x,
                y,
                (tile.width.max(1) as f64 * scale).max(10.0),
                (tile.height.max(1) as f64 * scale).max(8.0),
            )
        })
        .collect()
}

/// Duża grafika układu monitorów: prostokąty w proporcjach ekranów, wybrany podświetlony.
fn paint_displays_hero(app: &SettingsApp, ctx: &mut PaintCtx, hero: Rect) {
    let theme = *ctx.theme;
    ctx.scene
        .fill_rounded_rect(hero, theme.radius.large, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(hero, theme.radius.large, 1.0, theme.colors.border);
    let hint = format!(
        "{} · {}",
        tr("Monitor layout"),
        tr("Drag screens to change the arrangement")
    );
    ctx.label_left(
        hero.x() + 16.0,
        hero.y() + 17.0,
        &hint,
        theme.typography.caption.size,
        weight::MEDIUM,
        theme.colors.dim_text,
    );

    let tiles = monitor_tiles(app);
    let dragged = app.monitor_drag.map(|drag| drag.index);
    for (index, (tile, rect)) in tiles
        .iter()
        .zip(monitor_tile_rects(hero_inner(hero), &tiles))
        .enumerate()
    {
        let fill = if dragged == Some(index) {
            theme.colors.accent.scale_alpha(0.16)
        } else {
            theme.colors.shade
        };
        ctx.scene.fill_rounded_rect(rect, theme.radius.medium, fill);
        let (border, width) = if tile.selected {
            (theme.colors.accent, 2.0)
        } else {
            (theme.colors.border, 1.0)
        };
        ctx.scene
            .stroke_rounded_rect(rect, theme.radius.medium, width, border);
        // Krótka podstawka — monitor „stoi na biurku”.
        ctx.scene.fill_rect(
            Rect::from_xywh(rect.center().x - 10.0, rect.bottom() + 5.0, 20.0, 3.0),
            border.scale_alpha(0.65),
        );

        let name_size = theme.typography.body.size;
        let caption = theme.typography.caption.size;
        let name = fit_text(
            &tile.name,
            rect.width() - 12.0,
            name_size,
            weight::MEDIUM,
            ctx.text,
        );
        ctx.label_center(
            rect.center().x,
            rect.center().y - 8.0,
            &name,
            name_size,
            weight::MEDIUM,
            if tile.selected {
                theme.colors.accent
            } else {
                theme.colors.text
            },
        );
        let detail = format!("{} × {}", tile.width.max(1), tile.height.max(1));
        let detail = fit_text(
            &detail,
            rect.width() - 12.0,
            caption,
            weight::NORMAL,
            ctx.text,
        );
        ctx.label_center(
            rect.center().x,
            rect.center().y + 12.0,
            &detail,
            caption,
            weight::NORMAL,
            theme.colors.dim_text,
        );
    }
}

/// Podgląd motywu: miniaturowe okno w barwach akcentu i kilka ikon.
fn paint_theme_preview(
    ctx: &mut PaintCtx,
    rect: Rect,
    accent: libluna_core::Accent,
    icons: &[IconName],
    theme: &Theme,
    icon_theme: IconTheme,
) {
    let radius = theme.radius.medium;
    ctx.scene
        .fill_rounded_rect(rect, radius, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(rect, radius, 1.0, theme.colors.border);

    let bar = Rect::from_xywh(rect.x() + 6.0, rect.y() + 6.0, 6.0, rect.height() - 12.0);
    ctx.scene.fill_rounded_rect(bar, 3.0, accent.color());

    let win = Rect::from_xywh(rect.x() + 20.0, rect.y() + 8.0, 76.0, rect.height() - 16.0);
    ctx.scene
        .fill_rounded_rect(win, theme.radius.small, theme.colors.shade);
    ctx.scene.fill_rect(
        Rect::from_xywh(win.x() + 8.0, win.y() + 8.0, 30.0, 3.0),
        theme.colors.text.scale_alpha(0.55),
    );
    for index in 0..2 {
        ctx.scene.fill_rect(
            Rect::from_xywh(
                win.x() + 8.0,
                win.y() + 17.0 + index as f64 * 7.0,
                44.0 - index as f64 * 10.0,
                3.0,
            ),
            theme.colors.dim_text.scale_alpha(0.5),
        );
    }
    ctx.scene.fill_rounded_rect(
        Rect::from_xywh(win.x() + 8.0, win.bottom() - 14.0, 24.0, 8.0),
        theme.radius.small,
        accent.color(),
    );

    let shown = icons.len().min(4) as f64;
    let size = 18.0;
    let gap = 8.0;
    let total = shown * size + (shown - 1.0).max(0.0) * gap;
    let left = rect.right() - 8.0 - total;
    let mono = icon_theme.is_monochrome() || libluna_core::icons::active_icon_set().is_some();
    for (index, icon) in icons.iter().take(4).enumerate() {
        let r = Rect::from_xywh(
            left + index as f64 * (size + gap),
            rect.center().y - size / 2.0,
            size,
            size,
        );
        if mono {
            paint_icon(ctx, *icon, r, theme.colors.text);
        } else {
            ctx.scene
                .fill_rounded_rect(r, theme.radius.small, accent.color().scale_alpha(0.16));
            paint_icon(
                ctx,
                *icon,
                Rect::from_xywh(r.x() + 4.0, r.y() + 4.0, 10.0, 10.0),
                accent.color(),
            );
        }
    }
}

/// Prostokąty okna.
pub struct Layout {
    pub header: Rect,
    pub sidebar: Rect,
    pub nav: Rect,
    pub content: Rect,
    pub title: Rect,
    pub viewport: Rect,
}

pub fn layout(size: Size, theme: &Theme) -> Layout {
    let header = Rect::from_xywh(0.0, 0.0, size.width, theme.metrics.headerbar.height);
    let sidebar_w = SIDEBAR_W.min((size.width * 0.32).max(200.0));
    let sidebar = Rect::from_xywh(
        0.0,
        header.bottom(),
        sidebar_w,
        (size.height - header.height()).max(0.0),
    );
    let nav = Rect::from_xywh(
        sidebar.x() + 8.0,
        sidebar.y() + 8.0,
        (sidebar.width() - 16.0).max(0.0),
        (sidebar.height() - 40.0).max(0.0),
    );
    let content = Rect::from_xywh(
        sidebar.right(),
        header.bottom(),
        (size.width - sidebar.width()).max(0.0),
        (size.height - header.height()).max(0.0),
    );
    let pad = 32.0_f64.min((content.width() * 0.08).max(16.0));
    let title = Rect::from_xywh(
        content.x() + pad,
        content.y() + 20.0,
        (content.width() - 2.0 * pad).max(0.0),
        52.0,
    );
    let viewport = Rect::from_xywh(
        title.x(),
        title.bottom() + 8.0,
        title.width(),
        (content.bottom() - 24.0 - (title.bottom() + 8.0)).max(0.0),
    );
    Layout {
        header,
        sidebar,
        nav,
        content,
        title,
        viewport,
    }
}

/// Prostokąt listy stron przesunięty o przewinięcie sidebara.
fn nav_content(app: &SettingsApp, lay: &Layout) -> Rect {
    Rect::from_xywh(
        lay.nav.x(),
        lay.nav.y() - app.nav_scroll,
        lay.nav.width(),
        lay.nav.height(),
    )
}

/// Przewija sidebar tak, żeby wybrana strona była widoczna.
fn reveal_nav(app: &mut SettingsApp, lay: &Layout, theme: &Theme) {
    let Some(selected) = app.nav.selected() else {
        return;
    };
    let content = nav_content(app, lay);
    let Some(row) = app
        .nav
        .rows(content, theme)
        .into_iter()
        .find(|row| row.entry == selected)
    else {
        return;
    };
    let max = (app.nav.height(theme) - lay.nav.height()).max(0.0);
    if row.rect.y() < lay.nav.y() {
        app.nav_scroll = (app.nav_scroll + row.rect.y() - lay.nav.y()).max(0.0);
    } else if row.rect.bottom() > lay.nav.bottom() {
        app.nav_scroll = (app.nav_scroll + row.rect.bottom() - lay.nav.bottom()).min(max);
    }
}

/// Prostokąt przycisku „Info” w nagłówku (na lewo od kontrolek okna).
fn info_button_rect(lay: &Layout, theme: &Theme) -> Rect {
    let content = HeaderBar::new()
        .controls(ControlsSide::Right)
        .controls_layout(lay.header, theme)
        .content;
    let size = theme
        .metrics
        .headerbar
        .control
        .min((lay.header.height() - 8.0).max(20.0));
    Rect::from_xywh(
        (content.right() - size - 4.0).max(content.x()),
        lay.header.center().y - size / 2.0,
        size,
        size,
    )
}

/// Rysuje całą klatkę.
pub fn paint(app: &SettingsApp, scene: &mut Scene, size: Size, theme: &Theme, text: &TextEngine) {
    let lay = layout(size, theme);
    let mut ctx = PaintCtx::new(scene, theme, text);

    ctx.scene
        .fill_rect(Rect::from_size(size), theme.colors.window_bg);
    // Sidebar.
    ctx.scene.fill_rect(lay.sidebar, theme.colors.headerbar_bg);
    ctx.scene.fill_rect(
        Rect::from_xywh(
            lay.sidebar.right() - 1.0,
            lay.sidebar.y(),
            1.0,
            lay.sidebar.height(),
        ),
        theme.colors.border,
    );
    ctx.label_left(
        lay.sidebar.x() + 16.0,
        lay.sidebar.bottom() - 14.0,
        &format!("Luna {}", env!("CARGO_PKG_VERSION")),
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );

    // Nagłówek okna (kontrolki po prawej).
    HeaderBar::new()
        .title(app.title.clone())
        .title_align(TitleAlign::Start)
        .controls(ControlsSide::Right)
        .paint(&mut ctx, lay.header, app.hover_control);

    // Przejrzysty dostęp do technicznych informacji bieżącej strony.
    let info_rect = info_button_rect(&lay, theme);
    let info_bg = if app.info_sheet.is_open() {
        theme.colors.accent.scale_alpha(0.24)
    } else if app.info_hover {
        theme.colors.shade
    } else {
        theme.colors.shade.scale_alpha(0.65)
    };
    ctx.scene
        .fill_rounded_rect(info_rect, theme.radius.small, info_bg);
    let info_color = if app.info_sheet.is_open() {
        theme.colors.accent
    } else {
        theme.colors.text
    };
    let icon_size = (info_rect.width() * 0.72).round();
    paint_icon(
        &mut ctx,
        IconName::Info,
        Rect::from_xywh(
            info_rect.center().x - icon_size / 2.0,
            info_rect.center().y - icon_size / 2.0,
            icon_size,
            icon_size,
        ),
        info_color,
    );

    // Nawigacja.
    ctx.scene.push_clip(lay.nav);
    app.nav.paint(&mut ctx, nav_content(app, &lay));
    ctx.scene.pop_clip();
    let nav_max = (app.nav.height(theme) - lay.nav.height()).max(0.0);
    if nav_max > 0.5 {
        let track = Rect::from_xywh(
            lay.sidebar.right() - 5.0,
            lay.nav.y(),
            3.0,
            lay.nav.height(),
        );
        let total = app.nav.height(theme);
        let knob_h = (lay.nav.height() * lay.nav.height() / total).max(24.0);
        let y = lay.nav.y()
            + (lay.nav.height() - knob_h) * (app.nav_scroll.clamp(0.0, nav_max) / nav_max);
        ctx.scene.fill_rounded_rect(track, 1.5, theme.colors.shade);
        ctx.scene.fill_rounded_rect(
            Rect::from_xywh(track.x(), y, 3.0, knob_h),
            1.5,
            theme.colors.dim_text.scale_alpha(0.55),
        );
    }

    // Nagłówek strony.
    let icon = Rect::from_xywh(lay.title.x(), lay.title.y() + 9.0, 22.0, 22.0);
    let page_icon_color = if app.icons.theme.is_monochrome() {
        theme.colors.text
    } else {
        theme.colors.accent
    };
    paint_icon(&mut ctx, app.page.icon(), icon, page_icon_color);
    let page_title = tr(app.page.title_key());
    let page_subtitle = tr(app.page.subtitle_key());
    ctx.label_left(
        lay.title.x() + 32.0,
        lay.title.y() + 20.0,
        &page_title,
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );
    ctx.label_left(
        lay.title.x() + 32.0,
        lay.title.y() + 42.0,
        &page_subtitle,
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );

    // Zawartość strony.
    let groups = pages::content_groups(app);
    let page = preferences_page(&groups);
    let row_h = row_height(theme);
    let viewport = content_viewport(app, lay.viewport);
    let mut scroll = app.scroll;
    scroll.set_content_height(page.content_height(row_h, theme));
    let offset = scroll.offset().min(scroll.max_offset(viewport));
    scroll.set_offset(offset, viewport);
    let area = Rect::from_xywh(
        viewport.x(),
        viewport.y() - offset,
        viewport.width(),
        scroll.content_height().max(viewport.height()),
    );

    if let Some(hero) = displays_hero_rect(app, lay.viewport) {
        paint_displays_hero(app, &mut ctx, hero);
    }

    ctx.scene.push_clip(viewport);
    let contents = page.paint(&mut ctx, area, row_h);
    for (group, content) in groups.iter().zip(contents.iter()) {
        for (index, row) in group.rows.iter().enumerate() {
            let rect = PreferencesGroup::row_rect(*content, index, row_h);
            paint_row(
                &mut ctx,
                row,
                rect,
                index + 1 == group.rows.len(),
                app.icons.theme,
            );
        }
    }
    ctx.scene.pop_clip();
    scroll.paint_scrollbar(&mut ctx, viewport);

    if app.info_sheet.is_open() || app.info_sheet.progress() > 0.001 {
        app.info_sheet.paint(&mut ctx, size);
        paint_info_sheet(app, &mut ctx, size);
    }
    if app.picker.is_some() {
        paint_picker(app, &mut ctx, size);
    }
    if app.editor.is_some() {
        paint_editor(app, &mut ctx, size);
    }
    if app.wallpaper_picker.is_some() {
        paint_wallpaper_picker(app, &mut ctx, size);
    }
    if app.wifi_prompt.is_some() {
        paint_wifi_prompt(app, &mut ctx, size);
    }
    if app.wired_editor.is_some() {
        paint_wired_editor(app, &mut ctx, size);
    }
    if app.admin.is_some() {
        paint_admin(app, &mut ctx, size);
    }
    if app.account_form.is_some() {
        paint_account(app, &mut ctx, size);
    }
    if app.shortcut_record.is_some() {
        paint_shortcut_record(app, &mut ctx, size);
    }
    app.status.paint(&mut ctx, size);
}

fn preferences_page(groups: &[Group]) -> PreferencesPage {
    let mut page = PreferencesPage::new();
    for group in groups {
        page = page.add(PreferencesGroup::new(group.title.clone()), group.rows.len());
    }
    page
}

fn paint_row(ctx: &mut PaintCtx, row: &pages::Row, rect: Rect, last: bool, icon_theme: IconTheme) {
    let theme = *ctx.theme;
    if !last {
        ctx.scene.fill_rect(
            Rect::from_xywh(
                rect.x() + theme.metrics.group.pad_x,
                rect.bottom() - 1.0,
                (rect.width() - 2.0 * theme.metrics.group.pad_x).max(0.0),
                1.0,
            ),
            theme.colors.border.scale_alpha(0.55),
        );
    }

    let width = control_width(&row.control, &theme, ctx.text);
    let text_w =
        (rect.width() - 2.0 * theme.metrics.row.pad_x - width - theme.metrics.row.gap).max(0.0);
    let title = fit_text(
        &row.title,
        text_w,
        theme.typography.body.size,
        weight::NORMAL,
        ctx.text,
    );
    let subtitle = row.subtitle.as_ref().map(|subtitle| {
        fit_text(
            subtitle,
            text_w,
            theme.typography.caption.size,
            weight::NORMAL,
            ctx.text,
        )
    });
    let mut action_row = ActionRow::new(title);
    if let Some(subtitle) = subtitle {
        action_row = action_row.subtitle(subtitle);
    }
    let suffix = action_row.paint(ctx, rect, width);
    let control = control_rect(&row.control, rect, &theme, ctx.text);
    match &row.control {
        Control::None => {}
        Control::Value(value) => ctx.label_right(
            suffix.right(),
            rect.center().y,
            value,
            theme.typography.body.size,
            weight::NORMAL,
            theme.colors.text,
        ),
        Control::Switch(on) => Switch::new(*on).paint(ctx, control),
        Control::Slider { value, .. } => {
            let percent = (value * 100.0).round() as i64;
            ctx.label_right(
                suffix.right(),
                rect.y() + 16.0,
                &format!("{percent}%"),
                theme.typography.caption.size,
                weight::MEDIUM,
                theme.colors.dim_text,
            );
            Slider::new(*value).paint(ctx, control);
        }
        Control::Segmented {
            options, selected, ..
        } => {
            let mut switcher = ViewSwitcher::new(options.clone());
            switcher.set_selected(*selected);
            switcher.paint(ctx, control);
        }
        Control::Accents { selected, .. } => {
            for (index, swatch) in accent_rects(control, &theme).iter().enumerate() {
                ctx.scene.fill_rounded_rect(
                    *swatch,
                    swatch.width() / 2.0,
                    libluna_core::Accent::ALL[index].color(),
                );
                if index == *selected {
                    let ring = Rect::from_xywh(
                        swatch.x() - 2.0,
                        swatch.y() - 2.0,
                        swatch.width() + 4.0,
                        swatch.height() + 4.0,
                    );
                    ctx.scene
                        .stroke_rounded_rect(ring, ring.width() / 2.0, 2.0, theme.colors.text);
                }
            }
        }
        Control::Check(checked) => {
            let mut check = CheckButton::new(String::new());
            check.set_checked(*checked);
            check.paint(ctx, control);
        }
        Control::Select { value } => {
            let button = Button::new(value.clone()).dropdown(true);
            button.paint(ctx, control);
        }
        Control::IconStrip(icons) => {
            let mono = icon_theme.is_monochrome();
            let external = libluna_core::icons::active_icon_set().is_some();
            let size = 20.0;
            let gap = 8.0;
            for (index, icon) in icons.iter().enumerate() {
                let r = Rect::from_xywh(
                    control.x() + index as f64 * (size + gap),
                    control.center().y - size / 2.0,
                    size,
                    size,
                );
                if external {
                    // Zestaw zewnętrzny ma własne kolory — bez tła akcentu.
                    paint_icon(ctx, *icon, r, theme.colors.text);
                } else if mono {
                    paint_icon(ctx, *icon, r, theme.colors.text);
                } else {
                    ctx.scene.fill_rounded_rect(
                        r,
                        theme.radius.small,
                        theme.colors.accent.scale_alpha(0.16),
                    );
                    let inner = Rect::from_xywh(r.x() + 4.0, r.y() + 4.0, 12.0, 12.0);
                    paint_icon(ctx, *icon, inner, theme.colors.accent);
                }
            }
        }
        Control::ThemePreview { accent, icons } => {
            paint_theme_preview(ctx, control, *accent, icons, &theme, icon_theme);
        }
        Control::Wallpaper { path, selected } => {
            paint_wallpaper_control(ctx, control, path, *selected, &theme);
        }
        Control::Button { label, suggested } => {
            let button = Button::new(label.clone()).variant(if *suggested {
                ButtonVariant::Suggested
            } else {
                ButtonVariant::Default
            });
            button.paint(ctx, control);
        }
        Control::Buttons {
            labels, suggested, ..
        } => {
            for (index, rect) in buttons_rects(labels, suffix, &theme, ctx.text)
                .into_iter()
                .enumerate()
            {
                let button = Button::new(labels[index].clone()).variant(if index == *suggested {
                    ButtonVariant::Suggested
                } else {
                    ButtonVariant::Default
                });
                button.paint(ctx, rect);
            }
        }
    }
}

fn fit_text(text: &str, max_w: f64, size: f64, weight: u16, engine: &TextEngine) -> String {
    if max_w <= 0.0 || engine.measure(text, size, weight).width <= max_w {
        return text.to_owned();
    }
    let dots = "…";
    let dots_w = engine.measure(dots, size, weight).width;
    let mut out = String::new();
    for ch in text.chars() {
        let mut probe = out.clone();
        probe.push(ch);
        if engine.measure(&probe, size, weight).width + dots_w > max_w {
            break;
        }
        out = probe;
    }
    out.push('…');
    out
}

fn control_width(control: &Control, theme: &Theme, text: &TextEngine) -> f64 {
    match control {
        Control::None => 0.0,
        Control::Value(value) => {
            text.measure(value, theme.typography.body.size, weight::NORMAL)
                .width
        }
        Control::Switch(on) => Switch::new(*on).size(theme).0,
        Control::Slider { .. } => 220.0,
        Control::Segmented { options, .. } => {
            let caption = theme.typography.caption.size;
            let max = options
                .iter()
                .map(|option| text.measure(option, caption, weight::MEDIUM).width)
                .fold(0.0, f64::max);
            ((max + 24.0) * options.len() as f64).clamp(180.0, 340.0)
        }
        Control::Accents { .. } => {
            let n = libluna_core::Accent::ALL.len() as f64;
            n * 20.0 + (n - 1.0) * 8.0
        }
        Control::Check(_) => theme.metrics.check.size,
        Control::Select { value } => Button::new(value.clone())
            .dropdown(true)
            .width(text, theme)
            .clamp(160.0, 320.0),
        Control::IconStrip(icons) => {
            let n = icons.len() as f64;
            n * 20.0 + (n - 1.0).max(0.0) * 8.0
        }
        Control::ThemePreview { .. } => 248.0,
        Control::Wallpaper { .. } => 96.0,
        Control::Button { label, .. } => Button::new(label.clone()).width(text, theme),
        Control::Buttons { labels, .. } => {
            const GAP: f64 = 8.0;
            let total: f64 = labels
                .iter()
                .map(|label| Button::new(label.clone()).width(text, theme))
                .sum();
            total + GAP * labels.len().saturating_sub(1) as f64
        }
    }
}

/// Prostokąty kolejnych przycisków w wierszu `Control::Buttons` (od lewej do prawej).
fn buttons_rects(labels: &[String], area: Rect, theme: &Theme, text: &TextEngine) -> Vec<Rect> {
    const GAP: f64 = 8.0;
    let mut rects = Vec::with_capacity(labels.len());
    let mut right = area.right();
    for label in labels.iter().rev() {
        let button = Button::new(label.clone());
        let width = button.width(text, theme);
        let height = button.height(theme);
        let rect = Rect::from_xywh(right - width, area.center().y - height / 2.0, width, height);
        rects.push(rect);
        right = rect.x() - GAP;
    }
    rects.reverse();
    rects
}

fn control_rect(control: &Control, rect: Rect, theme: &Theme, text: &TextEngine) -> Rect {
    let width = control_width(control, theme, text);
    let suffix = Rect::from_xywh(
        rect.right() - theme.metrics.row.pad_x - width,
        rect.y(),
        width,
        rect.height(),
    );
    match control {
        Control::Switch(on) => {
            let (w, h) = Switch::new(*on).size(theme);
            Rect::from_xywh(suffix.right() - w, suffix.center().y - h / 2.0, w, h)
        }
        Control::Slider { .. } => Rect::from_xywh(
            suffix.x(),
            suffix.center().y + 8.0,
            suffix.width(),
            theme.metrics.slider.track_height,
        ),
        Control::Segmented { .. } => {
            Rect::from_xywh(suffix.x(), suffix.center().y - 14.0, suffix.width(), 28.0)
        }
        Control::Accents { .. } => {
            let h = 20.0;
            Rect::from_xywh(suffix.x(), suffix.center().y - h / 2.0, suffix.width(), h)
        }
        Control::Check(_) => {
            let size = theme.metrics.check.size;
            Rect::from_xywh(
                suffix.right() - size,
                suffix.center().y - size / 2.0,
                size,
                size,
            )
        }
        Control::Button { label, .. } => {
            let button = Button::new(label.clone());
            let h = button.height(theme);
            Rect::from_xywh(
                suffix.right() - width,
                suffix.center().y - h / 2.0,
                width,
                h,
            )
        }
        Control::Buttons { .. } => {
            let h = theme.metrics.button.height;
            Rect::from_xywh(suffix.x(), suffix.center().y - h / 2.0, width, h)
        }
        Control::Select { value } => {
            let button = Button::new(value.clone()).dropdown(true);
            let h = button.height(theme);
            Rect::from_xywh(
                suffix.right() - width,
                suffix.center().y - h / 2.0,
                width,
                h,
            )
        }
        Control::IconStrip(_) => Rect::from_xywh(
            suffix.right() - width,
            suffix.center().y - 12.0,
            width,
            24.0,
        ),
        Control::ThemePreview { .. } => Rect::from_xywh(
            suffix.right() - width,
            suffix.center().y - 26.0,
            width,
            52.0,
        ),
        Control::Wallpaper { .. } => Rect::from_xywh(
            suffix.right() - width,
            suffix.center().y - 27.0,
            width,
            54.0,
        ),
        Control::None | Control::Value(_) => suffix,
    }
}

fn accent_rects(strip: Rect, _theme: &Theme) -> Vec<Rect> {
    let size = 20.0;
    let gap = 8.0;
    let n = libluna_core::Accent::ALL.len();
    let total = n as f64 * size + (n as f64 - 1.0) * gap;
    let left = strip.right() - total;
    (0..n)
        .map(|i| {
            Rect::from_xywh(
                left + i as f64 * (size + gap),
                strip.center().y - size / 2.0,
                size,
                size,
            )
        })
        .collect()
}

/// Trafienie wiersza strony.
struct RowHit<'a> {
    row: &'a pages::Row,
    rect: Rect,
    control: Rect,
}

/// Układa wiersze aktywnej strony (z uwzględnieniem przewinięcia).
fn row_hits<'a>(
    groups: &'a [Group],
    viewport: Rect,
    theme: &Theme,
    text: &TextEngine,
    offset: f64,
) -> Vec<RowHit<'a>> {
    let page = preferences_page(groups);
    let area = Rect::from_xywh(
        viewport.x(),
        viewport.y() - offset,
        viewport.width(),
        page.content_height(row_height(theme), theme)
            .max(viewport.height()),
    );
    let mut out = Vec::new();
    for (group, group_rect) in groups
        .iter()
        .zip(page.group_rects(area, row_height(theme), theme))
    {
        let content = Rect::from_xywh(
            group_rect.x(),
            group_rect.y() + theme.metrics.group.header,
            group_rect.width(),
            (group_rect.height() - theme.metrics.group.header).max(0.0),
        );
        for (index, row) in group.rows.iter().enumerate() {
            let rect = PreferencesGroup::row_rect(content, index, row_height(theme));
            let control = control_rect(&row.control, rect, theme, text);
            out.push(RowHit { row, rect, control });
        }
    }
    out
}

/// Obsługuje wejście.
pub fn handle_events(
    app: &mut SettingsApp,
    events: &[Event],
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    apply_paste(app, events);
    // Nagrywanie skrótu przechwytuje klawisze przed wszystkim innym.
    if app.shortcut_record.is_some() {
        handle_shortcut_record(app, events);
        return;
    }
    // Modalny formularz/picker/edytor przechwytuje wejście — pod spodem nic nie reaguje.
    if app.admin.is_some() {
        handle_admin_events(app, events, size, theme);
        return;
    }
    if app.wifi_prompt.is_some() {
        handle_wifi_events(app, events, size, theme);
        return;
    }
    if app.wired_editor.is_some() {
        handle_wired_events(app, events, size, theme);
        return;
    }
    if app.wallpaper_picker.is_some() {
        handle_wallpaper_events(app, events, size, theme);
        return;
    }
    if app.picker.is_some() {
        handle_picker_events(app, events, size, theme);
        return;
    }
    if app.editor.is_some() {
        handle_editor_events(app, events, size, theme);
        return;
    }
    if app.account_form.is_some() {
        handle_account_events(app, events, size, theme);
        return;
    }
    if app.info_sheet.is_open() {
        handle_info_events(app, events, size, theme, text);
        return;
    }

    let lay = layout(size, theme);
    if app.nav_reveal {
        app.nav_reveal = false;
        reveal_nav(app, &lay, theme);
    }
    let header_bar = HeaderBar::new().controls(ControlsSide::Right);
    let groups = pages::content_groups(app);
    let page = preferences_page(&groups);
    app.scroll
        .set_content_height(page.content_height(row_height(theme), theme));
    app.scroll.set_offset(app.scroll.offset(), lay.viewport);

    for event in events {
        match event {
            Event::PointerMotion { position, .. } => {
                if app.monitor_drag.is_some() {
                    app.drag_monitor(*position);
                }
                app.hover_control = header_bar.control_at(lay.header, theme, *position);
                app.info_hover = info_button_rect(&lay, theme).contains(*position);
                if let Some((kind, track)) = app.drag {
                    let value = Slider::new(0.0).value_at(track, *position);
                    let percent = (value * 100.0).round() as u8;
                    app.on_action(match kind {
                        SliderKind::Volume => Action::SetVolume(percent),
                        SliderKind::Brightness => Action::SetBrightness(percent),
                        SliderKind::NightLight => Action::SetNightLight(percent),
                        SliderKind::EqPreamp => Action::SetEqPreamp(SliderKind::eq_db(percent)),
                        SliderKind::EqBand(index) => {
                            Action::SetEqBand(index, SliderKind::eq_db(percent))
                        }
                    });
                }
            }
            Event::PointerLeave => {
                app.hover_control = None;
                app.info_hover = false;
                app.drag = None;
            }
            Event::PointerButton(button) if button.state == ButtonState::Pressed => {
                if button.button != PointerButton::Left {
                    continue;
                }
                let p = button.position;
                if let Some(control) = header_bar.control_at(lay.header, theme, p) {
                    match control {
                        WindowControl::Minimize => app.pending.minimize = true,
                        WindowControl::Maximize => app.pending.maximize = true,
                        WindowControl::Close => app.pending.close = true,
                    }
                    continue;
                }
                let info_rect = info_button_rect(&lay, theme);
                if info_rect.contains(p) {
                    if app.info_sheet.is_open() {
                        app.close_info_sheet();
                    } else {
                        app.open_info_sheet();
                    }
                    continue;
                }
                if lay.header.contains(p) {
                    app.pending.begin_move = true;
                    continue;
                }
                if lay.nav.contains(p) {
                    if app.nav.click_at(nav_content(app, &lay), p, theme) {
                        if let Some(page) = app
                            .nav
                            .selected()
                            .and_then(|index| app.nav_pages.get(index).copied().flatten())
                        {
                            app.go_to(page);
                        }
                    }
                    continue;
                }
                if !lay.viewport.contains(p) {
                    continue;
                }
                if let Some(hero) = displays_hero_rect(app, lay.viewport) {
                    if hero.contains(p) {
                        let tiles = monitor_tiles(app);
                        let inner = hero_inner(hero);
                        let scale = monitor_map_scale(inner, &tiles);
                        for (index, rect) in monitor_tile_rects(inner, &tiles).iter().enumerate() {
                            if rect.contains(p) {
                                app.on_action(Action::SelectMonitor(index));
                                app.begin_monitor_drag(index, p, scale);
                            }
                        }
                        continue;
                    }
                }

                let content = content_viewport(app, lay.viewport);
                let hits = row_hits(&groups, content, theme, text, app.scroll.offset());
                for hit in hits {
                    handle_click(app, hit, p, theme, text);
                }
            }
            Event::PointerButton(button) if button.state == ButtonState::Released => {
                if button.button == PointerButton::Left {
                    app.drag = None;
                    app.end_monitor_drag();
                }
            }
            Event::Scroll(scroll) if lay.nav.contains(scroll.position) => {
                let max = (app.nav.height(theme) - lay.nav.height()).max(0.0);
                app.nav_scroll = (app.nav_scroll + scroll.dy / SCROLL_UNIT * 26.0).clamp(0.0, max);
            }
            Event::Scroll(scroll)
                if content_viewport(app, lay.viewport).contains(scroll.position) =>
            {
                let content = content_viewport(app, lay.viewport);
                app.scroll
                    .on_scroll(scroll.dy / SCROLL_UNIT * SCROLL_STEP, content);
            }
            Event::Scroll(_) => {}
            _ => {}
        }
    }
}

/// Rozdziela wklejony tekst do pola, które zgłosiło Ctrl+V.
fn apply_paste(app: &mut SettingsApp, events: &[Event]) {
    for event in events {
        let Event::Paste(text) = event else {
            continue;
        };
        let Some(target) = app.paste_target.take() else {
            break;
        };
        match target {
            TextPasteTarget::PickerQuery => picker_insert(app, text),
            TextPasteTarget::EditorEntry => editor_insert(app, text),
            TextPasteTarget::AdminPassword => admin_insert(app, text),
            TextPasteTarget::WifiPassword => wifi_insert(app, text),
            TextPasteTarget::WiredEntry(field) => wired_insert(app, field, text),
            TextPasteTarget::AccountEntry(field) => account_insert(app, field, text),
        }
    }
}

fn apply_text_clipboard(app: &mut SettingsApp, cmd: TextClipboardCommand, target: TextPasteTarget) {
    match cmd {
        TextClipboardCommand::Copy(text) | TextClipboardCommand::Cut(text) => {
            if let Some(text) = text {
                app.clipboard_text = Some(text);
            }
        }
        TextClipboardCommand::Paste => {
            app.paste_target = Some(target);
            app.paste_requested = true;
        }
    }
}

fn handle_click(app: &mut SettingsApp, hit: RowHit, p: Point, theme: &Theme, text: &TextEngine) {
    match &hit.row.control {
        Control::None | Control::Value(_) => {
            if hit.row.action != Action::None && hit.rect.contains(p) {
                app.on_action(hit.row.action.clone());
            }
        }
        Control::Select { .. } => {
            if hit.control.contains(p) || hit.rect.contains(p) {
                app.on_action(hit.row.action.clone());
            }
        }
        Control::IconStrip(_) => {}
        Control::ThemePreview { .. } => {}
        Control::Switch(_) => {
            // Klik w cały wiersz jest wygodniejszy niż trafianie w mały przełącznik.
            if hit.rect.contains(p) || hit.control.contains(p) {
                app.on_action(hit.row.action.clone());
            }
        }
        Control::Slider { kind, value } => {
            let slider = Slider::new(*value);
            if slider.hit(hit.control, p, theme) {
                let percent = (slider.value_at(hit.control, p) * 100.0).round() as u8;
                app.drag = Some((*kind, hit.control));
                app.on_action(match kind {
                    SliderKind::Volume => Action::SetVolume(percent),
                    SliderKind::Brightness => Action::SetBrightness(percent),
                    SliderKind::NightLight => Action::SetNightLight(percent),
                    SliderKind::EqPreamp => Action::SetEqPreamp(SliderKind::eq_db(percent)),
                    SliderKind::EqBand(index) => {
                        Action::SetEqBand(*index, SliderKind::eq_db(percent))
                    }
                });
            }
        }
        Control::Segmented {
            options, actions, ..
        } => {
            let switcher = ViewSwitcher::new(options.clone());
            if let Some(index) = switcher.index_at(hit.control, p) {
                if let Some(action) = actions.get(index) {
                    app.on_action(action.clone());
                }
            }
        }
        Control::Accents { target, .. } => {
            for (index, swatch) in accent_rects(hit.control, theme).iter().enumerate() {
                if swatch.contains(p) {
                    let accent = libluna_core::Accent::ALL[index];
                    app.on_action(match target {
                        AccentTarget::Desktop => Action::SetAccent(accent),
                        AccentTarget::Greeter => Action::SetGreeterAccent(accent),
                        AccentTarget::Lock => Action::SetLockAccent(accent),
                    });
                    break;
                }
            }
        }
        Control::Check(_) => {
            // Cały wiersz jest wygodnym celem — tak jak przy przełączniku.
            if hit.rect.contains(p) || hit.control.contains(p) {
                app.on_action(hit.row.action.clone());
            }
        }
        Control::Wallpaper { .. } => {
            if hit.rect.contains(p) || hit.control.contains(p) {
                app.on_action(hit.row.action.clone());
            }
        }
        Control::Button { .. } => {
            let button = match &hit.row.control {
                Control::Button { label, .. } => Button::new(label.clone()),
                _ => return,
            };
            if button.click_at(hit.control, p) {
                app.on_action(hit.row.action.clone());
            }
        }
        Control::Buttons {
            labels, actions, ..
        } => {
            for (index, rect) in buttons_rects(labels, hit.control, theme, text)
                .into_iter()
                .enumerate()
            {
                if rect.contains(p) {
                    if let Some(action) = actions.get(index) {
                        app.on_action(action.clone());
                    }
                    break;
                }
            }
        }
    }
}

// ── Wyszukiwany wybór (picker) i mały edytor tekstu ──

const PICKER_ROW_H: f64 = 32.0;

struct PickerLayout {
    card: Rect,
    title: Rect,
    search: Rect,
    list: Rect,
    hint: Rect,
}

fn picker_layout(size: Size, _theme: &Theme) -> PickerLayout {
    let w = (size.width - 120.0).clamp(320.0, 560.0);
    let h = (size.height - 140.0).clamp(260.0, 480.0);
    let card = Rect::from_xywh((size.width - w) / 2.0, (size.height - h) / 2.0, w, h);
    let title = Rect::from_xywh(card.x() + 16.0, card.y() + 12.0, card.width() - 32.0, 30.0);
    let search = Rect::from_xywh(
        card.x() + 16.0,
        title.bottom() + 8.0,
        card.width() - 32.0,
        36.0,
    );
    let hint = Rect::from_xywh(
        card.x() + 16.0,
        card.bottom() - 30.0,
        card.width() - 32.0,
        20.0,
    );
    let list = Rect::from_xywh(
        card.x() + 8.0,
        search.bottom() + 8.0,
        card.width() - 16.0,
        (hint.y() - search.bottom() - 16.0).max(0.0),
    );
    PickerLayout {
        card,
        title,
        search,
        list,
        hint,
    }
}

fn picker_row_at(app: &SettingsApp, lay: &PickerLayout, p: Point) -> Option<usize> {
    if !lay.list.contains(p) {
        return None;
    }
    let picker = app.picker.as_ref()?;
    let index = ((p.y - lay.list.y() + picker.scroll) / PICKER_ROW_H).floor() as usize;
    (index < app.picker_filtered().len()).then_some(index)
}

fn picker_max_scroll(app: &SettingsApp, lay: &PickerLayout) -> f64 {
    let count = app.picker_filtered().len() as f64;
    (count * PICKER_ROW_H - lay.list.height()).max(0.0)
}

fn picker_insert(app: &mut SettingsApp, text: &str) {
    let plain: String = text.chars().filter(|c| !c.is_control()).collect();
    if plain.is_empty() {
        return;
    }
    if let Some(picker) = &mut app.picker {
        picker.query.insert_str(&plain);
        picker.selected = 0;
        picker.scroll = 0.0;
    }
}

fn picker_move(app: &mut SettingsApp, lay: &PickerLayout, delta: i64) {
    let count = app.picker_filtered().len();
    if count == 0 {
        return;
    }
    let Some(picker) = &mut app.picker else {
        return;
    };
    let next = (picker.selected as i64 + delta).clamp(0, count as i64 - 1) as usize;
    picker.selected = next;
    let top = next as f64 * PICKER_ROW_H;
    if top < picker.scroll {
        picker.scroll = top;
    } else if top + PICKER_ROW_H > picker.scroll + lay.list.height() {
        picker.scroll = (top + PICKER_ROW_H - lay.list.height()).max(0.0);
    }
}

fn handle_picker_events(app: &mut SettingsApp, events: &[Event], size: Size, theme: &Theme) {
    let lay = picker_layout(size, theme);
    for event in events {
        match event {
            Event::PointerMotion { position, .. } => {
                app.picker_hover = picker_row_at(app, &lay, *position);
            }
            Event::PointerLeave => app.picker_hover = None,
            Event::PointerButton(button)
                if button.state == ButtonState::Pressed && button.button == PointerButton::Left =>
            {
                let p = button.position;
                if !lay.card.contains(p) {
                    app.picker = None;
                    continue;
                }
                if lay.search.contains(p) {
                    if let Some(picker) = &mut app.picker {
                        picker.query.set_focused(true);
                    }
                } else if let Some(index) = picker_row_at(app, &lay, p) {
                    if let Some(picker) = &mut app.picker {
                        picker.selected = index;
                    }
                    app.apply_picker();
                }
            }
            Event::Scroll(scroll) if lay.list.contains(scroll.position) => {
                let max = picker_max_scroll(app, &lay);
                let step = scroll.dy / SCROLL_UNIT * PICKER_ROW_H * 2.0;
                if let Some(picker) = &mut app.picker {
                    picker.scroll = (picker.scroll + step).clamp(0.0, max);
                }
            }
            Event::Key(key) if key.state == ButtonState::Pressed => {
                let clipboard = app
                    .picker
                    .as_mut()
                    .and_then(|picker| clipboard_command(&mut picker.query, key));
                if let Some(cmd) = clipboard {
                    apply_text_clipboard(app, cmd, TextPasteTarget::PickerQuery);
                    continue;
                }
                match key.keysym {
                    0xff1b => app.picker = None,
                    0xff0d | 0xff8d => app.apply_picker(),
                    0xff52 => picker_move(app, &lay, -1),
                    0xff54 => picker_move(app, &lay, 1),
                    0xff50 => picker_move(app, &lay, -(app.picker_filtered().len() as i64)),
                    0xff57 => picker_move(app, &lay, app.picker_filtered().len() as i64),
                    0xff08 => {
                        if let Some(picker) = &mut app.picker {
                            picker.query.backspace();
                            picker.selected = 0;
                            picker.scroll = 0.0;
                        }
                    }
                    0x61 if key.modifiers.ctrl => {
                        if let Some(picker) = &mut app.picker {
                            picker.query.select_all();
                        }
                    }
                    _ => {
                        if !key.modifiers.ctrl {
                            if let Some(text) = &key.text {
                                picker_insert(app, text);
                            }
                        }
                    }
                }
            }
            Event::Text(text) => picker_insert(app, text),
            _ => {}
        }
    }
}

fn paint_picker(app: &SettingsApp, ctx: &mut PaintCtx, size: Size) {
    let Some(picker) = &app.picker else {
        return;
    };
    let theme = *ctx.theme;
    let lay = picker_layout(size, &theme);
    ctx.scene
        .fill_rect(Rect::from_size(size), Color::rgba(0.0, 0.0, 0.0, 0.45));
    ctx.scene
        .fill_rounded_rect(lay.card, theme.radius.large, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(lay.card, theme.radius.large, 1.0, theme.colors.border);

    let title = match picker.kind {
        pages::PickerKind::Language => tr("Interface language"),
        pages::PickerKind::Locale => tr("Region and formats"),
        pages::PickerKind::Timezone => tr("Time zone"),
        pages::PickerKind::IconSet => tr("External icon set"),
        pages::PickerKind::Resolution => tr("Resolution"),
        pages::PickerKind::Refresh => tr("Refresh rate"),
        pages::PickerKind::ColorProfile => tr("Color profile"),
        pages::PickerKind::DefaultApp => tr("Default application"),
    };
    ctx.label_left(
        lay.title.x(),
        lay.title.center().y,
        &title,
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );

    let mut query = picker.query.clone();
    query.set_focused(true);
    query.paint(ctx, lay.search);

    let filtered = app.picker_filtered();
    if filtered.is_empty() {
        ctx.label_center(
            lay.list.center().x,
            lay.list.center().y,
            &tr("No results"),
            theme.typography.body.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
    } else {
        ctx.scene.push_clip(lay.list);
        let current = app.picker_current_value(picker.kind);
        for (index, item) in filtered.iter().enumerate() {
            let row = Rect::from_xywh(
                lay.list.x() + 4.0,
                lay.list.y() + index as f64 * PICKER_ROW_H - picker.scroll,
                lay.list.width() - 8.0,
                PICKER_ROW_H,
            );
            if row.bottom() < lay.list.y() || row.y() > lay.list.bottom() {
                continue;
            }
            if index == picker.selected {
                ctx.scene.fill_rounded_rect(
                    row,
                    theme.radius.small,
                    theme.colors.accent.scale_alpha(0.18),
                );
                ctx.scene
                    .stroke_rounded_rect(row, theme.radius.small, 1.5, theme.colors.accent);
            } else if app.picker_hover == Some(index) {
                ctx.scene.fill_rounded_rect(
                    row,
                    theme.radius.small,
                    theme.colors.shade.scale_alpha(1.8),
                );
            }
            ctx.label_left(
                row.x() + 10.0,
                row.center().y,
                &item.label,
                theme.typography.body.size,
                weight::NORMAL,
                theme.colors.text,
            );
            if item.value == current {
                let check = Rect::from_xywh(row.right() - 22.0, row.center().y - 7.0, 14.0, 14.0);
                paint_icon(ctx, IconName::Check, check, theme.colors.accent);
            }
        }
        ctx.scene.pop_clip();
    }

    ctx.label_center(
        lay.hint.center().x,
        lay.hint.center().y,
        &tr("↑↓ select · Enter confirm · Esc close"),
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
}

struct EditorLayout {
    card: Rect,
    title: Rect,
    entry: Rect,
    save: Rect,
    cancel: Rect,
}

fn editor_layout(size: Size, theme: &Theme) -> EditorLayout {
    let w = (size.width - 160.0).clamp(320.0, 520.0);
    let h = 190.0_f64.min(size.height - 80.0);
    let card = Rect::from_xywh((size.width - w) / 2.0, (size.height - h) / 2.0, w, h);
    let title = Rect::from_xywh(card.x() + 20.0, card.y() + 14.0, card.width() - 40.0, 30.0);
    let entry = Rect::from_xywh(
        card.x() + 20.0,
        title.bottom() + 10.0,
        card.width() - 40.0,
        36.0,
    );
    let button_h = theme.metrics.button.height;
    let save = Rect::from_xywh(
        card.right() - 20.0 - 96.0,
        card.bottom() - 16.0 - button_h,
        96.0,
        button_h,
    );
    let cancel = Rect::from_xywh(save.x() - 8.0 - 96.0, save.y(), 96.0, button_h);
    EditorLayout {
        card,
        title,
        entry,
        save,
        cancel,
    }
}

fn editor_insert(app: &mut SettingsApp, text: &str) {
    let plain: String = text.chars().filter(|c| !c.is_control()).collect();
    if plain.is_empty() {
        return;
    }
    if let Some(editor) = &mut app.editor {
        editor.entry.insert_str(&plain);
    }
}

fn handle_editor_events(app: &mut SettingsApp, events: &[Event], size: Size, theme: &Theme) {
    let lay = editor_layout(size, theme);
    for event in events {
        match event {
            Event::PointerButton(button)
                if button.state == ButtonState::Pressed && button.button == PointerButton::Left =>
            {
                let p = button.position;
                if lay.save.contains(p) {
                    app.save_editor();
                } else if lay.cancel.contains(p) || !lay.card.contains(p) {
                    app.editor = None;
                } else if lay.entry.contains(p) {
                    if let Some(editor) = &mut app.editor {
                        editor.entry.set_focused(true);
                    }
                }
            }
            Event::Key(key) if key.state == ButtonState::Pressed => {
                let clipboard = app
                    .editor
                    .as_mut()
                    .and_then(|editor| clipboard_command(&mut editor.entry, key));
                if let Some(cmd) = clipboard {
                    apply_text_clipboard(app, cmd, TextPasteTarget::EditorEntry);
                    continue;
                }
                match key.keysym {
                    0xff1b => app.editor = None,
                    0xff0d | 0xff8d => app.save_editor(),
                    0xff08 => {
                        if let Some(editor) = &mut app.editor {
                            editor.entry.backspace();
                        }
                    }
                    0x61 if key.modifiers.ctrl => {
                        if let Some(editor) = &mut app.editor {
                            editor.entry.select_all();
                        }
                    }
                    _ => {
                        if !key.modifiers.ctrl {
                            if let Some(text) = &key.text {
                                editor_insert(app, text);
                            }
                        }
                    }
                }
            }
            Event::Text(text) => editor_insert(app, text),
            _ => {}
        }
    }
}

struct AdminLayout {
    card: Rect,
    title: Rect,
    subtitle: Rect,
    entry: Rect,
    error: Rect,
    save: Rect,
    cancel: Rect,
}

fn admin_layout(size: Size, theme: &Theme) -> AdminLayout {
    let w = (size.width - 160.0).clamp(360.0, 560.0);
    let h = 244.0_f64.min(size.height - 80.0);
    let card = Rect::from_xywh((size.width - w) / 2.0, (size.height - h) / 2.0, w, h);
    let title = Rect::from_xywh(card.x() + 20.0, card.y() + 14.0, card.width() - 40.0, 28.0);
    let subtitle = Rect::from_xywh(
        card.x() + 20.0,
        title.bottom() + 2.0,
        card.width() - 40.0,
        20.0,
    );
    let entry = Rect::from_xywh(
        card.x() + 20.0,
        subtitle.bottom() + 12.0,
        card.width() - 40.0,
        36.0,
    );
    let error = Rect::from_xywh(
        card.x() + 20.0,
        entry.bottom() + 6.0,
        card.width() - 40.0,
        18.0,
    );
    let button_h = theme.metrics.button.height;
    let save = Rect::from_xywh(
        card.right() - 20.0 - 96.0,
        card.bottom() - 16.0 - button_h,
        96.0,
        button_h,
    );
    let cancel = Rect::from_xywh(save.x() - 8.0 - 96.0, save.y(), 96.0, button_h);
    AdminLayout {
        card,
        title,
        subtitle,
        entry,
        error,
        save,
        cancel,
    }
}

fn admin_insert(app: &mut SettingsApp, text: &str) {
    let plain: String = text.chars().filter(|c| !c.is_control()).collect();
    if plain.is_empty() {
        return;
    }
    if let Some(prompt) = &mut app.admin {
        if !prompt.is_pending() {
            prompt.password.insert_str(&plain);
        }
    }
}

fn handle_admin_events(app: &mut SettingsApp, events: &[Event], size: Size, theme: &Theme) {
    let lay = admin_layout(size, theme);
    for event in events {
        match event {
            Event::PointerButton(button)
                if button.state == ButtonState::Pressed && button.button == PointerButton::Left =>
            {
                let p = button.position;
                if lay.save.contains(p) {
                    app.submit_admin_password();
                } else if lay.cancel.contains(p) || !lay.card.contains(p) {
                    app.admin = None;
                } else if lay.entry.contains(p) {
                    if let Some(prompt) = &mut app.admin {
                        prompt.password.set_focused(true);
                    }
                }
            }
            Event::Key(key) if key.state == ButtonState::Pressed => {
                let clipboard = app
                    .admin
                    .as_mut()
                    .filter(|prompt| !prompt.is_pending())
                    .and_then(|prompt| clipboard_command(&mut prompt.password, key));
                if let Some(cmd) = clipboard {
                    apply_text_clipboard(app, cmd, TextPasteTarget::AdminPassword);
                    continue;
                }
                match key.keysym {
                    0xff1b => app.admin = None,
                    0xff0d | 0xff8d => app.submit_admin_password(),
                    0xff08 => {
                        if let Some(prompt) = &mut app.admin {
                            if !prompt.is_pending() {
                                prompt.password.backspace();
                            }
                        }
                    }
                    0x61 if key.modifiers.ctrl => {
                        if let Some(prompt) = &mut app.admin {
                            if !prompt.is_pending() {
                                prompt.password.select_all();
                            }
                        }
                    }
                    _ => {
                        if !key.modifiers.ctrl {
                            if let Some(text) = &key.text {
                                admin_insert(app, text);
                            }
                        }
                    }
                }
            }
            Event::Text(text) => admin_insert(app, text),
            _ => {}
        }
    }
}

fn paint_admin(app: &SettingsApp, ctx: &mut PaintCtx, size: Size) {
    let Some(prompt) = &app.admin else {
        return;
    };
    let theme = *ctx.theme;
    let lay = admin_layout(size, &theme);
    ctx.scene
        .fill_rect(Rect::from_size(size), Color::rgba(0.0, 0.0, 0.0, 0.45));
    ctx.scene
        .fill_rounded_rect(lay.card, theme.radius.large, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(lay.card, theme.radius.large, 1.0, theme.colors.border);
    ctx.label_left(
        lay.title.x(),
        lay.title.center().y,
        &tr("Administrator password"),
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );
    ctx.label_left(
        lay.subtitle.x(),
        lay.subtitle.center().y,
        &tr("Changing the login screen requires administrator approval"),
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
    let mut password = prompt.password.clone();
    password.set_focused(true);
    password.paint(ctx, lay.entry);
    if let Some(error) = &prompt.error {
        ctx.label_left(
            lay.error.x(),
            lay.error.center().y,
            error,
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.warning,
        );
    } else if prompt.is_pending() {
        ctx.label_left(
            lay.error.x(),
            lay.error.center().y,
            &tr("Applying changes…"),
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
    }
    Button::new(tr("Apply changes"))
        .variant(ButtonVariant::Suggested)
        .paint(ctx, lay.save);
    Button::new(tr("Cancel")).paint(ctx, lay.cancel);
}

/// Układ formularza konta (modal wyśrodkowany).
struct AccountLayout {
    card: Rect,
    title: Rect,
    subtitle: Rect,
    fields: Vec<(AccountField, Rect)>,
    error: Rect,
    submit: Rect,
    cancel: Rect,
}

fn account_layout(size: Size, theme: &Theme, form: &AccountForm) -> AccountLayout {
    let field_h = 38.0;
    let gap = 8.0;
    let w = (size.width - 160.0).clamp(380.0, 560.0);
    let fields = form.fields();
    let button_h = theme.metrics.button.height;
    let needed = 16.0
        + 28.0
        + 2.0
        + 18.0
        + 12.0
        + fields.len() as f64 * (field_h + gap)
        + 20.0
        + button_h
        + 16.0;
    let h = needed.min(size.height - 60.0);
    let card = Rect::from_xywh((size.width - w) / 2.0, (size.height - h) / 2.0, w, h);
    let title = Rect::from_xywh(card.x() + 20.0, card.y() + 14.0, card.width() - 40.0, 28.0);
    let subtitle = Rect::from_xywh(
        card.x() + 20.0,
        title.bottom() + 2.0,
        card.width() - 40.0,
        18.0,
    );
    let mut y = subtitle.bottom() + 12.0;
    let mut rects = Vec::new();
    for field in fields {
        rects.push((
            field,
            Rect::from_xywh(card.x() + 20.0, y, card.width() - 40.0, field_h),
        ));
        y += field_h + gap;
    }
    let error = Rect::from_xywh(card.x() + 20.0, y, card.width() - 40.0, 18.0);
    let submit = Rect::from_xywh(
        card.right() - 20.0 - 120.0,
        card.bottom() - 16.0 - button_h,
        120.0,
        button_h,
    );
    let cancel = Rect::from_xywh(submit.x() - 8.0 - 96.0, submit.y(), 96.0, button_h);
    AccountLayout {
        card,
        title,
        subtitle,
        fields: rects,
        error,
        submit,
        cancel,
    }
}

fn account_insert(app: &mut SettingsApp, field: AccountField, text: &str) {
    let plain: String = text.chars().filter(|c| !c.is_control()).collect();
    if plain.is_empty() {
        return;
    }
    if let Some(form) = &mut app.account_form {
        form.entry_mut(field).insert_str(&plain);
    }
}

fn handle_account_events(app: &mut SettingsApp, events: &[Event], size: Size, theme: &Theme) {
    let Some(form) = &app.account_form else {
        return;
    };
    let lay = account_layout(size, theme, form);
    let pending = form.is_pending();
    for event in events {
        match event {
            Event::PointerButton(button)
                if button.state == ButtonState::Pressed && button.button == PointerButton::Left =>
            {
                let p = button.position;
                if lay.submit.contains(p) {
                    app.submit_account_form();
                } else if lay.cancel.contains(p) || !lay.card.contains(p) {
                    if !pending {
                        app.account_form = None;
                    }
                } else if let Some((field, _)) =
                    lay.fields.iter().find(|(_, rect)| rect.contains(p))
                {
                    if let Some(form) = &mut app.account_form {
                        form.focus = *field;
                        form.sync_focus();
                    }
                }
            }
            Event::Key(key) if key.state == ButtonState::Pressed => {
                if pending {
                    continue;
                }
                let focus = app.account_form.as_ref().map(|form| form.focus);
                let clipboard = match (app.account_form.as_mut(), focus) {
                    (Some(form), Some(focus)) => clipboard_command(form.entry_mut(focus), key),
                    _ => None,
                };
                if let Some(cmd) = clipboard {
                    if let Some(focus) = focus {
                        apply_text_clipboard(app, cmd, TextPasteTarget::AccountEntry(focus));
                    }
                    continue;
                }
                match key.keysym {
                    0xff1b => app.account_form = None,
                    0xff0d | 0xff8d => app.submit_account_form(),
                    0xff09 => {
                        if let Some(form) = &mut app.account_form {
                            form.cycle_focus(key.modifiers.shift);
                        }
                    }
                    0xff08 => {
                        if let (Some(form), Some(focus)) = (&mut app.account_form, focus) {
                            form.entry_mut(focus).backspace();
                        }
                    }
                    0x61 if key.modifiers.ctrl => {
                        if let (Some(form), Some(focus)) = (&mut app.account_form, focus) {
                            form.entry_mut(focus).select_all();
                        }
                    }
                    _ => {
                        if !key.modifiers.ctrl {
                            if let (Some(text), Some(focus)) = (&key.text, focus) {
                                account_insert(app, focus, text);
                            }
                        }
                    }
                }
            }
            Event::Text(text) => {
                if !pending {
                    if let Some(focus) = app.account_form.as_ref().map(|form| form.focus) {
                        account_insert(app, focus, text);
                    }
                }
            }
            _ => {}
        }
    }
}

fn paint_account(app: &SettingsApp, ctx: &mut PaintCtx, size: Size) {
    let Some(form) = &app.account_form else {
        return;
    };
    let theme = *ctx.theme;
    let lay = account_layout(size, &theme, form);
    ctx.scene
        .fill_rect(Rect::from_size(size), Color::rgba(0.0, 0.0, 0.0, 0.45));
    ctx.scene
        .fill_rounded_rect(lay.card, theme.radius.large, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(lay.card, theme.radius.large, 1.0, theme.colors.border);
    ctx.label_left(
        lay.title.x(),
        lay.title.center().y,
        &tr(form.mode.title()),
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );
    let subtitle = match &form.mode {
        crate::app::AccountMode::AddUser => tr("Creates a home folder and the bash shell"),
        crate::app::AccountMode::ChangePassword { login } => {
            format!("{}: {login}", tr("New password for account"))
        }
    };
    ctx.label_left(
        lay.subtitle.x(),
        lay.subtitle.center().y,
        &subtitle,
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
    for (field, rect) in &lay.fields {
        let mut entry = form.entry(*field).clone();
        // Kursor rysuje tylko aktywne pole — reszta wygląda jak zwykłe pola.
        entry.set_focused(*field == form.focus);
        entry.paint(ctx, *rect);
    }
    if let Some(error) = &form.error {
        ctx.label_left(
            lay.error.x(),
            lay.error.center().y,
            error,
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.warning,
        );
    } else if form.is_pending() {
        ctx.label_left(
            lay.error.x(),
            lay.error.center().y,
            &tr("Applying changes…"),
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
    }
    let submit_label = match form.mode {
        crate::app::AccountMode::AddUser => tr("Create account"),
        crate::app::AccountMode::ChangePassword { .. } => tr("Change password"),
    };
    Button::new(submit_label)
        .variant(ButtonVariant::Suggested)
        .paint(ctx, lay.submit);
    Button::new(tr("Cancel")).paint(ctx, lay.cancel);
}

fn paint_editor(app: &SettingsApp, ctx: &mut PaintCtx, size: Size) {
    let Some(editor) = &app.editor else {
        return;
    };
    let theme = *ctx.theme;
    let lay = editor_layout(size, &theme);
    ctx.scene
        .fill_rect(Rect::from_size(size), Color::rgba(0.0, 0.0, 0.0, 0.45));
    ctx.scene
        .fill_rounded_rect(lay.card, theme.radius.large, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(lay.card, theme.radius.large, 1.0, theme.colors.border);
    ctx.label_left(
        lay.title.x(),
        lay.title.center().y,
        &editor.title,
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );
    let mut entry = editor.entry.clone();
    entry.set_focused(true);
    entry.paint(ctx, lay.entry);
    Button::new(tr("Save"))
        .variant(ButtonVariant::Suggested)
        .paint(ctx, lay.save);
    Button::new(tr("Cancel")).paint(ctx, lay.cancel);
}

// ── Nagrywanie skrótu klawiszowego ──

/// Przechwytuje pierwszą kombinację z Super/Ctrl/Alt (Esc anuluje).
fn handle_shortcut_record(app: &mut SettingsApp, events: &[Event]) {
    for event in events {
        if let Event::Key(key) = event {
            if key.state != ButtonState::Pressed {
                continue;
            }
            if key.keysym == 0xff1b {
                app.shortcut_record = None;
                return;
            }
            // Same modyfikatory nie tworzą kombinacji.
            if (0xffe1..=0xffee).contains(&key.keysym) {
                continue;
            }
            let modifiers = key.modifiers;
            if modifiers.logo || modifiers.ctrl || modifiers.alt {
                let binding = libluna_core::shortcuts::Binding::from_key(
                    modifiers.logo,
                    modifiers.ctrl,
                    modifiers.alt,
                    modifiers.shift,
                    key.keysym,
                );
                // Alt+Tab i Alt+F4 obsługuje kompozytor niezależnie od konfiguracji.
                let reserved = !binding.logo
                    && !binding.ctrl
                    && binding.alt
                    && !binding.shift
                    && matches!(binding.keysym, 0xff09 | 0xffbe);
                if reserved {
                    app.show_status(tr("This combination is reserved"));
                    continue;
                }
                app.assign_shortcut(binding);
                app.shortcut_record = None;
                return;
            }
        }
    }
}

fn paint_shortcut_record(app: &SettingsApp, ctx: &mut PaintCtx, size: Size) {
    let Some(action) = app.shortcut_record else {
        return;
    };
    let theme = *ctx.theme;
    let w = (size.width - 120.0).clamp(360.0, 520.0);
    let h = 184.0;
    let card = Rect::from_xywh((size.width - w) / 2.0, (size.height - h) / 2.0, w, h);
    ctx.scene
        .fill_rect(Rect::from_size(size), Color::rgba(0.0, 0.0, 0.0, 0.45));
    ctx.scene
        .fill_rounded_rect(card, theme.radius.large, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(card, theme.radius.large, 1.0, theme.colors.border);
    ctx.label_left(
        card.x() + 20.0,
        card.y() + 30.0,
        &tr("Press a key combination"),
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );
    ctx.label_left(
        card.x() + 20.0,
        card.y() + 56.0,
        &crate::pages::shortcut_action_label(action),
        theme.typography.body.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
    let slot = Rect::from_xywh(card.x() + 20.0, card.y() + 74.0, w - 40.0, 48.0);
    ctx.scene.fill_rounded_rect(
        slot,
        theme.radius.small,
        theme.colors.shade.scale_alpha(1.6),
    );
    ctx.scene
        .stroke_rounded_rect(slot, theme.radius.small, 1.5, theme.colors.accent);
    ctx.label_center(
        slot.center().x,
        slot.center().y,
        &tr("Super, Ctrl or Alt + key"),
        theme.typography.body.size,
        weight::NORMAL,
        theme.colors.text,
    );
    ctx.label_left(
        card.x() + 20.0,
        card.bottom() - 22.0,
        &tr("Press Esc to cancel"),
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
}

// ── Panel Info (BottomSheet) ──

/// Wewnętrzny obszar treści panelu Info.
fn info_content_rect(app: &SettingsApp, size: Size, theme: &Theme) -> Rect {
    let content = app.info_sheet.content_rect(size, theme);
    Rect::from_xywh(
        content.x() + 20.0,
        content.y() + 8.0,
        (content.width() - 40.0).max(0.0),
        (content.height() - 16.0).max(0.0),
    )
}

fn paint_info_sheet(app: &SettingsApp, ctx: &mut PaintCtx, size: Size) {
    let theme = *ctx.theme;
    let content = info_content_rect(app, size, &theme);
    let groups = pages::info_groups(app);
    if groups.is_empty() {
        ctx.label_center(
            content.center().x,
            content.center().y,
            &tr("No additional information for this page"),
            theme.typography.body.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
        return;
    }

    let page = preferences_page(&groups);
    let mut scroll = app.info_scroll;
    scroll.set_content_height(page.content_height(row_height(&theme), &theme));
    let offset = scroll.offset().min(scroll.max_offset(content));
    let area = Rect::from_xywh(
        content.x(),
        content.y() - offset,
        content.width(),
        scroll.content_height().max(content.height()),
    );

    ctx.scene.push_clip(content);
    let contents = page.paint(ctx, area, row_height(&theme));
    for (group, group_rect) in groups.iter().zip(contents.iter()) {
        for (index, row) in group.rows.iter().enumerate() {
            let rect = PreferencesGroup::row_rect(*group_rect, index, row_height(&theme));
            paint_row(
                ctx,
                row,
                rect,
                index + 1 == group.rows.len(),
                app.icons.theme,
            );
        }
    }
    ctx.scene.pop_clip();
    scroll.paint_scrollbar(ctx, content);
}

fn handle_info_events(
    app: &mut SettingsApp,
    events: &[Event],
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    let groups = pages::info_groups(app);
    let page = preferences_page(&groups);
    let content = info_content_rect(app, size, theme);
    app.info_scroll
        .set_content_height(page.content_height(row_height(theme), theme));
    app.info_scroll
        .set_offset(app.info_scroll.offset(), content);

    for event in events {
        match event {
            Event::PointerMotion { position, .. } => {
                app.info_sheet.update_drag(position.y);
            }
            Event::PointerButton(button)
                if button.state == ButtonState::Pressed && button.button == PointerButton::Left =>
            {
                let p = button.position;
                if app.info_sheet.close_at(size, theme, p)
                    || app.info_sheet.scrim_at(size, theme, p)
                {
                    app.close_info_sheet();
                } else if app.info_sheet.drag_zone_at(size, theme, p) {
                    app.info_sheet.begin_drag(p.y);
                } else if content.contains(p) {
                    let hits = row_hits(&groups, content, theme, text, app.info_scroll.offset());
                    for hit in hits {
                        handle_click(app, hit, p, theme, text);
                    }
                }
            }
            Event::PointerButton(button)
                if button.state == ButtonState::Released
                    && button.button == PointerButton::Left =>
            {
                if app.info_sheet.end_drag(theme) {
                    app.close_info_sheet();
                }
            }
            Event::Scroll(scroll) if content.contains(scroll.position) => {
                app.info_scroll
                    .on_scroll(scroll.dy / SCROLL_UNIT * SCROLL_STEP, content);
            }
            Event::Key(key) if key.state == ButtonState::Pressed && key.keysym == 0xff1b => {
                app.close_info_sheet();
            }
            _ => {}
        }
    }
}

// ── Wybór tapety z dysku ──

const WALLPAPER_COLS: usize = 3;
const WALLPAPER_GAP: f64 = 14.0;
const WALLPAPER_ROW_H: f64 = 34.0;
const WALLPAPER_SHEET_ROW_H: f64 = 44.0;
const WALLPAPER_SHEET_W: f64 = 430.0;

struct WallpaperLayout {
    card: Rect,
    title: Rect,
    path: Rect,
    grid: Rect,
    hint: Rect,
    choose: Rect,
    cancel: Rect,
    toggle: Rect,
    chip: Rect,
    /// Arkusz „Więcej tapet” (liczony zawsze, rysowany gdy otwarty).
    sheet: Rect,
    sheet_title: Rect,
    sheet_rows: Vec<Rect>,
}

fn wallpaper_layout(app: &SettingsApp, size: Size, theme: &Theme) -> WallpaperLayout {
    let w = (size.width - 120.0).clamp(420.0, 900.0);
    let h = (size.height - 100.0).clamp(300.0, 660.0);
    let card = Rect::from_xywh((size.width - w) / 2.0, (size.height - h) / 2.0, w, h);
    let title = Rect::from_xywh(card.x() + 20.0, card.y() + 14.0, card.width() - 88.0, 28.0);
    let toggle = Rect::from_xywh(
        card.right() - 20.0 - 34.0,
        title.center().y - 17.0,
        34.0,
        34.0,
    );
    let path = Rect::from_xywh(
        card.x() + 20.0,
        title.bottom() + 2.0,
        card.width() - 40.0,
        20.0,
    );
    let chip = Rect::from_xywh(path.right() - 132.0, path.y() - 3.0, 132.0, 26.0);
    let button_h = theme.metrics.button.height;
    let choose = Rect::from_xywh(
        card.right() - 20.0 - 120.0,
        card.bottom() - 16.0 - button_h,
        120.0,
        button_h,
    );
    let cancel = Rect::from_xywh(choose.x() - 8.0 - 96.0, choose.y(), 96.0, button_h);
    let hint = Rect::from_xywh(
        card.x() + 20.0,
        choose.y() - 26.0,
        card.width() - 40.0,
        18.0,
    );
    let grid = Rect::from_xywh(
        card.x() + 16.0,
        path.bottom() + 10.0,
        card.width() - 32.0,
        (hint.y() - path.bottom() - 18.0).max(0.0),
    );

    // Arkusz „Więcej tapet” nad kartą.
    let rows = app
        .wallpaper_picker
        .as_ref()
        .map(|picker| picker.sources.len() + 2)
        .unwrap_or(0);
    let sheet_h = 60.0 + rows as f64 * WALLPAPER_SHEET_ROW_H + 14.0;
    let sheet_w = WALLPAPER_SHEET_W.min((size.width - 80.0).max(240.0));
    let sheet = Rect::from_xywh(
        (size.width - sheet_w) / 2.0,
        (size.height - sheet_h) / 2.0,
        sheet_w,
        sheet_h,
    );
    let sheet_title = Rect::from_xywh(
        sheet.x() + 18.0,
        sheet.y() + 14.0,
        sheet.width() - 36.0,
        24.0,
    );
    let sheet_rows = (0..rows)
        .map(|index| {
            Rect::from_xywh(
                sheet.x() + 8.0,
                sheet_title.bottom() + 8.0 + index as f64 * WALLPAPER_SHEET_ROW_H,
                sheet.width() - 16.0,
                WALLPAPER_SHEET_ROW_H,
            )
        })
        .collect();

    WallpaperLayout {
        card,
        title,
        path,
        grid,
        hint,
        choose,
        cancel,
        toggle,
        chip,
        sheet,
        sheet_title,
        sheet_rows,
    }
}

/// Rozmiar kafla: podgląd 16:9 + pasek z nazwą pod nim.
fn wallpaper_tile_size(grid: Rect) -> (f64, f64) {
    let width = ((grid.width() - WALLPAPER_GAP * (WALLPAPER_COLS as f64 - 1.0))
        / WALLPAPER_COLS as f64)
        .max(40.0);
    let inner = (width - 12.0).max(10.0);
    (width, inner * 9.0 / 16.0 + 32.0)
}

fn wallpaper_row_count(app: &SettingsApp) -> usize {
    app.wallpaper_picker
        .as_ref()
        .map(|picker| (picker.entries.len() + 1).div_ceil(WALLPAPER_COLS))
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WallpaperHit {
    Entry(usize),
    More,
}

fn wallpaper_hit(app: &SettingsApp, lay: &WallpaperLayout, p: Point) -> Option<WallpaperHit> {
    let picker = app.wallpaper_picker.as_ref()?;
    if !lay.grid.contains(p) {
        return None;
    }
    if picker.browse {
        let index = ((p.y - lay.grid.y() + picker.scroll) / WALLPAPER_ROW_H).floor() as usize;
        return (index < picker.entries.len()).then_some(WallpaperHit::Entry(index));
    }
    let (tile_w, tile_h) = wallpaper_tile_size(lay.grid);
    let column = ((p.x - lay.grid.x()) / (tile_w + WALLPAPER_GAP)).floor();
    let row = ((p.y - lay.grid.y() + picker.scroll) / (tile_h + WALLPAPER_GAP)).floor();
    if column < 0.0 || column >= WALLPAPER_COLS as f64 || row < 0.0 {
        return None;
    }
    let x0 = lay.grid.x() + column * (tile_w + WALLPAPER_GAP);
    let y0 = lay.grid.y() + row * (tile_h + WALLPAPER_GAP) - picker.scroll;
    if p.x > x0 + tile_w || p.y > y0 + tile_h {
        return None;
    }
    let index = row as usize * WALLPAPER_COLS + column as usize;
    if index == 0 {
        Some(WallpaperHit::More)
    } else if index <= picker.entries.len() {
        Some(WallpaperHit::Entry(index - 1))
    } else {
        None
    }
}

fn wallpaper_max_scroll(app: &SettingsApp, lay: &WallpaperLayout) -> f64 {
    let Some(picker) = &app.wallpaper_picker else {
        return 0.0;
    };
    if picker.browse {
        let content = picker.entries.len() as f64 * WALLPAPER_ROW_H;
        return (content - lay.grid.height()).max(0.0);
    }
    let (_, tile_h) = wallpaper_tile_size(lay.grid);
    let rows = wallpaper_row_count(app) as f64;
    let content = rows * tile_h + (rows - 1.0).max(0.0) * WALLPAPER_GAP;
    (content - lay.grid.height()).max(0.0)
}

/// Przewija siatkę tak, żeby wybrany kafel był widoczny.
fn wallpaper_reveal(app: &mut SettingsApp, lay: &WallpaperLayout) {
    let max = wallpaper_max_scroll(app, lay);
    let Some(picker) = &mut app.wallpaper_picker else {
        return;
    };
    if picker.browse {
        let top = picker.selected as f64 * WALLPAPER_ROW_H;
        if top < picker.scroll {
            picker.scroll = top;
        } else if top + WALLPAPER_ROW_H > picker.scroll + lay.grid.height() {
            picker.scroll = top + WALLPAPER_ROW_H - lay.grid.height();
        }
    } else {
        let (_, tile_h) = wallpaper_tile_size(lay.grid);
        let row = (picker.selected / WALLPAPER_COLS) as f64;
        let top = row * (tile_h + WALLPAPER_GAP);
        if top < picker.scroll {
            picker.scroll = top;
        } else if top + tile_h > picker.scroll + lay.grid.height() {
            picker.scroll = top + tile_h - lay.grid.height();
        }
    }
    picker.scroll = picker.scroll.clamp(0.0, max);
}

fn handle_wallpaper_events(app: &mut SettingsApp, events: &[Event], size: Size, theme: &Theme) {
    let lay = wallpaper_layout(app, size, theme);
    for event in events {
        match event {
            Event::PointerMotion { position, .. } => {
                let more = app
                    .wallpaper_picker
                    .as_ref()
                    .is_some_and(|picker| picker.more);
                let hover = if more {
                    None
                } else {
                    match wallpaper_hit(app, &lay, *position) {
                        Some(WallpaperHit::Entry(index)) => Some(index + 1),
                        Some(WallpaperHit::More) => Some(0),
                        None => None,
                    }
                };
                let more_hover = if more {
                    lay.sheet_rows
                        .iter()
                        .position(|row| row.contains(*position))
                } else {
                    None
                };
                if let Some(picker) = &mut app.wallpaper_picker {
                    picker.hover = hover;
                    picker.more_hover = more_hover;
                }
            }
            Event::PointerLeave => {
                if let Some(picker) = &mut app.wallpaper_picker {
                    picker.hover = None;
                    picker.more_hover = None;
                }
            }
            Event::PointerButton(button)
                if button.state == ButtonState::Pressed && button.button == PointerButton::Left =>
            {
                let p = button.position;
                let more = app
                    .wallpaper_picker
                    .as_ref()
                    .is_some_and(|picker| picker.more);
                if more {
                    if let Some(index) = lay.sheet_rows.iter().position(|row| row.contains(p)) {
                        wallpaper_sheet_action(app, index);
                    } else if !lay.sheet.contains(p) {
                        if let Some(picker) = &mut app.wallpaper_picker {
                            picker.more = false;
                        }
                    }
                    continue;
                }
                if lay.cancel.contains(p) || !lay.card.contains(p) {
                    app.cancel_wallpaper_picker();
                } else if lay.toggle.contains(p) {
                    if let Some(picker) = &mut app.wallpaper_picker {
                        if picker.browse {
                            picker.show_grid();
                        } else {
                            picker.browse_disk();
                        }
                    }
                } else if lay.choose.contains(p) {
                    app.choose_wallpaper();
                } else if let Some(hit) = wallpaper_hit(app, &lay, p) {
                    match hit {
                        WallpaperHit::More => {
                            if let Some(picker) = &mut app.wallpaper_picker {
                                picker.more = true;
                                picker.more_hover = None;
                            }
                        }
                        WallpaperHit::Entry(index) => app.wallpaper_pick(index),
                    }
                }
            }
            Event::Scroll(scroll) if lay.grid.contains(scroll.position) => {
                let max = wallpaper_max_scroll(app, &lay);
                let step = if app
                    .wallpaper_picker
                    .as_ref()
                    .is_some_and(|picker| picker.browse)
                {
                    WALLPAPER_ROW_H * 2.0
                } else {
                    wallpaper_tile_size(lay.grid).1 + WALLPAPER_GAP
                };
                let step = scroll.dy / SCROLL_UNIT * step;
                if let Some(picker) = &mut app.wallpaper_picker {
                    picker.scroll = (picker.scroll + step).clamp(0.0, max);
                }
            }
            Event::Key(key) if key.state == ButtonState::Pressed => {
                if key.keysym == 0xff1b {
                    if app
                        .wallpaper_picker
                        .as_ref()
                        .is_some_and(|picker| picker.more)
                    {
                        if let Some(picker) = &mut app.wallpaper_picker {
                            picker.more = false;
                        }
                    } else {
                        app.cancel_wallpaper_picker();
                    }
                    continue;
                }
                match key.keysym {
                    0xff0d | 0xff8d => app.choose_wallpaper(),
                    0xff51 => wallpaper_move(app, -1, 0),
                    0xff53 => wallpaper_move(app, 1, 0),
                    0xff52 => wallpaper_move(app, 0, -1),
                    0xff54 => wallpaper_move(app, 0, 1),
                    0xff50 => wallpaper_move(app, 0, -3),
                    0xff57 => wallpaper_move(app, 0, 3),
                    _ => continue,
                }
                wallpaper_reveal(app, &lay);
            }
            _ => {}
        }
    }
}

/// Działanie wiersza arkusza „Więcej tapet”.
fn wallpaper_sheet_action(app: &mut SettingsApp, index: usize) {
    let sources_len = app
        .wallpaper_picker
        .as_ref()
        .map(|picker| picker.sources.len())
        .unwrap_or(0);
    if index < sources_len {
        let path = app
            .wallpaper_picker
            .as_ref()
            .map(|picker| picker.sources[index].path.clone());
        if let (Some(path), Some(picker)) = (path, &mut app.wallpaper_picker) {
            picker.navigate(path);
            picker.more = false;
        }
    } else if index == sources_len {
        app.surprise_wallpaper();
    } else if let Some(picker) = &mut app.wallpaper_picker {
        picker.browse_disk();
    }
}

fn wallpaper_move(app: &mut SettingsApp, dx: i64, dy: i64) {
    let Some(picker) = &mut app.wallpaper_picker else {
        return;
    };
    if picker.entries.is_empty() {
        return;
    }
    let cols = if picker.browse {
        1
    } else {
        WALLPAPER_COLS as i64
    };
    let next = picker.selected as i64 + dx + dy * cols;
    picker.selected = next.clamp(0, picker.entries.len() as i64 - 1) as usize;
}

fn paint_wallpaper_picker(app: &SettingsApp, ctx: &mut PaintCtx, size: Size) {
    let Some(picker) = &app.wallpaper_picker else {
        return;
    };
    let theme = *ctx.theme;
    let lay = wallpaper_layout(app, size, &theme);
    ctx.scene
        .fill_rect(Rect::from_size(size), Color::rgba(0.0, 0.0, 0.0, 0.45));
    ctx.scene
        .fill_rounded_rect(lay.card, theme.radius.large, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(lay.card, theme.radius.large, 1.0, theme.colors.border);

    ctx.label_left(
        lay.title.x(),
        lay.title.center().y,
        &tr("Choose a wallpaper"),
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );

    // Przełącznik: siatka miniatur ↔ lista plików.
    ctx.scene
        .fill_rounded_rect(lay.toggle, theme.radius.small, theme.colors.shade);
    let toggle_icon = if picker.browse {
        IconName::Grid
    } else {
        IconName::List
    };
    paint_icon(
        ctx,
        toggle_icon,
        Rect::from_xywh(
            lay.toggle.center().x - 9.0,
            lay.toggle.center().y - 9.0,
            18.0,
            18.0,
        ),
        theme.colors.dim_text,
    );

    // Ścieżka + plakietka miejsca docelowego.
    let path_width = (lay.path.width() - lay.chip.width() - 12.0).max(0.0);
    let path = fit_text(
        &picker.dir.display().to_string(),
        path_width,
        theme.typography.caption.size,
        weight::NORMAL,
        ctx.text,
    );
    ctx.label_left(
        lay.path.x(),
        lay.path.center().y,
        &path,
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
    ctx.scene
        .fill_rounded_rect(lay.chip, lay.chip.height() / 2.0, theme.colors.shade);
    let target = match picker.target {
        WallpaperTarget::Greeter => tr("Login Screen"),
        WallpaperTarget::Desktop => tr("Desktop"),
    };
    ctx.label_center(
        lay.chip.center().x,
        lay.chip.center().y,
        &target,
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );

    if picker.browse {
        if picker.entries.is_empty() {
            ctx.label_center(
                lay.grid.center().x,
                lay.grid.center().y,
                &tr("No PNG images in this folder"),
                theme.typography.body.size,
                weight::NORMAL,
                theme.colors.dim_text,
            );
        } else {
            paint_wallpaper_list(picker, ctx, &lay, &theme);
        }
    } else {
        // Siatka zawsze rysuje kafel „Więcej tapet” — także w pustym katalogu.
        paint_wallpaper_grid(app, picker, ctx, &lay, &theme);
        if picker.entries.is_empty() {
            let (_, tile_h) = wallpaper_tile_size(lay.grid);
            ctx.label_center(
                lay.grid.center().x,
                lay.grid.y() + tile_h + 26.0,
                &tr("No PNG images in this folder"),
                theme.typography.body.size,
                weight::NORMAL,
                theme.colors.dim_text,
            );
        }
    }

    let hint = if picker.target == WallpaperTarget::Desktop && picker.previewed {
        tr("Live preview — Cancel restores the previous wallpaper")
    } else {
        match picker.target {
            WallpaperTarget::Greeter => tr("Only PNG images are supported by the login screen"),
            WallpaperTarget::Desktop => tr("Pick any PNG image from your computer"),
        }
    };
    ctx.label_left(
        lay.hint.x(),
        lay.hint.center().y,
        &fit_text(
            &hint,
            lay.hint.width(),
            theme.typography.caption.size,
            weight::NORMAL,
            ctx.text,
        ),
        theme.typography.caption.size,
        weight::NORMAL,
        if picker.previewed && picker.target == WallpaperTarget::Desktop {
            theme.colors.accent
        } else {
            theme.colors.dim_text
        },
    );
    let selected_is_dir = picker
        .selected_path()
        .map(|path| path.is_dir())
        .unwrap_or(false);
    Button::new(if selected_is_dir {
        tr("Open")
    } else {
        tr("Choose")
    })
    .variant(ButtonVariant::Suggested)
    .paint(ctx, lay.choose);
    Button::new(tr("Cancel")).paint(ctx, lay.cancel);

    if picker.more {
        paint_wallpaper_sheet(app, ctx, size, &lay, &theme);
    }
}

fn paint_wallpaper_grid(
    app: &SettingsApp,
    picker: &WallpaperPicker,
    ctx: &mut PaintCtx,
    lay: &WallpaperLayout,
    theme: &Theme,
) {
    let (tile_w, tile_h) = wallpaper_tile_size(lay.grid);
    let step = tile_h + WALLPAPER_GAP;
    let rows = wallpaper_row_count(app);
    let content = rows as f64 * tile_h + rows.saturating_sub(1) as f64 * WALLPAPER_GAP;
    let max_scroll = (content - lay.grid.height()).max(0.0);
    let scroll = picker.scroll.clamp(0.0, max_scroll);

    // Kafel „Więcej tapet” jest pierwszy — zawsze widoczny, także w pustym katalogu.
    ctx.scene.push_clip(lay.grid);
    for index in 0..=picker.entries.len() {
        let row = index / WALLPAPER_COLS;
        let column = index % WALLPAPER_COLS;
        let tile = Rect::from_xywh(
            lay.grid.x() + column as f64 * (tile_w + WALLPAPER_GAP),
            lay.grid.y() + row as f64 * step - scroll,
            tile_w,
            tile_h,
        );
        if tile.bottom() < lay.grid.y() || tile.y() > lay.grid.bottom() {
            continue;
        }
        let hovered = picker.hover == Some(index);
        if index == 0 {
            paint_wallpaper_more_tile(ctx, theme, tile, hovered);
            continue;
        }
        let entry = index - 1;
        let path = &picker.entries[entry];
        let selected = entry == picker.selected;
        if path.is_dir() {
            let is_parent = entry == 0 && picker.dir.parent().is_some();
            paint_wallpaper_folder_tile(ctx, theme, tile, path, is_parent, hovered, selected);
        } else {
            paint_wallpaper_image_tile(ctx, theme, picker, tile, path, hovered, selected);
        }
    }
    ctx.scene.pop_clip();

    if max_scroll > 0.5 {
        let track = Rect::from_xywh(lay.grid.right() - 3.0, lay.grid.y(), 3.0, lay.grid.height());
        let ratio = lay.grid.height() / content;
        let knob_h = (lay.grid.height() * ratio).max(24.0);
        let y = lay.grid.y() + (lay.grid.height() - knob_h) * (scroll / max_scroll);
        ctx.scene.fill_rounded_rect(track, 1.5, theme.colors.shade);
        ctx.scene.fill_rounded_rect(
            Rect::from_xywh(track.x(), y, 3.0, knob_h),
            1.5,
            theme.colors.dim_text.scale_alpha(0.55),
        );
    }
}

fn paint_wallpaper_list(
    picker: &WallpaperPicker,
    ctx: &mut PaintCtx,
    lay: &WallpaperLayout,
    theme: &Theme,
) {
    let max_scroll = (picker.entries.len() as f64 * WALLPAPER_ROW_H - lay.grid.height()).max(0.0);
    let scroll = picker.scroll.clamp(0.0, max_scroll);
    ctx.scene.push_clip(lay.grid);
    for (index, entry) in picker.entries.iter().enumerate() {
        let row = Rect::from_xywh(
            lay.grid.x() + 4.0,
            lay.grid.y() + index as f64 * WALLPAPER_ROW_H - scroll,
            lay.grid.width() - 8.0,
            WALLPAPER_ROW_H,
        );
        if row.bottom() < lay.grid.y() || row.y() > lay.grid.bottom() {
            continue;
        }
        if index == picker.selected {
            ctx.scene.fill_rounded_rect(
                row,
                theme.radius.small,
                theme.colors.accent.scale_alpha(0.18),
            );
            ctx.scene
                .stroke_rounded_rect(row, theme.radius.small, 1.5, theme.colors.accent);
        } else if picker.hover == Some(index) {
            ctx.scene.fill_rounded_rect(
                row,
                theme.radius.small,
                theme.colors.shade.scale_alpha(1.8),
            );
        }

        let is_parent = index == 0 && picker.dir.parent().is_some();
        let is_dir = entry.is_dir();
        let label = if is_parent {
            "..".to_owned()
        } else {
            let name = entry
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("?");
            if is_dir {
                format!("{name}/")
            } else {
                name.to_owned()
            }
        };
        let icon = if is_dir || is_parent {
            IconName::Files
        } else {
            IconName::Image
        };
        let icon_rect = Rect::from_xywh(row.x() + 8.0, row.center().y - 8.0, 16.0, 16.0);
        paint_icon(ctx, icon, icon_rect, theme.colors.dim_text);
        ctx.label_left(
            row.x() + 32.0,
            row.center().y,
            &fit_text(
                &label,
                (row.width() - 40.0).max(0.0),
                theme.typography.body.size,
                weight::NORMAL,
                ctx.text,
            ),
            theme.typography.body.size,
            weight::NORMAL,
            theme.colors.text,
        );
    }
    ctx.scene.pop_clip();
}

/// Podgląd kafla (16:9) z paskiem nazwy pod nim.
fn wallpaper_preview_rect(tile: Rect) -> Rect {
    Rect::from_xywh(
        tile.x() + 6.0,
        tile.y() + 6.0,
        tile.width() - 12.0,
        tile.height() - 32.0,
    )
}

fn paint_wallpaper_image_tile(
    ctx: &mut PaintCtx,
    theme: &Theme,
    picker: &WallpaperPicker,
    tile: Rect,
    path: &Path,
    hovered: bool,
    selected: bool,
) {
    let radius = theme.radius.medium;
    if selected {
        ctx.scene
            .fill_rounded_rect(tile, radius, theme.colors.accent.scale_alpha(0.16));
    } else if hovered {
        ctx.scene
            .fill_rounded_rect(tile, radius, theme.colors.shade.scale_alpha(1.6));
    }
    let preview = wallpaper_preview_rect(tile);
    ctx.scene.fill_rounded_rect(
        preview,
        theme.radius.small,
        Color::rgba(0.0, 0.0, 0.0, 0.35),
    );
    match wallpaper_thumbnail(path) {
        Some(image) => {
            ctx.scene.push_clip_rounded(preview, theme.radius.small);
            ctx.scene.image(preview, image);
            ctx.scene.pop_clip();
        }
        None => paint_wallpaper_placeholder(ctx, theme, preview, path, picker.target),
    }
    ctx.scene
        .stroke_rounded_rect(preview, theme.radius.small, 1.0, theme.colors.border);

    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("wallpaper.png");
    let label = fit_text(
        name,
        (tile.width() - 12.0).max(0.0),
        theme.typography.caption.size,
        weight::NORMAL,
        ctx.text,
    );
    ctx.label_center(
        tile.center().x,
        preview.bottom() + 13.0,
        &label,
        theme.typography.caption.size,
        weight::NORMAL,
        if selected {
            theme.colors.text
        } else {
            theme.colors.dim_text
        },
    );

    if selected {
        ctx.scene
            .stroke_rounded_rect(tile, radius, 2.0, theme.colors.accent);
        let center = Point::new(preview.right() - 14.0, preview.y() + 14.0);
        ctx.scene.fill_circle(center, 10.0, theme.colors.accent);
        paint_icon(
            ctx,
            IconName::Check,
            Rect::from_xywh(center.x - 6.0, center.y - 6.0, 12.0, 12.0),
            theme.colors.accent_fg,
        );
    }
}

fn paint_wallpaper_folder_tile(
    ctx: &mut PaintCtx,
    theme: &Theme,
    tile: Rect,
    path: &Path,
    is_parent: bool,
    hovered: bool,
    selected: bool,
) {
    let radius = theme.radius.medium;
    if selected {
        ctx.scene
            .fill_rounded_rect(tile, radius, theme.colors.accent.scale_alpha(0.16));
    } else if hovered {
        ctx.scene
            .fill_rounded_rect(tile, radius, theme.colors.shade.scale_alpha(1.6));
    }
    let preview = wallpaper_preview_rect(tile);
    ctx.scene
        .fill_rounded_rect(preview, theme.radius.small, theme.colors.shade);
    ctx.scene
        .stroke_rounded_rect(preview, theme.radius.small, 1.0, theme.colors.border);
    paint_icon(
        ctx,
        IconName::Files,
        Rect::from_xywh(
            preview.center().x - 22.0,
            preview.center().y - 22.0,
            44.0,
            44.0,
        ),
        theme.colors.dim_text,
    );
    let name = if is_parent {
        tr("Up").to_owned()
    } else {
        format!(
            "{}/",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("?")
        )
    };
    let label = fit_text(
        &name,
        (tile.width() - 12.0).max(0.0),
        theme.typography.caption.size,
        weight::NORMAL,
        ctx.text,
    );
    ctx.label_center(
        tile.center().x,
        preview.bottom() + 13.0,
        &label,
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
    if selected {
        ctx.scene
            .stroke_rounded_rect(tile, radius, 2.0, theme.colors.accent);
    }
}

fn paint_wallpaper_more_tile(ctx: &mut PaintCtx, theme: &Theme, tile: Rect, hovered: bool) {
    let radius = theme.radius.medium;
    if hovered {
        ctx.scene
            .fill_rounded_rect(tile, radius, theme.colors.shade.scale_alpha(1.6));
    }
    let preview = wallpaper_preview_rect(tile);
    ctx.scene
        .stroke_rounded_rect(preview, theme.radius.small, 1.0, theme.colors.border);
    let center = preview.center();
    ctx.scene
        .fill_circle(center, 22.0, theme.colors.accent.scale_alpha(0.18));
    paint_icon(
        ctx,
        IconName::Plus,
        Rect::from_xywh(center.x - 12.0, center.y - 12.0, 24.0, 24.0),
        theme.colors.accent,
    );
    ctx.label_center(
        tile.center().x,
        preview.bottom() + 13.0,
        &tr("More wallpapers"),
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
}

/// Podgląd zastępczy, gdy PNG nie da się zdekodować — dwukolorowa kompozycja
/// wyliczona z nazwy pliku (każda tapeta wygląda inaczej, bez losowości w klatkach).
fn paint_wallpaper_placeholder(
    ctx: &mut PaintCtx,
    theme: &Theme,
    rect: Rect,
    path: &Path,
    target: WallpaperTarget,
) {
    let seed = path
        .to_string_lossy()
        .bytes()
        .fold(2166136261u32, |acc, byte| {
            (acc ^ byte as u32).wrapping_mul(16777619)
        });
    let hue = (seed % 360) as f64;
    let accent = match target {
        WallpaperTarget::Greeter => theme.colors.accent,
        WallpaperTarget::Desktop => theme.colors.success,
    };
    let base = accent.scale_alpha(0.35);
    let glow = accent.scale_alpha(0.55);
    ctx.scene.push_clip_rounded(rect, theme.radius.small);
    ctx.scene.fill_rect(rect, base);
    let size = rect.height() * 0.9;
    let dx = (hue / 360.0) as f64 * (rect.width() - size).max(0.0);
    ctx.scene.fill_circle(
        Point::new(rect.x() + dx + size / 2.0, rect.bottom() - size * 0.15),
        size / 2.0,
        glow,
    );
    ctx.scene.pop_clip();
}

fn paint_wallpaper_sheet(
    app: &SettingsApp,
    ctx: &mut PaintCtx,
    size: Size,
    lay: &WallpaperLayout,
    theme: &Theme,
) {
    let Some(picker) = &app.wallpaper_picker else {
        return;
    };
    ctx.scene
        .fill_rect(Rect::from_size(size), Color::rgba(0.0, 0.0, 0.0, 0.35));
    ctx.scene
        .fill_rounded_rect(lay.sheet, theme.radius.large, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(lay.sheet, theme.radius.large, 1.0, theme.colors.border);
    ctx.label_left(
        lay.sheet_title.x(),
        lay.sheet_title.center().y,
        &tr("More wallpapers"),
        theme.typography.body.size,
        weight::BOLD,
        theme.colors.text,
    );

    for (index, row) in lay.sheet_rows.iter().enumerate() {
        let hovered = picker.more_hover == Some(index);
        if hovered {
            ctx.scene.fill_rounded_rect(
                *row,
                theme.radius.small,
                theme.colors.shade.scale_alpha(1.6),
            );
        }
        let (icon, label) = if index < picker.sources.len() {
            let source = &picker.sources[index];
            let icon = match source.key {
                "Home folder" => IconName::Home,
                "System wallpapers" => IconName::Image,
                _ => IconName::Files,
            };
            (icon, tr(source.key).to_owned())
        } else if index == picker.sources.len() {
            (IconName::Star, tr("Surprise me").to_owned())
        } else {
            (IconName::Files, tr("Browse the disk…").to_owned())
        };
        paint_icon(
            ctx,
            icon,
            Rect::from_xywh(row.x() + 12.0, row.center().y - 9.0, 18.0, 18.0),
            theme.colors.dim_text,
        );
        ctx.label_left(
            row.x() + 42.0,
            row.center().y,
            &fit_text(
                &label,
                (row.width() - 84.0).max(0.0),
                theme.typography.body.size,
                weight::NORMAL,
                ctx.text,
            ),
            theme.typography.body.size,
            weight::NORMAL,
            theme.colors.text,
        );
        paint_icon(
            ctx,
            IconName::ChevronRight,
            Rect::from_xywh(row.right() - 28.0, row.center().y - 8.0, 16.0, 16.0),
            theme.colors.dim_text.scale_alpha(0.8),
        );
    }
}

// ── Formularz hasła Wi-Fi ──

fn wifi_insert(app: &mut SettingsApp, text: &str) {
    let plain: String = text.chars().filter(|c| !c.is_control()).collect();
    if plain.is_empty() {
        return;
    }
    if let Some(prompt) = &mut app.wifi_prompt {
        if !prompt.is_pending() {
            prompt.password.insert_str(&plain);
        }
    }
}

fn handle_wifi_events(app: &mut SettingsApp, events: &[Event], size: Size, theme: &Theme) {
    let lay = admin_layout(size, theme);
    for event in events {
        match event {
            Event::PointerButton(button)
                if button.state == ButtonState::Pressed && button.button == PointerButton::Left =>
            {
                let p = button.position;
                if lay.save.contains(p) {
                    app.submit_wifi_password();
                } else if lay.cancel.contains(p) || !lay.card.contains(p) {
                    app.wifi_prompt = None;
                } else if lay.entry.contains(p) {
                    if let Some(prompt) = &mut app.wifi_prompt {
                        prompt.password.set_focused(true);
                    }
                }
            }
            Event::Key(key) if key.state == ButtonState::Pressed => {
                let clipboard = app
                    .wifi_prompt
                    .as_mut()
                    .filter(|prompt| !prompt.is_pending())
                    .and_then(|prompt| clipboard_command(&mut prompt.password, key));
                if let Some(cmd) = clipboard {
                    apply_text_clipboard(app, cmd, TextPasteTarget::WifiPassword);
                    continue;
                }
                match key.keysym {
                    0xff1b => app.wifi_prompt = None,
                    0xff0d | 0xff8d => app.submit_wifi_password(),
                    0xff08 => {
                        if let Some(prompt) = &mut app.wifi_prompt {
                            if !prompt.is_pending() {
                                prompt.password.backspace();
                            }
                        }
                    }
                    0x61 if key.modifiers.ctrl => {
                        if let Some(prompt) = &mut app.wifi_prompt {
                            if !prompt.is_pending() {
                                prompt.password.select_all();
                            }
                        }
                    }
                    _ => {
                        if !key.modifiers.ctrl {
                            if let Some(text) = &key.text {
                                wifi_insert(app, text);
                            }
                        }
                    }
                }
            }
            Event::Text(text) => wifi_insert(app, text),
            _ => {}
        }
    }
}

fn paint_wifi_prompt(app: &SettingsApp, ctx: &mut PaintCtx, size: Size) {
    let Some(prompt) = &app.wifi_prompt else {
        return;
    };
    let theme = *ctx.theme;
    let lay = admin_layout(size, &theme);
    ctx.scene
        .fill_rect(Rect::from_size(size), Color::rgba(0.0, 0.0, 0.0, 0.45));
    ctx.scene
        .fill_rounded_rect(lay.card, theme.radius.large, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(lay.card, theme.radius.large, 1.0, theme.colors.border);
    ctx.label_left(
        lay.title.x(),
        lay.title.center().y,
        &tr("Wi-Fi password"),
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );
    ctx.label_left(
        lay.subtitle.x(),
        lay.subtitle.center().y,
        &format!("{}: {}", tr("Network"), prompt.ssid),
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
    let mut password = prompt.password.clone();
    password.set_focused(true);
    password.paint(ctx, lay.entry);
    if let Some(error) = &prompt.error {
        ctx.label_left(
            lay.error.x(),
            lay.error.center().y,
            error,
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.warning,
        );
    } else if prompt.is_pending() {
        ctx.label_left(
            lay.error.x(),
            lay.error.center().y,
            &tr("Connecting…"),
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
    }
    Button::new(tr("Connect"))
        .variant(ButtonVariant::Suggested)
        .paint(ctx, lay.save);
    Button::new(tr("Cancel")).paint(ctx, lay.cancel);
}

// ── Formularz IPv4 połączenia przewodowego ──

struct WiredLayout {
    card: Rect,
    title: Rect,
    subtitle: Rect,
    address_caption: Rect,
    address: Rect,
    gateway_caption: Rect,
    gateway: Rect,
    dns_caption: Rect,
    dns: Rect,
    hint: Rect,
    save: Rect,
    cancel: Rect,
}

fn wired_layout(size: Size, theme: &Theme) -> WiredLayout {
    let w = (size.width - 120.0).clamp(360.0, 560.0);
    let h = 356.0_f64.min(size.height - 60.0);
    let card = Rect::from_xywh((size.width - w) / 2.0, (size.height - h) / 2.0, w, h);
    let x = card.x() + 20.0;
    let width = card.width() - 40.0;
    let title = Rect::from_xywh(x, card.y() + 14.0, width, 28.0);
    let subtitle = Rect::from_xywh(x, title.bottom() + 2.0, width, 18.0);
    let mut y = subtitle.bottom() + 10.0;
    let mut stack = |height: f64| {
        let rect = Rect::from_xywh(x, y, width, height);
        y += height + 6.0;
        rect
    };
    let address_caption = stack(16.0);
    let address = stack(36.0);
    let gateway_caption = stack(16.0);
    let gateway = stack(36.0);
    let dns_caption = stack(16.0);
    let dns = stack(36.0);
    let hint = Rect::from_xywh(x, y, width, 18.0);
    let button_h = theme.metrics.button.height;
    let save = Rect::from_xywh(
        card.right() - 20.0 - 96.0,
        card.bottom() - 16.0 - button_h,
        96.0,
        button_h,
    );
    let cancel = Rect::from_xywh(save.x() - 8.0 - 96.0, save.y(), 96.0, button_h);
    WiredLayout {
        card,
        title,
        subtitle,
        address_caption,
        address,
        gateway_caption,
        gateway,
        dns_caption,
        dns,
        hint,
        save,
        cancel,
    }
}

fn wired_field_mut(editor: &mut crate::app::WiredEditor, field: usize) -> &mut Entry {
    match field {
        1 => &mut editor.gateway,
        2 => &mut editor.dns,
        _ => &mut editor.address,
    }
}

fn wired_active_field(app: &SettingsApp) -> usize {
    let Some(editor) = &app.wired_editor else {
        return 0;
    };
    if editor.gateway.is_focused() {
        1
    } else if editor.dns.is_focused() {
        2
    } else {
        0
    }
}

fn wired_focus(app: &mut SettingsApp, field: usize) {
    if let Some(editor) = &mut app.wired_editor {
        editor.address.set_focused(field == 0);
        editor.gateway.set_focused(field == 1);
        editor.dns.set_focused(field == 2);
    }
}

fn wired_insert(app: &mut SettingsApp, field: usize, text: &str) {
    let plain: String = text.chars().filter(|ch| !ch.is_control()).collect();
    if plain.is_empty() {
        return;
    }
    if let Some(editor) = &mut app.wired_editor {
        wired_field_mut(editor, field).insert_str(&plain);
    }
}

fn handle_wired_events(app: &mut SettingsApp, events: &[Event], size: Size, theme: &Theme) {
    let lay = wired_layout(size, theme);
    for event in events {
        match event {
            Event::PointerButton(button)
                if button.state == ButtonState::Pressed && button.button == PointerButton::Left =>
            {
                let p = button.position;
                if lay.save.contains(p) {
                    app.save_wired_editor();
                } else if lay.cancel.contains(p) || !lay.card.contains(p) {
                    app.wired_editor = None;
                } else if lay.address.contains(p) {
                    wired_focus(app, 0);
                } else if lay.gateway.contains(p) {
                    wired_focus(app, 1);
                } else if lay.dns.contains(p) {
                    wired_focus(app, 2);
                }
            }
            Event::Key(key) if key.state == ButtonState::Pressed => {
                let field = wired_active_field(app);
                let clipboard = app
                    .wired_editor
                    .as_mut()
                    .and_then(|editor| clipboard_command(wired_field_mut(editor, field), key));
                if let Some(cmd) = clipboard {
                    apply_text_clipboard(app, cmd, TextPasteTarget::WiredEntry(field));
                    continue;
                }
                match key.keysym {
                    0xff1b => app.wired_editor = None,
                    0xff0d | 0xff8d => app.save_wired_editor(),
                    0xff08 => {
                        if let Some(editor) = &mut app.wired_editor {
                            wired_field_mut(editor, field).backspace();
                        }
                    }
                    0x61 if key.modifiers.ctrl => {
                        if let Some(editor) = &mut app.wired_editor {
                            wired_field_mut(editor, field).select_all();
                        }
                    }
                    _ => {
                        if !key.modifiers.ctrl {
                            if let Some(text) = &key.text {
                                wired_insert(app, field, text);
                            }
                        }
                    }
                }
            }
            Event::Text(text) => {
                let field = wired_active_field(app);
                wired_insert(app, field, text);
            }
            _ => {}
        }
    }
}

fn paint_wired_editor(app: &SettingsApp, ctx: &mut PaintCtx, size: Size) {
    let Some(editor) = &app.wired_editor else {
        return;
    };
    let theme = *ctx.theme;
    let lay = wired_layout(size, &theme);
    ctx.scene
        .fill_rect(Rect::from_size(size), Color::rgba(0.0, 0.0, 0.0, 0.45));
    ctx.scene
        .fill_rounded_rect(lay.card, theme.radius.large, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(lay.card, theme.radius.large, 1.0, theme.colors.border);
    ctx.label_left(
        lay.title.x(),
        lay.title.center().y,
        &tr("Wired IPv4"),
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );
    ctx.label_left(
        lay.subtitle.x(),
        lay.subtitle.center().y,
        &tr("Leave the address empty to use DHCP"),
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
    for (rect, caption) in [
        (lay.address_caption, tr("IPv4 address")),
        (lay.gateway_caption, tr("Gateway")),
        (lay.dns_caption, tr("DNS servers (comma separated)")),
    ] {
        ctx.label_left(
            rect.x(),
            rect.center().y,
            &caption,
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
    }
    editor.address.clone().paint(ctx, lay.address);
    editor.gateway.clone().paint(ctx, lay.gateway);
    editor.dns.clone().paint(ctx, lay.dns);
    let hint = match &editor.error {
        Some(error) => (error.clone(), theme.colors.warning),
        None => (
            tr("Prefix notation, e.g. 192.168.1.20/24"),
            theme.colors.dim_text,
        ),
    };
    ctx.label_left(
        lay.hint.x(),
        lay.hint.center().y,
        &fit_text(
            &hint.0,
            lay.hint.width(),
            theme.typography.caption.size,
            weight::NORMAL,
            ctx.text,
        ),
        theme.typography.caption.size,
        weight::NORMAL,
        hint.1,
    );
    Button::new(tr("Save"))
        .variant(ButtonVariant::Suggested)
        .paint(ctx, lay.save);
    Button::new(tr("Cancel")).paint(ctx, lay.cancel);
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_core::{KeyEvent, Modifiers, PointerEvent, ScrollEvent, Theme, ThemeSettings};

    fn key(keysym: u32, text: Option<&str>) -> Event {
        Event::Key(KeyEvent {
            keysym,
            keycode: 0,
            text: text.map(str::to_owned),
            state: ButtonState::Pressed,
            modifiers: Modifiers::NONE,
            repeat: false,
        })
    }

    #[test]
    fn info_button_opens_sheet_and_escape_closes() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1080.0, 760.0);
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.page = pages::Page::Workspaces;
        let lay = layout(size, &theme);
        let rect = info_button_rect(&lay, &theme);
        let click = Event::PointerButton(PointerEvent {
            position: rect.center(),
            button: PointerButton::Left,
            state: ButtonState::Pressed,
            modifiers: Modifiers::NONE,
        });
        handle_events(&mut app, &[click], size, &theme, &text);
        assert!(app.info_sheet.is_open(), "przycisk Info ma otworzyć panel");
        handle_events(&mut app, &[key(0xff1b, None)], size, &theme, &text);
        assert!(!app.info_sheet.is_open(), "Esc ma zamknąć panel Info");
    }

    #[test]
    fn account_form_paints_and_escape_closes() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1080.0, 760.0);
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.open_account_form(crate::app::AccountMode::AddUser);
        let mut scene = Scene::new();
        paint(&app, &mut scene, size, &theme, &text);
        assert!(!scene.is_empty());
        handle_events(&mut app, &[key(0xff1b, None)], size, &theme, &text);
        assert!(app.account_form.is_none());
    }

    #[test]
    fn wallpaper_picker_and_info_sheet_paint() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1080.0, 760.0);
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.open_wallpaper_picker();
        app.info_sheet.set_open(true);
        let mut scene = Scene::new();
        paint(&app, &mut scene, size, &theme, &text);
        assert!(!scene.is_empty());
    }

    fn wallpaper_temp_dir(count: usize) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "luna-wp-grid-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("katalog");
        for index in 0..count {
            std::fs::write(dir.join(format!("tapeta-{index}.png")), b"png").expect("png");
        }
        dir
    }

    #[test]
    fn wallpaper_grid_maps_clicks_to_tiles() {
        let theme = Theme::dark();
        let size = Size::new(1080.0, 760.0);
        let dir = wallpaper_temp_dir(5);
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.open_desktop_wallpaper_picker();
        app.wallpaper_picker.as_mut().unwrap().navigate(dir.clone());

        let lay = wallpaper_layout(&app, size, &theme);
        let (tile_w, tile_h) = wallpaper_tile_size(lay.grid);
        let at = |column: usize, row: usize| {
            Point::new(
                lay.grid.x() + column as f64 * (tile_w + WALLPAPER_GAP) + tile_w / 2.0,
                lay.grid.y() + row as f64 * (tile_h + WALLPAPER_GAP) + tile_h / 2.0,
            )
        };
        // Kafel 0 to „Więcej tapet”, potem katalog nadrzędny i pliki.
        assert_eq!(
            wallpaper_hit(&app, &lay, at(0, 0)),
            Some(WallpaperHit::More)
        );
        assert_eq!(
            wallpaper_hit(&app, &lay, at(2, 0)),
            Some(WallpaperHit::Entry(1))
        );
        assert_eq!(
            wallpaper_hit(&app, &lay, at(0, 1)),
            Some(WallpaperHit::Entry(2))
        );
        assert_eq!(
            wallpaper_hit(&app, &lay, at(1, 1)),
            Some(WallpaperHit::Entry(3))
        );
        assert_eq!(
            wallpaper_hit(&app, &lay, at(2, 1)),
            Some(WallpaperHit::Entry(4))
        );
        assert_eq!(
            wallpaper_hit(&app, &lay, at(0, 2)),
            Some(WallpaperHit::Entry(5))
        );
        // Przerwa między kaflami i obszar poza siatką nie są klikalne.
        assert_eq!(
            wallpaper_hit(
                &app,
                &lay,
                Point::new(
                    lay.grid.x() + tile_w + WALLPAPER_GAP / 2.0,
                    lay.grid.y() + 4.0
                )
            ),
            None
        );
        assert_eq!(wallpaper_hit(&app, &lay, Point::new(0.0, 0.0)), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn more_tile_opens_sheet_and_escape_closes_only_the_sheet() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1080.0, 760.0);
        let dir = wallpaper_temp_dir(1);
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.open_wallpaper_picker();
        app.wallpaper_picker.as_mut().unwrap().navigate(dir.clone());

        let lay = wallpaper_layout(&app, size, &theme);
        let click = Event::PointerButton(PointerEvent {
            position: Point::new(lay.grid.x() + 8.0, lay.grid.y() + 8.0),
            button: PointerButton::Left,
            state: ButtonState::Pressed,
            modifiers: Modifiers::NONE,
        });
        handle_events(&mut app, &[click], size, &theme, &text);
        assert!(
            app.wallpaper_picker
                .as_ref()
                .is_some_and(|picker| picker.more),
            "kafel „Więcej tapet” ma otworzyć arkusz"
        );

        handle_events(&mut app, &[key(0xff1b, None)], size, &theme, &text);
        let picker = app
            .wallpaper_picker
            .as_ref()
            .expect("picker zostaje otwarty");
        assert!(!picker.more, "Esc zamyka tylko arkusz");
        assert!(!picker.browse);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sheet_browse_row_switches_to_the_file_list() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1080.0, 760.0);
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.open_wallpaper_picker();
        let sources = app
            .wallpaper_picker
            .as_ref()
            .map(|picker| picker.sources.len())
            .unwrap_or(0);
        app.wallpaper_picker.as_mut().unwrap().more = true;
        let lay = wallpaper_layout(&app, size, &theme);
        // Ostatni wiersz arkusza to „Przeglądaj dysk…”.
        let row = lay.sheet_rows[sources + 1];
        let click = Event::PointerButton(PointerEvent {
            position: row.center(),
            button: PointerButton::Left,
            state: ButtonState::Pressed,
            modifiers: Modifiers::NONE,
        });
        handle_events(&mut app, &[click], size, &theme, &text);
        let picker = app.wallpaper_picker.as_ref().unwrap();
        assert!(picker.browse, "wiersz ma przełączyć na listę plików");
        assert!(!picker.more);
    }

    #[test]
    fn picker_typing_filters_and_escape_closes() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1080.0, 760.0);
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.open_picker(pages::PickerKind::Language);
        handle_events(
            &mut app,
            &[key(0x64, Some("d")), key(0x65, Some("e"))],
            size,
            &theme,
            &text,
        );
        let filtered = app.picker_filtered();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].value, "de");
        handle_events(&mut app, &[key(0xff1b, None)], size, &theme, &text);
        assert!(app.picker.is_none());
    }

    #[test]
    fn clipboard_works_in_picker_query() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1080.0, 760.0);
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.open_picker(pages::PickerKind::Language);
        {
            let picker = app.picker.as_mut().unwrap();
            picker.query.set_text("de");
            picker.query.select_all();
        }

        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };
        let copy = Event::Key(KeyEvent {
            keysym: 0x63,
            keycode: 0,
            text: None,
            state: ButtonState::Pressed,
            modifiers: ctrl,
            repeat: false,
        });
        handle_events(&mut app, &[copy], size, &theme, &text);
        assert_eq!(app.clipboard_text.as_deref(), Some("de"));

        let paste = Event::Key(KeyEvent {
            keysym: 0x76,
            keycode: 0,
            text: None,
            state: ButtonState::Pressed,
            modifiers: ctrl,
            repeat: false,
        });
        handle_events(&mut app, &[paste], size, &theme, &text);
        assert!(app.paste_requested);
        assert_eq!(app.paste_target, Some(TextPasteTarget::PickerQuery));

        handle_events(
            &mut app,
            &[Event::Paste("pl".to_owned())],
            size,
            &theme,
            &text,
        );
        assert_eq!(app.picker.as_ref().unwrap().query.text(), "pl");
    }

    #[test]
    fn workspace_editor_accepts_typing_and_escape_cancels() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1080.0, 760.0);
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.open_workspace_editor(0);
        handle_events(&mut app, &[key(0x4b, Some("K"))], size, &theme, &text);
        assert!(app.editor.as_ref().unwrap().entry.text().contains('K'));
        handle_events(&mut app, &[key(0xff1b, None)], size, &theme, &text);
        assert!(app.editor.is_none());
    }

    #[test]
    fn admin_prompt_paints_and_escape_closes() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1080.0, 760.0);
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.on_action(Action::ApplyGreeter);
        assert!(app.admin.is_some());
        let mut scene = Scene::new();
        paint(&app, &mut scene, size, &theme, &text);
        assert!(!scene.is_empty());
        handle_events(&mut app, &[key(0xff1b, None)], size, &theme, &text);
        assert!(app.admin.is_none());
    }

    #[test]
    fn wheel_scrolls_the_picker_down_and_back() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1080.0, 760.0);
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.timezones = (0..100).map(|i| format!("Region/City{i}")).collect();
        app.open_picker(crate::pages::PickerKind::Timezone);
        let lay = picker_layout(size, &theme);
        let at = lay.list.center();

        // Jeden „ząbek” kółka w dół (dy = 20 jednostek) przesuwa listę o dwa wiersze.
        let down = Event::Scroll(ScrollEvent {
            position: at,
            dx: 0.0,
            dy: SCROLL_UNIT,
        });
        handle_events(&mut app, &[down], size, &theme, &text);
        let scroll = app.picker.as_ref().expect("picker").scroll;
        assert!(
            (scroll - PICKER_ROW_H * 2.0).abs() < 0.01,
            "scroll={scroll}"
        );

        // Kółko w górę wraca na początek — ten sam znak co przewijanie strony.
        let up = Event::Scroll(ScrollEvent {
            position: at,
            dx: 0.0,
            dy: -SCROLL_UNIT,
        });
        handle_events(&mut app, &[up], size, &theme, &text);
        assert_eq!(app.picker.as_ref().expect("picker").scroll, 0.0);
    }

    #[test]
    fn wired_editor_paints_and_escape_closes() {
        use luna::network::{Device, DeviceKind, DeviceState};

        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1080.0, 760.0);
        let mut app = SettingsApp::new(&ThemeSettings::load());
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
        let mut scene = Scene::new();
        paint(&app, &mut scene, size, &theme, &text);
        assert!(!scene.is_empty());
        handle_events(&mut app, &[key(0xff1b, None)], size, &theme, &text);
        assert!(app.wired_editor.is_none());
    }

    #[test]
    fn displays_hero_selects_a_monitor() {
        use crate::pages::Monitor;

        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1080.0, 760.0);
        let mut app = SettingsApp::new(&ThemeSettings::load());
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
                modes: vec![],
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
                modes: vec![],
            },
        ];
        app.selected_monitor = 0;

        let mut scene = Scene::new();
        paint(&app, &mut scene, size, &theme, &text);
        assert!(!scene.is_empty());

        let lay = layout(size, &theme);
        let hero = displays_hero_rect(&app, lay.viewport).expect("hero");
        let tiles = monitor_tiles(&app);
        let rects = monitor_tile_rects(hero_inner(hero), &tiles);
        assert_eq!(rects.len(), 2);
        let click = Event::PointerButton(PointerEvent {
            position: rects[1].center(),
            button: PointerButton::Left,
            state: ButtonState::Pressed,
            modifiers: Modifiers::NONE,
        });
        handle_events(&mut app, &[click], size, &theme, &text);
        assert_eq!(app.selected_monitor, 1);
    }

    #[test]
    fn layout_keeps_content_inside_window() {
        let theme = Theme::dark();
        let lay = layout(Size::new(900.0, 600.0), &theme);
        assert!(lay.header.contains(Point::new(100.0, 10.0)));
        assert!(lay.nav.right() <= lay.sidebar.right());
        assert!(lay.viewport.right() <= 900.0);
        assert!(lay.viewport.bottom() <= 600.0);
        assert!(lay.viewport.height() > 0.0);
    }

    #[test]
    fn buttons_row_paints_and_clicks_the_right_action() {
        use luna::bluetooth::{Adapter, Device, Snapshot};

        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1080.0, 760.0);
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.demo = true;
        app.page = pages::Page::Bluetooth;
        app.bluetooth = Snapshot {
            available: true,
            adapters: vec![Adapter {
                path: "/org/bluez/hci0".to_owned(),
                name: "hci0".to_owned(),
                address: "AA".to_owned(),
                powered: true,
                discovering: false,
            }],
            devices: vec![Device {
                path: "/org/bluez/hci0/dev_1".to_owned(),
                adapter: "/org/bluez/hci0".to_owned(),
                address: "11".to_owned(),
                name: "Klawiatura".to_owned(),
                paired: true,
                connected: false,
                trusted: false,
                rssi: None,
                icon: None,
            }],
            error: None,
        };

        let groups = pages::groups(&app);
        let lay = layout(size, &theme);
        let hit = row_hits(&groups, lay.viewport, &theme, &text, 0.0)
            .into_iter()
            .find(|hit| hit.row.title == "Klawiatura")
            .expect("wiersz urządzenia");
        let rects = buttons_rects(
            match &hit.row.control {
                pages::Control::Buttons { labels, .. } => labels,
                other => panic!("oczekiwano przycisków, jest {other:?}"),
            },
            hit.control,
            &theme,
            &text,
        );
        assert_eq!(rects.len(), 2, "Połącz + Usuń");
        assert!(
            rects[0].right() <= rects[1].x(),
            "przyciski nie mogą się nakładać"
        );
        for rect in &rects {
            assert!(rect.x() >= hit.control.x());
            assert!(rect.right() <= hit.control.right() + 0.5);
            assert!(rect.y() >= hit.rect.y() && rect.bottom() <= hit.rect.bottom());
        }
        // Klik w drugi przycisk trafia w akcję „Remove”.
        let click = Event::PointerButton(PointerEvent {
            position: rects[1].center(),
            button: PointerButton::Left,
            state: ButtonState::Pressed,
            modifiers: Modifiers::NONE,
        });
        handle_events(&mut app, &[click], size, &theme, &text);
        assert!(
            app.action_tasks.len() + usize::from(app.bluetooth_snapshot.is_some()) > 0,
            "akcja ma się uruchomić w tle"
        );
    }

    #[test]
    fn shortcut_recording_captures_a_super_combination() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1080.0, 760.0);
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.demo = true;
        app.shortcut_record = Some(libluna_core::ShortcutAction::CloseWindow);
        let press = Event::Key(KeyEvent {
            keysym: 0x006b,
            keycode: 0,
            text: Some("k".to_owned()),
            state: ButtonState::Pressed,
            modifiers: Modifiers {
                logo: true,
                ..Modifiers::NONE
            },
            repeat: false,
        });
        handle_events(&mut app, &[press], size, &theme, &text);
        assert_eq!(
            app.shortcuts
                .binding(libluna_core::ShortcutAction::CloseWindow),
            libluna_core::shortcuts::Binding::parse("Super+K")
        );
        assert!(app.shortcut_record.is_none());
    }

    #[test]
    fn shortcut_recording_ignores_plain_keys_and_escape_cancels() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1080.0, 760.0);
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.demo = true;
        app.shortcut_record = Some(libluna_core::ShortcutAction::Minimize);
        handle_events(&mut app, &[key(0x0061, Some("a"))], size, &theme, &text);
        assert!(
            app.shortcut_record.is_some(),
            "sam klawisz bez modyfikatora nie kończy nagrywania"
        );
        handle_events(&mut app, &[key(0xff1b, None)], size, &theme, &text);
        assert!(app.shortcut_record.is_none(), "Esc anuluje nagrywanie");
    }

    #[test]
    fn shortcut_record_modal_paints() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1080.0, 760.0);
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.demo = true;
        app.page = pages::Page::Shortcuts;
        app.shortcut_record = Some(libluna_core::ShortcutAction::Overview);
        let mut scene = Scene::new();
        paint(&app, &mut scene, size, &theme, &text);
        assert!(!scene.is_empty());
    }

    #[test]
    fn every_page_paints_and_rows_hit() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let mut app = SettingsApp::new(&ThemeSettings::load());
        let size = Size::new(1080.0, 760.0);
        let lay = layout(size, &theme);
        for page in pages::Page::ALL {
            app.page = page;
            let groups = pages::groups(&app);
            let hits = row_hits(&groups, lay.viewport, &theme, &text, 0.0);
            assert!(!hits.is_empty(), "strona {page:?} bez wierszy");
            let mut scene = Scene::new();
            paint(&app, &mut scene, size, &theme, &text);
            assert!(!scene.is_empty());
        }
    }

    #[test]
    fn control_rects_stay_inside_rows() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let mut app = SettingsApp::new(&ThemeSettings::load());
        app.demo = true;
        let lay = layout(Size::new(1080.0, 760.0), &theme);
        for page in pages::Page::ALL {
            app.page = page;
            let groups = pages::groups(&app);
            for hit in row_hits(&groups, lay.viewport, &theme, &text, 0.0) {
                assert!(
                    hit.control.x() >= hit.rect.x() - 0.5
                        && hit.control.right() <= hit.rect.right() + 0.5,
                    "kontrolka poza wierszem na stronie {page:?}: {:?}",
                    hit.row.title
                );
            }
        }
    }
}
