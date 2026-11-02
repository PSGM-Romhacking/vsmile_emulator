//! The look: one dark-gray theme (Blender's colors),
//! applied to the egui style.
//!
//! User rule: **no rounded corners**, on any component.
//! Dark-gray aesthetic; color only in occasional details and buttons.

use egui::{Color32, CornerRadius, FontFamily, FontId, Shadow, Stroke, TextStyle, Visuals};

/// The app's single type scale (px). TV mode multiplies everything by 1.6.
#[derive(Clone, Copy, Debug)]
pub struct Fonts {
    pub sm: f32,
    pub base: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
    pub xxl: f32,
    pub xxxl: f32,
}

impl Fonts {
    pub fn scaled(scale: f32) -> Self {
        let f = |px: f32| if scale == 1.0 { px } else { (px * scale).round().max(9.0) };
        Self { sm: f(11.0), base: f(12.0), md: f(13.0), lg: f(14.0), xl: f(16.0), xxl: f(18.0), xxxl: f(24.0) }
    }
}

/// The theme's colors, already opaque over the background.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub bg_main: Color32,
    pub bg_sidebar: Color32,
    pub surface: Color32,
    pub border: Color32,
    pub border_focus: Color32,
    pub text_main: Color32,
    pub text_muted: Color32,
    pub text_section: Color32,
    pub text_accent: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub hover_bg: Color32,
    pub card_bg: Color32,
    pub card_border: Color32,
    pub danger: Color32,
    pub danger_hover: Color32,
    pub danger_text: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub error: Color32,
    pub ra_gold: Color32,
    /// Neutral highlight (selection, active item): gray, never the accent color.
    pub sel_bg: Color32,
}

const fn rgb(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// `rgb` over `base` with opacity `a`.
fn over(rgb: (u8, u8, u8), a: f32, base: Color32) -> Color32 {
    let m = |c: u8, b: u8| (c as f32 * a + b as f32 * (1.0 - a)).round() as u8;
    Color32::from_rgb(m(rgb.0, base.r()), m(rgb.1, base.g()), m(rgb.2, base.b()))
}

/// Blends `a` with `b` (`t` = weight of `b`).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let f = |x: u8, y: u8| (x as f32 * (1.0 - t) + y as f32 * t).round() as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

/// The palette, subdued: the accent stays only on the main buttons, and even then
/// faded; status colors (success, warning, error) pulled toward gray; main text slightly
/// grayed (pure white is tiring); selection and active items in a neutral highlight.
pub fn palette() -> Palette {
    let bg_main = rgb(0x303030);
    let bg_sidebar = rgb(0x282828);
    let text_muted = rgb(0x848484);
    let text_main = mix(rgb(0xffffff), bg_main, 0.17);
    let danger_text = rgb(0xfca5a5);
    Palette {
        bg_main,
        bg_sidebar,
        surface: bg_sidebar, // cards: solid surface, no border
        border: rgb(0x3d3d3d),
        border_focus: rgb(0x4772b3),
        text_main,
        text_muted,
        text_section: rgb(0xb8b8b8),
        text_accent: rgb(0x5db6ea),
        accent: mix(rgb(0x4772b3), bg_sidebar, 0.35),
        accent_hover: mix(rgb(0x3a5f99), bg_sidebar, 0.35),
        hover_bg: over((150, 150, 150), 0.1, bg_main),
        card_bg: over((100, 100, 100), 0.1, bg_main),
        card_border: over((150, 150, 150), 0.2, bg_main),
        danger: rgb(0x3f1515),
        danger_hover: rgb(0x7f1d1d),
        danger_text,
        success: mix(rgb(0x8bc34a), text_muted, 0.45),
        warning: mix(rgb(0xe6a23c), text_muted, 0.40),
        error: mix(danger_text, text_muted, 0.30),
        ra_gold: mix(rgb(0xf39c12), text_muted, 0.35),
        sel_bg: mix(bg_main, text_main, 0.16),
    }
}

pub const BOLD: &str = "bold";

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD.into()))
}

/// Where Unix systems keep their fonts: the Linux distros under `/usr/share`, the BSDs
/// (fonts installed from ports/pkgsrc) under `/usr/local/share`.
const FONT_ROOTS: [&str; 2] = ["/usr/share/fonts", "/usr/local/share/fonts"];

/// Reads a font: a path relative to the font folders is tried under each of
/// [`FONT_ROOTS`]; an absolute one (Windows) as it is.
fn read_font(path: &str) -> Option<Vec<u8>> {
    if path.starts_with('/') || path.contains(":\\") {
        return std::fs::read(path).ok();
    }
    FONT_ROOTS.iter().find_map(|root| std::fs::read(std::path::Path::new(root).join(path)).ok())
}

/// Loads a system font (Inter/Noto/DejaVu) for regular and bold,
/// if present; otherwise egui's built-in font stays. Local reads only.
pub fn install_fonts(ctx: &egui::Context) {
    let mut defs = egui::FontDefinitions::default();
    let pairs = [
        ("google-noto/NotoSans-Regular.ttf", "google-noto/NotoSans-Bold.ttf"),
        ("noto/NotoSans-Regular.ttf", "noto/NotoSans-Bold.ttf"),
        ("truetype/noto/NotoSans-Regular.ttf", "truetype/noto/NotoSans-Bold.ttf"),
        ("dejavu-sans-fonts/DejaVuSans.ttf", "dejavu-sans-fonts/DejaVuSans-Bold.ttf"),
        ("truetype/dejavu/DejaVuSans.ttf", "truetype/dejavu/DejaVuSans-Bold.ttf"),
        ("dejavu/DejaVuSans.ttf", "dejavu/DejaVuSans-Bold.ttf"),
        ("TTF/DejaVuSans.ttf", "TTF/DejaVuSans-Bold.ttf"),
        ("C:\\Windows\\Fonts\\segoeui.ttf", "C:\\Windows\\Fonts\\segoeuib.ttf"),
    ];
    let mut bold_family: Vec<String> = vec![];
    for (reg, bld) in pairs {
        if let (Some(r), Some(b)) = (read_font(reg), read_font(bld)) {
            defs.font_data.insert("ui-regular".into(), egui::FontData::from_owned(r).into());
            defs.font_data.insert("ui-bold".into(), egui::FontData::from_owned(b).into());
            defs.families.entry(FontFamily::Proportional).or_default().insert(0, "ui-regular".into());
            bold_family.push("ui-bold".into());
            break;
        }
    }
    // symbols (✓ ✕ ≠ →) the text font lacks: fallback fonts
    let fallbacks = [
        "google-noto/NotoSansSymbols2-Regular.ttf",
        "noto/NotoSansSymbols2-Regular.ttf",
        "truetype/noto/NotoSansSymbols2-Regular.ttf",
        "google-noto/NotoSansMath-Regular.ttf",
        "noto/NotoSansMath-Regular.ttf",
        "truetype/noto/NotoSansMath-Regular.ttf",
        "truetype/dejavu/DejaVuSans.ttf",
        "dejavu/DejaVuSans.ttf",
        "TTF/DejaVuSans.ttf",
        "gdouros-symbola/Symbola.ttf",
        "C:\\Windows\\Fonts\\seguisym.ttf",
    ];
    for (i, f) in fallbacks.iter().enumerate() {
        if let Some(b) = read_font(f) {
            let name = format!("ui-fallback-{i}");
            defs.font_data.insert(name.clone(), egui::FontData::from_owned(b).into());
            let prop = defs.families.entry(FontFamily::Proportional).or_default();
            let pos = prop.iter().position(|n| n == "ui-regular").map_or(0, |p| p + 1);
            prop.insert(pos, name.clone());
        }
    }
    // Japanese (original titles in the info sheets): embedded subset of Noto Sans CJK JP
    // (kana + the kanji in use, 46 KB; tools/fonts/generate.py, not in the repository), guaranteed on any
    // distro; then the system CJK fonts, for kanji the subset lacks
    let cjk: [(&str, u32); 6] = [
        ("google-droid-sans-fonts/DroidSansFallbackFull.ttf", 0),
        ("truetype/droid/DroidSansFallbackFull.ttf", 0),
        ("opentype/noto/NotoSansCJK-Regular.ttc", 0),
        ("noto-cjk/NotoSansCJK-Regular.ttc", 0),
        ("noto/NotoSansCJK-Regular.ttc", 0),
        ("C:\\Windows\\Fonts\\YuGothR.ttc", 0),
    ];
    let mut jp = vec![("ui-jp".to_string(), egui::FontData::from_static(include_bytes!("../../../assets/fonts/NotoSansJP-subset.ttf")))];
    for (i, (f, index)) in cjk.iter().enumerate() {
        if let Some(b) = read_font(f) {
            jp.push((format!("ui-cjk-{i}"), egui::FontData { index: *index, ..egui::FontData::from_owned(b) }));
            break; // one is enough: they're large
        }
    }
    for (name, data) in jp {
        defs.font_data.insert(name.clone(), data.into());
        defs.families.entry(FontFamily::Proportional).or_default().push(name);
    }
    // bold falls back to the proportional family (includes the symbols) when the font is missing
    let prop = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    bold_family.extend(prop);
    defs.families.insert(FontFamily::Name(BOLD.into()), bold_family);
    ctx.set_fonts(defs);
}

/// Applies theme and scale to the context.
pub fn apply(ctx: &egui::Context, scale: f32) {
    let p = palette();
    let f = Fonts::scaled(scale);
    let mut v = Visuals::dark();
    let zero = CornerRadius::ZERO;

    v.override_text_color = Some(p.text_main);
    v.panel_fill = p.bg_main;
    v.window_fill = p.bg_sidebar;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = zero;
    v.menu_corner_radius = zero;
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow::NONE;
    v.extreme_bg_color = p.bg_sidebar;
    v.text_edit_bg_color = Some(p.bg_sidebar);
    v.faint_bg_color = p.bg_sidebar;
    v.code_bg_color = p.bg_sidebar;
    v.hyperlink_color = p.text_accent;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.error;
    v.selection.bg_fill = p.sel_bg;
    v.selection.stroke = Stroke::new(1.0, p.text_main);
    v.striped = true;

    let w = &mut v.widgets;
    for wv in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        wv.corner_radius = zero;
        wv.expansion = 0.0;
    }
    w.noninteractive.bg_fill = p.bg_main;
    w.noninteractive.weak_bg_fill = p.bg_main;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text_main);

    w.inactive.bg_fill = p.bg_sidebar;
    w.inactive.weak_bg_fill = p.bg_main;
    w.inactive.bg_stroke = Stroke::new(1.0, p.border);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text_main);

    w.hovered.bg_fill = p.hover_bg;
    w.hovered.weak_bg_fill = p.hover_bg;
    w.hovered.bg_stroke = Stroke::new(1.0, p.text_muted);
    w.hovered.fg_stroke = Stroke::new(1.0, p.text_main);

    w.active.bg_fill = p.border;
    w.active.weak_bg_fill = p.border;
    w.active.bg_stroke = Stroke::new(1.0, p.text_muted);
    w.active.fg_stroke = Stroke::new(1.0, p.text_main);

    w.open.bg_fill = p.bg_sidebar;
    w.open.weak_bg_fill = p.bg_sidebar;
    w.open.bg_stroke = Stroke::new(1.0, p.border);
    w.open.fg_stroke = Stroke::new(1.0, p.text_main);

    // the same visuals for both egui themes: this theme is what decides
    ctx.set_visuals_of(egui::Theme::Dark, v.clone());
    ctx.set_visuals_of(egui::Theme::Light, v);

    ctx.all_styles_mut(|s| {
        use FontFamily::{Monospace, Proportional};
        s.text_styles = [
            (TextStyle::Small, FontId::new(f.sm, Proportional)),
            (TextStyle::Body, FontId::new(f.md, Proportional)),
            (TextStyle::Button, FontId::new(f.md, Proportional)),
            (TextStyle::Heading, FontId::new(f.xxl, FontFamily::Name(BOLD.into()))),
            (TextStyle::Monospace, FontId::new(f.base, Monospace)),
        ]
        .into();
        s.spacing.button_padding = egui::vec2(12.0, 6.0) * scale.max(1.0);
        s.spacing.interact_size.y = 32.0 * scale.max(1.0) * 0.75;
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        // bar always visible, with its own space (doesn't cover the content): subtle
        // gray handle (text at 35%) that brightens on mouse hover. egui's
        // "solid" style paints handle and track the same color in this theme.
        s.spacing.scroll = egui::style::ScrollStyle {
            bar_width: 8.0,
            floating_width: 8.0,
            floating_allocated_width: 8.0,
            foreground_color: true,
            dormant_background_opacity: 1.0,
            active_background_opacity: 1.0,
            interact_background_opacity: 1.0,
            dormant_handle_opacity: 0.35,
            active_handle_opacity: 0.5,
            interact_handle_opacity: 0.8,
            ..egui::style::ScrollStyle::floating()
        };
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Japanese titles in the info sheets have glyphs (no boxes).
    #[test]
    fn japanese_has_glyphs() {
        let ctx = egui::Context::default();
        install_fonts(&ctx);
        // fonts take effect from the frame on; the output's texture deltas are cleared, as the
        // renderer would apply them (epaint panics when they are dropped unapplied)
        ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
        for font in [egui::FontId::proportional(14.0), bold(14.0)] {
            assert!(ctx.fonts_mut(|f| f.has_glyphs(&font, "プーヤン エキサイト麻雀 ワープ&ワープ")), "{font:?}");
        }
        // only the embedded subset, as on a distro without a Japanese font installed
        let ctx = egui::Context::default();
        let mut defs = egui::FontDefinitions::empty();
        defs.font_data.insert("jp".into(), egui::FontData::from_static(include_bytes!("../../../assets/fonts/NotoSansJP-subset.ttf")).into());
        defs.families.insert(FontFamily::Proportional, vec!["jp".into()]);
        ctx.set_fonts(defs);
        ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
        assert!(ctx.fonts_mut(|f| f.has_glyphs(&egui::FontId::proportional(14.0), "プーヤンエキサイト麻雀ワープ")));
    }

    #[test]
    fn translucent_colors_are_blended_over_background() {
        assert_eq!(over((200, 100, 0), 0.5, Color32::BLACK), Color32::from_rgb(100, 50, 0));
    }

    #[test]
    fn tv_mode_scale() {
        let f = Fonts::scaled(1.6);
        assert_eq!(f.md, (13.0f32 * 1.6).round());
        assert_eq!(Fonts::scaled(1.0).md, 13.0);
    }
}
