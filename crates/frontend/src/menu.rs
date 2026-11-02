//! Menu items in the style of desktop programs (KDE/GTK): full row with
//! hover highlight, left column for ✓ / •, shortcut right-aligned in
//! gray and an arrow on submenus. egui only handles opening/closing the menus.

use egui::containers::menu::{SubMenu, SubMenuButton};
use egui::{Align2, FontId, Margin, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, pos2, vec2};

use crate::widgets::Look;

const ROW_H: f32 = 26.0;
const GUTTER: f32 = 30.0; // column for the ✓ / • mark
const MIN_W: f32 = 240.0;

pub enum Mark {
    None,
    Check(bool),
    Radio(bool),
}

/// Style of the bar and menus: rows packed together, vertical margin only.
pub fn style(s: &mut egui::Style) {
    egui::containers::menu::menu_style(s);
    s.spacing.menu_margin = Margin::symmetric(0, 4);
    s.spacing.item_spacing.y = 0.0;
    s.spacing.menu_spacing = 0.0;
}

/// Prepares a menu's content: rows packed together and minimum width.
/// (egui popups don't inherit the bar's style, so it goes here)
pub fn begin(ui: &mut Ui) {
    ui.spacing_mut().item_spacing.y = 0.0;
    ui.set_min_width(MIN_W);
}

/// Row width: the default, or more if text + shortcut doesn't fit.
fn width(ui: &Ui, l: &Look, text: &str, shortcut: &str, mono: bool) -> f32 {
    let font = FontId::proportional(l.f.md);
    let tw = |t: &str, font: FontId| ui.fonts_mut(|f| f.layout_no_wrap(t.to_owned(), font, egui::Color32::WHITE).size().x);
    let right = if shortcut.is_empty() { 0.0 } else { 32.0 + tw(shortcut, right_font(l, mono)) };
    let need = GUTTER + tw(text, font) + right + 34.0;
    need.max(MIN_W).max(ui.min_rect().width())
}

/// Draws the row; returns the response and the background index (filled in later,
/// once it's known whether it's hovered or has an open submenu).
/// Font of the right column: the shortcut, or a monospace detail (sizes, hashes) whose
/// columns line up from one row to the next.
fn right_font(l: &Look, mono: bool) -> FontId {
    if mono { FontId::monospace(l.f.md * 0.92) } else { FontId::proportional(l.f.md) }
}

#[allow(clippy::too_many_arguments)]
fn row(ui: &mut Ui, l: &Look, text: &str, shortcut: &str, mark: Mark, enabled: bool, arrow: bool, mono: bool) -> (Response, egui::layers::ShapeIdx, Rect) {
    let w = width(ui, l, text, shortcut, mono);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2(w, ROW_H), sense);
    let painter = ui.painter();
    let bg = painter.add(Shape::Noop);
    let fg = if enabled { l.p.text_main } else { l.p.text_muted };
    let cy = rect.center().y;
    let x = rect.left();
    match mark {
        Mark::Check(true) => {
            let pts = vec![pos2(x + 10.0, cy), pos2(x + 14.0, cy + 4.0), pos2(x + 21.0, cy - 5.0)];
            painter.add(Shape::line(pts, Stroke::new(1.8, fg)));
        }
        Mark::Radio(true) => {
            painter.circle_filled(pos2(x + 15.5, cy), 3.5, fg);
        }
        _ => {}
    }
    let font = FontId::proportional(l.f.md);
    painter.text(pos2(x + GUTTER, cy), Align2::LEFT_CENTER, text, font.clone(), fg);
    if arrow {
        let a = Rect::from_center_size(pos2(rect.right() - 14.0, cy), Vec2::splat(12.0));
        painter.add(SubMenuButton::arrow_shape(a, fg));
    } else if !shortcut.is_empty() {
        painter.text(pos2(rect.right() - 14.0, cy), Align2::RIGHT_CENTER, shortcut, right_font(l, mono), l.p.text_muted);
    }
    (resp, bg, rect)
}

fn highlight(ui: &Ui, l: &Look, bg: egui::layers::ShapeIdx, rect: Rect) {
    ui.painter().set(bg, Shape::rect_filled(rect, 0.0, l.p.sel_bg));
}

/// Clickable item; closes the menu on click.
pub fn item(ui: &mut Ui, l: &Look, text: &str, shortcut: &str, mark: Mark, enabled: bool) -> bool {
    item_with(ui, l, text, shortcut, mark, enabled, false)
}

/// Clickable item whose right column is a detail in a monospace font (sizes, hashes),
/// aligned with the rows above and below.
pub fn item_mono(ui: &mut Ui, l: &Look, text: &str, detail: &str, mark: Mark) -> bool {
    item_with(ui, l, text, detail, mark, true, true)
}

fn item_with(ui: &mut Ui, l: &Look, text: &str, shortcut: &str, mark: Mark, enabled: bool, mono: bool) -> bool {
    let (resp, bg, rect) = row(ui, l, text, shortcut, mark, enabled, false, mono);
    if enabled && resp.hovered() {
        highlight(ui, l, bg, rect);
    }
    if resp.clicked() {
        ui.close();
        return true;
    }
    false
}

/// Informational text (disabled), e.g. "no games in this folder".
pub fn note(ui: &mut Ui, l: &Look, text: &str) {
    row(ui, l, text, "", Mark::None, false, false, false);
}

/// Submenu with an arrow; opens on mouse hover.
pub fn submenu<R>(ui: &mut Ui, l: &Look, text: &str, enabled: bool, content: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    let (resp, bg, rect) = row(ui, l, text, "", Mark::None, enabled, true, false);
    if !enabled {
        return None;
    }
    let r = SubMenu::new().show(ui, &resp, |ui| {
        begin(ui);
        content(ui)
    });
    if r.is_some() || resp.hovered() {
        highlight(ui, l, bg, rect);
    }
    r.map(|r| r.inner)
}

pub fn separator(ui: &mut Ui, l: &Look) {
    let w = MIN_W.max(ui.min_rect().width());
    let (rect, _) = ui.allocate_exact_size(vec2(w, 9.0), Sense::hover());
    let y = rect.center().y.round() + 0.5;
    ui.painter().line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0, l.p.border));
}

/// Item with a thumbnail (save state slots): image on the left, text beside it.
/// Without an image, draws an empty box of the same size.
pub fn item_thumb(ui: &mut Ui, l: &Look, tex: Option<egui::TextureId>, text: &str, mark: Mark) -> bool {
    const TH: f32 = 56.0; // thumbnail (224 / 4)
    let font = FontId::proportional(l.f.md);
    let tw = ui.fonts_mut(|f| f.layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::WHITE).size().x);
    let w = (GUTTER + TH + 12.0 + tw + 24.0).max(MIN_W).max(ui.min_rect().width());
    let (rect, resp) = ui.allocate_exact_size(vec2(w, TH + 8.0), Sense::click());
    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_filled(rect, 0.0, l.p.sel_bg);
    }
    let cy = rect.center().y;
    if let Mark::Radio(true) = mark {
        painter.circle_filled(pos2(rect.left() + 15.5, cy), 3.5, l.p.text_main);
    }
    let img = Rect::from_min_size(pos2(rect.left() + GUTTER, rect.top() + 4.0), Vec2::splat(TH));
    match tex {
        Some(t) => {
            painter.image(t, img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
        None => {
            painter.rect_filled(img, 0.0, l.p.bg_main);
        }
    }
    painter.rect_stroke(img, 0.0, Stroke::new(1.0, l.p.border), egui::StrokeKind::Outside);
    painter.text(pos2(img.right() + 12.0, cy), Align2::LEFT_CENTER, text, font, l.p.text_main);
    if resp.clicked() {
        ui.close();
        return true;
    }
    false
}
