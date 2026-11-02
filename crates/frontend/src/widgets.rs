//! Small UI pieces shared by the windows.

use egui::{Align, Button, Color32, CornerRadius, FontId, Frame, Layout, Margin, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::i18n::tr;

use crate::theme::{Fonts, Palette, bold};

/// Current palette + type scale.
#[derive(Clone, Copy)]
pub struct Look {
    pub p: Palette,
    pub f: Fonts,
    pub scale: f32,
}

impl Look {
    /// Section label: small, uppercase, faded.
    pub fn section(&self, ui: &mut Ui, text: &str) {
        ui.label(RichText::new(text.to_uppercase()).font(bold(self.f.sm)).color(self.p.text_section).extra_letter_spacing(1.0));
    }

    pub fn hint(&self, ui: &mut Ui, text: &str) -> egui::Response {
        ui.label(RichText::new(text).size(self.f.base).color(self.p.text_muted))
    }

    pub fn hint_rt(&self, text: impl Into<String>) -> RichText {
        RichText::new(text).size(self.f.base).color(self.p.text_muted)
    }

    pub fn strong(&self, text: impl Into<String>) -> RichText {
        RichText::new(text).font(bold(self.f.md)).color(self.p.text_main)
    }

    /// Primary action: background in the theme's accent.
    pub fn primary(&self, ui: &mut Ui, text: &str) -> egui::Response {
        let b = Button::new(RichText::new(text).font(bold(self.f.md)).color(self.p.text_main)).fill(self.p.accent).stroke(Stroke::new(1.0, self.p.accent));
        ui.add(b.min_size(Vec2::new(0.0, self.btn_h())))
    }

    pub fn btn_h(&self) -> f32 {
        32.0 * self.scale.max(1.0)
    }

    pub fn separator(&self, ui: &mut Ui) {
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(Vec2::new(w, 1.0), Sense::hover());
        ui.painter().rect_filled(r, 0.0, self.p.border);
    }

    // ---------------------------------------------- tool windows (debugger, cheats)

    /// Top strip of a tool window (toolbar + tabs), on the sidebar color.
    pub fn strip(&self, ui: &mut Ui, body: impl FnOnce(&mut Ui)) {
        Frame::new().fill(self.p.bg_sidebar).inner_margin(Margin::symmetric(14, 0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add_space(10.0);
            body(ui);
        });
        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
        ui.painter().rect_filled(r, 0.0, self.p.border);
    }

    /// Underlined tabs (source text, translated here).
    pub fn tabs(&self, ui: &mut Ui, titles: &[&str], current: &mut usize) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (i, t) in titles.iter().enumerate() {
                let active = *current == i;
                let font = if active { bold(self.f.md) } else { FontId::proportional(self.f.md) };
                let galley = ui.painter().layout_no_wrap(tr(t), font, if active { self.p.text_main } else { self.p.text_muted });
                let (rect, resp) = ui.allocate_exact_size(galley.size() + Vec2::new(24.0, 16.0), Sense::click());
                if resp.hovered() && !active {
                    ui.painter().rect_filled(rect, 0.0, self.p.hover_bg);
                }
                ui.painter().galley(rect.center() - galley.size() / 2.0, galley, self.p.text_main);
                if active {
                    ui.painter().rect_filled(Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - 2.0), rect.max), 0.0, self.p.border_focus);
                }
                if resp.clicked() {
                    *current = i;
                }
            }
        });
    }

    /// A panel on the sidebar color with an outline; `title` as a section label.
    pub fn card<R>(&self, ui: &mut Ui, title: Option<&str>, body: impl FnOnce(&mut Ui) -> R) -> R {
        Frame::new()
            .fill(self.p.bg_sidebar)
            .stroke(Stroke::new(1.0, self.p.border))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 6.0;
                if let Some(t) = title {
                    self.section(ui, t);
                    ui.add_space(2.0);
                }
                body(ui)
            })
            .inner
    }

    /// Column header of a table in a card.
    pub fn column(&self, text: &str) -> RichText {
        RichText::new(text.to_uppercase()).font(bold(self.f.sm)).color(self.p.text_section).extra_letter_spacing(0.5)
    }

    /// Small square: an on/off light, a color swatch, a legend.
    pub fn swatch(&self, ui: &mut Ui, fill: Color32, outline: Color32) {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
        ui.painter().rect_filled(r, 0.0, fill);
        ui.painter().rect_stroke(r, CornerRadius::ZERO, Stroke::new(1.0, outline), StrokeKind::Inside);
    }

    /// Dialog skeleton: title, subtitle, separator, content and footer.
    pub fn dialog<R>(&self, ctx: &egui::Context, id: &str, size: Vec2, title: &str, subtitle: &str, body: impl FnOnce(&mut Ui) -> R) -> egui::ModalResponse<R> {
        let frame = Frame::new().fill(self.p.bg_sidebar).stroke(Stroke::new(1.0, self.p.border)).inner_margin(Margin::symmetric(24, 20));
        egui::Modal::new(egui::Id::unique(id)).frame(frame).backdrop_color(Color32::from_black_alpha(140)).show(ctx, |ui| {
            ui.set_width(size.x - 48.0);
            ui.set_min_height(size.y - 40.0);
            ui.set_max_height(size.y - 40.0);
            ui.spacing_mut().item_spacing.y = 12.0;
            ui.add(egui::Label::new(RichText::new(title).font(bold(self.f.xxl)).color(self.p.text_main)).wrap());
            if !subtitle.is_empty() {
                ui.add(egui::Label::new(self.hint_rt(subtitle)).wrap());
            }
            self.separator(ui);
            body(ui)
        })
    }

    /// Dialog footer row, always anchored at the bottom.
    pub fn footer(&self, ui: &mut Ui, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.horizontal(|ui| {
                left(ui);
                ui.with_layout(Layout::right_to_left(Align::Center), right);
            });
        });
    }
}
