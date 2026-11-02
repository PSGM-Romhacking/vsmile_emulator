//! The game screen: the picture, scaled and filtered, inside the chosen frame.

#[allow(unused_imports)]
use super::*;

impl App {
    /// Textures of the chosen frame: the bezel (with its game area) or the blurred cover.
    pub(super) fn frame_textures(&mut self, ctx: &egui::Context) -> (Option<(egui::TextureHandle, [f32; 4])>, Option<egui::TextureHandle>) {
        if !self.loaded {
            return (None, None);
        }
        if let Some(b) = crate::brand::get().bezels.iter().find(|b| b.key == self.frame) {
            let t = self.bezel_tex.entry(b.key).or_insert_with(|| {
                let img = image::load_from_memory(b.png).ok()?.to_rgba8();
                let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
                Some(ctx.load_texture(format!("bezel_{}", b.key), ci, egui::TextureOptions::LINEAR))
            });
            return (t.clone().map(|t| (t, b.screen)), None);
        }
        if self.frame != "cover" {
            return (None, None);
        }
        if let Some(game) = self.cur_game.clone()
            && self.cover_bg.as_ref().map(|(g, _)| g) != Some(&game)
        {
            // the same image as the library card (the user's own cover included)
            let (name, thumb) = self.game_name(&game);
            let tex = match self.game_art(crate::covers::Art::Boxart, &game, &name, thumb.as_deref(), false) {
                crate::covers::Status::Ready(p) => blurred_cover(ctx, &p),
                _ => None,
            };
            self.cover_bg = Some((game, tex));
        }
        (None, self.cover_bg.as_ref().and_then(|(_, t)| t.clone()))
    }

    pub(super) fn screen(&mut self, ui: &mut egui::Ui) {
        let l = self.look;
        let (bezel, cover) = self.frame_textures(ui.ctx());
        // with a game, the background around the image is black (like the TV); without one, the theme's
        let fill = if self.loaded { Color32::BLACK } else { l.p.bg_sidebar };
        egui::CentralPanel::default().frame(egui::Frame::new().fill(fill)).show(ui, |ui| {
            let rect = ui.max_rect();
            let painter = ui.painter();
            let Some(tex) = &self.tex else { return }; // first frame hasn't arrived yet
            // fill all the space keeping the aspect ratio (or integer multiples only,
            // if "Integer scaling" is on), always centered
            let ppp = ui.ctx().pixels_per_point();
            let disp = self.display_size();
            let full_uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            // frames only when there is room on the sides (fullscreen, maximized, wide window)
            let wide = rect.width() > rect.height() * disp.x / disp.y * 1.15;
            let bezel = bezel.filter(|_| wide);
            // with a bezel, its image fills the window (keeping its proportion) and
            // the game goes in its screen area; otherwise, the usual fit
            let (dst, size, s, bezel_rect) = match &bezel {
                Some((bt, scr)) => {
                    let iw = bt.size_vec2();
                    let br = egui::Rect::from_center_size(rect.center(), iw * (rect.width() / iw.x).min(rect.height() / iw.y));
                    let dst = egui::Rect::from_min_size(br.min + Vec2::new(scr[0] * br.width(), scr[1] * br.height()), Vec2::new(scr[2] * br.width(), scr[3] * br.height()));
                    (dst, dst.size(), dst.height() * ppp / disp.y, Some(br))
                }
                None => {
                    let fit = (rect.width() * ppp / disp.x).min(rect.height() * ppp / disp.y);
                    let s = if self.integer_scale { fit.floor() } else { fit }.max(1.0);
                    let size = disp * s / ppp;
                    let dst = egui::Rect::from_center_size(rect.center(), size);
                    let min = egui::pos2((dst.min.x * ppp).round() / ppp, (dst.min.y * ppp).round() / ppp); // align to the physical pixel
                    (egui::Rect::from_min_size(min, size), size, s, None)
                }
            };
            let [tw, th] = tex.size();
            // cover: blurred and darkened, filling the window (cropped, not stretched)
            if let Some(c) = cover.as_ref().filter(|_| wide) {
                let (ci, cr) = (c.size_vec2(), rect.size());
                let uv = if cr.x / cr.y > ci.x / ci.y {
                    let h = (ci.x / ci.y) / (cr.x / cr.y);
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.5 - h / 2.0), egui::pos2(1.0, 0.5 + h / 2.0))
                } else {
                    let w = (cr.x / cr.y) / (ci.x / ci.y);
                    egui::Rect::from_min_max(egui::pos2(0.5 - w / 2.0, 0.0), egui::pos2(0.5 + w / 2.0, 1.0))
                };
                painter.image(c.id(), rect, uv, Color32::WHITE);
            }
            painter.image(tex.id(), dst, full_uv, Color32::WHITE);
            // filters: dark lines between the game's lines (and phosphor grid on CRT)
            if self.filter != Filter::None && s >= 2.0 {
                let (rh, rw) = (size.y / th as f32, size.x / tw as f32);
                let (alpha, frac) = if self.filter == Filter::Crt { (70, 0.45) } else { (110, 0.4) };
                for y in 0..th {
                    let top = dst.top() + (y + 1) as f32 * rh - rh * frac;
                    painter.rect_filled(egui::Rect::from_min_size(egui::pos2(dst.left(), top), Vec2::new(size.x, rh * frac)), 0.0, Color32::from_black_alpha(alpha));
                }
                if self.filter == Filter::Crt {
                    for x in 0..tw {
                        let left = dst.left() + (x + 1) as f32 * rw - rw * 0.2;
                        painter.rect_filled(egui::Rect::from_min_size(egui::pos2(left, dst.top()), Vec2::new(rw * 0.2, size.y)), 0.0, Color32::from_black_alpha(35));
                    }
                }
            }
            // the bezel goes over the game: its hole shows the game, the glass and corners cover its edges
            if let (Some((bt, _)), Some(br)) = (&bezel, bezel_rect) {
                painter.image(bt.id(), br, full_uv, Color32::WHITE);
            }
            let tag = match self.speed {
                Speed::Fast if !self.paused => Some(trf("FAST FORWARD {n}x", &[("n", &self.ff_speed)])),
                Speed::Rewind if !self.paused => Some(tr("REWIND")),
                _ => None,
            };
            if let Some(t) = tag {
                let g = painter.layout_no_wrap(t, theme::bold(l.f.md), l.p.text_main);
                let r = egui::Rect::from_min_size(dst.right_top() + Vec2::new(-g.size().x - 28.0, 12.0), g.size() + Vec2::new(16.0, 8.0));
                painter.rect_filled(r, 0.0, l.p.bg_sidebar.gamma_multiply(0.85));
                painter.galley(r.min + Vec2::new(8.0, 4.0), g, l.p.text_main);
            }
            if let Some((msg, until)) = &self.toast
                && self.clock.elapsed().as_secs_f64() < *until {
                    let g = painter.layout_no_wrap(msg.clone(), egui::FontId::proportional(l.f.md), l.p.text_main);
                    let r = egui::Rect::from_min_size(dst.left_bottom() + Vec2::new(12.0, -g.size().y - 20.0), g.size() + Vec2::new(16.0, 8.0));
                    painter.rect_filled(r, 0.0, l.p.bg_sidebar.gamma_multiply(0.9));
                    painter.galley(r.min + Vec2::new(8.0, 4.0), g, l.p.text_main);
                }
            if let Some(r) = &self.recorder {
                let s = r.seconds() as u64;
                let g = painter.layout_no_wrap(format!("REC {:02}:{:02}", s / 60, s % 60), theme::bold(l.f.md), l.p.text_main);
                let r = egui::Rect::from_min_size(dst.right_top() + Vec2::new(-g.size().x - 44.0, 44.0), g.size() + Vec2::new(30.0, 8.0));
                painter.rect_filled(r, 0.0, l.p.bg_sidebar.gamma_multiply(0.85));
                painter.circle_filled(r.left_center() + Vec2::new(11.0, 0.0), 5.0, l.p.danger_text); // the only colored detail
                painter.galley(r.min + Vec2::new(22.0, 4.0), g, l.p.text_main);
            }
            if self.show_fps {
                let fps = if self.paused || self.focus_paused { 0.0 } else { self.fps_value };
                let pct = fps * self.frame_seconds() * 100.0;
                let g = painter.layout_no_wrap(format!("{fps:.1} fps · {pct:.0}%"), egui::FontId::monospace(l.f.base), l.p.text_main);
                let r = egui::Rect::from_min_size(dst.left_top() + Vec2::new(12.0, 12.0), g.size() + Vec2::new(12.0, 6.0));
                painter.rect_filled(r, 0.0, l.p.bg_sidebar.gamma_multiply(0.85));
                painter.galley(r.min + Vec2::new(6.0, 3.0), g, l.p.text_main);
            }
            if self.paused || self.focus_paused {
                let font = theme::bold(l.f.xl);
                let g = painter.layout_no_wrap(tr("PAUSED"), font, l.p.text_main);
                let r = egui::Rect::from_center_size(dst.center(), g.size() + Vec2::new(36.0, 16.0));
                painter.rect_filled(r, 0.0, l.p.bg_sidebar.gamma_multiply(0.92));
                painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0, l.p.border), egui::StrokeKind::Inside);
                painter.galley(r.center() - g.size() / 2.0, g, l.p.text_main);
            }
        });
    }
}
