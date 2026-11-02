//! The library (what the window shows without a game), game identification and info
//! sheets.

#[allow(unused_imports)]
use super::*;

impl App {
    pub(super) fn identify(&self, path: &Path, rom: &[u8]) -> Ident {
        let sheet = self.sheets.for_rom(rom);
        Ident {
            mtime: std::fs::metadata(path).and_then(|m| m.modified()).ok(),
            title: sheet.as_ref().map(|s| s.title.clone()).filter(|t| !t.is_empty()),
            thumbnail: sheet.as_ref().and_then(|s| s.thumbnail.clone()),
            sheet,
        }
    }

    /// Game name to display and the cover name in libretro-thumbnails, if the
    /// info sheet has one. Reads the file only when it changes.
    pub(super) fn game_name(&mut self, path: &Path) -> (String, Option<String>) {
        let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        let id = match self.ids.get(path) {
            Some(i) if i.mtime == mtime => i.clone(),
            _ => {
                let id = match crate::rom::read(path, self.sys.multi_file_roms) {
                    Ok(rom) => self.identify(path, &rom),
                    Err(_) => Ident { mtime, title: None, thumbnail: None, sheet: None },
                };
                self.ids.insert(path.to_path_buf(), id.clone());
                id
            }
        };
        (display(path, &id), id.thumbnail)
    }

    /// Opens the game info sheet (Help › Game info, or right-click in the library).
    pub(super) fn open_info(&mut self, path: &Path) {
        let rom = match crate::rom::read(path, self.sys.multi_file_roms) {
            Ok(r) => r,
            Err(e) => {
                self.dialog = Dialog::Error(e);
                return;
            }
        };
        // no info sheet: just the file name and the key for anyone who wants to create one
        let sheet = self.sheets.for_rom(&rom).unwrap_or_else(|| crate::sheets::Sheet { title: stem(path), key: format!("crc:{:08x}", emu_api::crc32(&rom)), ..Default::default() });
        // same image as the library card
        let (name, thumb) = self.game_name(path);
        let cover = match self.game_art(crate::covers::Art::Boxart, path, &name, thumb.as_deref(), false) {
            crate::covers::Status::Ready(p) => Some(p),
            _ => None,
        };
        self.dialog = Dialog::Info(Box::new(sheet), cover, path.to_path_buf());
    }

    /// A game's image. First the user's own, made by hand, under any name the user may
    /// have given it: the name shown, the libretro (No-Intro) name or the ROM file's name.
    /// Then the downloaded one, by the libretro name (no "thumbnail", as homebrews and
    /// hacks: by the name shown). `fetch` queues the download when it is missing.
    pub(super) fn game_art(&self, art: crate::covers::Art, path: &Path, name: &str, thumb: Option<&str>, fetch: bool) -> crate::covers::Status {
        let file = stem(path);
        let names: Vec<&str> = [Some(name), thumb, Some(file.as_str())].into_iter().flatten().collect();
        match crate::covers::own_any(art, &names) {
            Some(p) => crate::covers::Status::Ready(p),
            None => self.covers.get(art, thumb.unwrap_or(name), fetch),
        }
    }

    /// Texture of a cached cover/screen (loaded once).
    pub(super) fn cover_texture(&mut self, ctx: &egui::Context, path: &Path, art: crate::covers::Art) -> Option<egui::TextureHandle> {
        if let Some(t) = self.cover_tex.get(path) {
            return Some(t.clone());
        }
        let img = image::open(path).ok()?.to_rgba8();
        let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
        // images are downscaled in the grid: smoothing looks better than skipped pixels
        let _ = art;
        let opts = egui::TextureOptions::LINEAR;
        let t = ctx.load_texture(path.to_string_lossy(), ci, opts);
        self.cover_tex.insert(path.to_path_buf(), t.clone());
        Some(t)
    }

    /// Library: what the window shows when no game is loaded (like DuckStation/PCSX2;
    /// "Close game" returns to it). Cover (portrait, 7 × 2) or title / in-game screen
    /// (landscape 4:3, 6 × 3), with the same grid width.
    pub(super) fn library_view(&mut self, ui: &mut egui::Ui) {
        use crate::covers::{Art, Status};
        let ctx = ui.ctx().clone();
        let l = self.look;
        let art = self.library_art;
        let q = self.library_filter.to_lowercase();
        // official name (by hash) when the ROM is recognized: finds the cover even if the file was renamed
        let named: Vec<(PathBuf, String, Option<String>)> = self.games.clone().into_iter().map(|g| { let (n, t) = self.game_name(&g); (g, n, t) }).collect();
        let mut games: Vec<(PathBuf, String, Option<String>)> = named.into_iter().filter(|(g, n, _)| q.is_empty() || n.to_lowercase().contains(&q) || g.file_stem().is_some_and(|s| s.to_string_lossy().to_lowercase().contains(&q))).collect();
        let list = self.library_list;
        if list {
            let (col, asc) = self.list_sort;
            games.sort_by(|(ga, na, _), (gb, nb, _)| {
                let key = |g: &PathBuf, n: &str| list_key(col, g, n, self.ids.get(g).and_then(|i| i.sheet.as_ref()), &self.usage);
                key(ga, na).cmp(&key(gb, nb))
            });
            if !asc {
                games.reverse();
            }
        }
        let sheets: Vec<Option<crate::sheets::Sheet>> = games.iter().map(|(g, _, _)| self.ids.get(g).and_then(|i| i.sheet.clone())).collect();
        let cards: Vec<(PathBuf, String, Option<egui::TextureHandle>, bool)> = games
            .into_iter()
            .map(|(g, name, thumb)| {
                if list {
                    return (g, name, None, false); // the list has no images: nothing to download
                }
                let status = self.game_art(art, &g, &name, thumb.as_deref(), true);
                let (tex, pending) = match status {
                    Status::Ready(p) => (self.cover_texture(&ctx, &p, art), false),
                    Status::Pending => (None, true),
                    Status::Missing => (None, false),
                };
                (g, name, tex, pending)
            })
            .collect();

        // cover: portrait (7 × 2); title and screen: landscape 4:3 (6 × 3); bigger in couch mode
        let k = if self.couch { 1.5 } else { 1.0 };
        let (img_w, img_h) = if art == Art::Boxart { (cover_w(self.sys) * k, COVER_H * k) } else { (SHOT_W * k, SHOT_H * k) };
        let (card_w, card_h) = (img_w + 12.0, img_h + 6.0 + 34.0 * k);
        let (mut pick, mut folder, mut new_art, mut info) = (None, false, None, None);
        let mut history: Option<PathBuf> = None;
        let (mut new_list, mut sort_click): (Option<bool>, Option<ListCol>) = (None, None);
        // controller / arrow keys (nav.rs); the quick menu takes them while open
        let browse = self.nav.quick.is_none() && self.dialog == Dialog::None && !cards.is_empty();
        if self.games.is_empty() && self.nav.quick.is_none() && self.dialog == Dialog::None && self.nav.pressed(nav::NavKey::Ok) {
            folder = true;
        }
        if browse && self.nav.pressed(nav::NavKey::Alt) {
            self.next_library_art();
        }
        if browse && self.nav.pressed(nav::NavKey::Ok) {
            pick = cards.get(self.nav.sel.min(cards.len() - 1)).map(|c| c.0.clone());
            self.nav.block_input = true;
        }
        let missing = self.missing_bios();
        let mut bios = false;
        let mut filter = std::mem::take(&mut self.library_filter);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(l.p.bg_main).inner_margin(egui::Margin { left: 24, right: 24, top: 16, bottom: 8 })).show(ui, |ui| {
            // header: image type on the left; search and change folder on the right
            ui.horizontal(|ui| {
                for a in Art::ALL {
                    let on = !list && art == a;
                    let txt = RichText::new(tr(a.label())).size(l.f.lg);
                    if ui.selectable_label(on, if on { txt.strong() } else { txt }).clicked() {
                        new_art = Some(a);
                        new_list = Some(false);
                    }
                }
                let txt = RichText::new(tr("List")).size(l.f.lg);
                if ui.selectable_label(list, if list { txt.strong() } else { txt }).clicked() {
                    new_list = Some(true);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr("Change folder…")).on_hover_text(trf("Folder: {folder}", &[("folder", &self.rom_dir.display())])).clicked() {
                        folder = true;
                    }
                    if !self.sys.bios.is_empty() {
                        let where_ = crate::config::Folder::Bios.dir().map_or_else(|| "?".to_string(), |d| d.display().to_string());
                        bios |= ui.button(tr("BIOS folder…")).on_hover_text(trf("Folder: {folder}", &[("folder", &where_)])).clicked();
                    }
                    let hint = trf("Search {n} games…", &[("n", &self.games.len())]);
                    ui.add(egui::TextEdit::singleline(&mut filter).hint_text(hint).desired_width(220.0));
                });
            });
            ui.add_space(8.0);
            l.separator(ui);
            ui.add_space(10.0);
            // no game starts without the BIOS: say so above the games, with the way out
            if !missing.is_empty() {
                l.card(ui, Some(&tr("BIOS MISSING")), |ui| {
                    ui.label(RichText::new(trf("Without these system files no game starts: {files}.", &[("files", &missing.join(", "))])).color(l.p.warning));
                    l.hint(ui, &tr("Put them in the BIOS folder or in the ROM folder (loose or in a .zip; any name), or choose the folder where they are."));
                    ui.add_space(6.0);
                    bios |= l.primary(ui, &tr("Choose BIOS folder…")).clicked();
                });
                ui.add_space(10.0);
            }

            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                if cards.is_empty() {
                    if self.games.is_empty() {
                        folder |= self.welcome(ui);
                    } else {
                        ui.add_space(40.0);
                        ui.vertical_centered(|ui| {
                            l.hint(ui, &tr("No game matches the search"));
                        });
                    }
                }
                // the selection moves by the grid that will be drawn: as many columns as fit
                let avail = ui.available_width();
                let cols = if list { 1 } else { (((avail + CARD_GAP) / (card_w + CARD_GAP)).floor() as usize).max(1) };
                // the width left over after the last column that fits is shared among the gaps,
                // so the grid spans the whole width instead of leaving an empty strip on the
                // right (rows below stay aligned to the columns). With fewer games than
                // columns, the normal gap. The half pixel keeps rounding from wrapping early.
                let gap_x = if cols > 1 && cards.len() >= cols {
                    let spare = avail - cols as f32 * card_w - (cols - 1) as f32 * CARD_GAP;
                    CARD_GAP + spare / (cols - 1) as f32 - 0.5
                } else {
                    CARD_GAP
                };
                if browse {
                    let (n, sel) = (cards.len(), self.nav.sel.min(cards.len() - 1));
                    let moved = if self.nav.pressed(nav::NavKey::Left) {
                        sel.saturating_sub(1)
                    } else if self.nav.pressed(nav::NavKey::Right) {
                        (sel + 1).min(n - 1)
                    } else if self.nav.pressed(nav::NavKey::Up) {
                        sel.saturating_sub(cols)
                    } else if self.nav.pressed(nav::NavKey::Down) {
                        (sel + cols).min(n - 1)
                    } else {
                        sel
                    };
                    self.nav.moved |= moved != self.nav.sel;
                    self.nav.sel = moved;
                }
                let (focus, scroll) = (self.nav.active.then_some(self.nav.sel), std::mem::take(&mut self.nav.moved));
                if list && !cards.is_empty() {
                    let row_h = 30.0 * k;
                    let font = egui::FontId::proportional(l.f.md * k);
                    let w = ui.available_width();
                    let head_font = crate::theme::bold(l.f.md * k);
                    let text_w = |s: &str, f: &egui::FontId| ui.fonts_mut(|fonts| fonts.layout_no_wrap(s.to_owned(), f.clone(), Color32::WHITE).size().x);
                    // each column as wide as its widest text (header with room for the sort arrow)
                    let need: Vec<f32> = ListCol::ALL
                        .iter()
                        .map(|&c| {
                            let head = text_w(&format!("{}  ▲", tr(c.label())), &head_font);
                            let cells = cards.iter().zip(&sheets).map(|((g, name, _, _), s)| text_w(&list_text(c, g, name, s.as_ref(), &self.usage), &font)).fold(0.0, f32::max);
                            head.max(cells) + 24.0
                        })
                        .collect();
                    let widths = fit_columns(&need, w);
                    let cell_x = |c: ListCol| ListCol::ALL.iter().zip(&widths).take_while(|(x, _)| **x != c).map(|(_, cw)| cw).sum::<f32>();
                    let col_w = |c: ListCol| widths[ListCol::ALL.iter().position(|&x| x == c).unwrap_or(0)];
                    // header: click a column to sort by it, again to reverse
                    let (hr, _) = ui.allocate_exact_size(Vec2::new(w, row_h), Sense::hover());
                    ui.painter().rect_filled(hr, 0.0, l.p.bg_sidebar);
                    for c in ListCol::ALL {
                        let r = egui::Rect::from_min_size(egui::pos2(hr.left() + cell_x(c), hr.top()), Vec2::new(col_w(c), row_h));
                        let resp = ui.interact(r, egui::Id::unique(("list_head", c.label())), Sense::click());
                        let arrow = if self.list_sort.0 == c { if self.list_sort.1 { "  ▲" } else { "  ▼" } } else { "" };
                        let color = if resp.hovered() { l.p.text_main } else { l.p.text_muted };
                        ui.painter().with_clip_rect(r).text(egui::pos2(r.left() + 8.0, r.center().y), egui::Align2::LEFT_CENTER, format!("{}{arrow}", tr(c.label())), crate::theme::bold(l.f.md * k), color);
                        if resp.clicked() {
                            sort_click = Some(c);
                        }
                    }
                    for (i, ((g, name, _, _), sheet)) in cards.iter().zip(&sheets).enumerate() {
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, row_h), Sense::click());
                        let p = ui.painter();
                        let focused = focus == Some(i);
                        if resp.hovered() || focused {
                            p.rect_filled(rect, 0.0, l.p.sel_bg);
                        } else if i % 2 == 1 {
                            p.rect_filled(rect, 0.0, l.p.bg_sidebar.gamma_multiply(0.5));
                        }
                        if focused && scroll {
                            resp.scroll_to_me(Some(egui::Align::Center));
                        }
                        for c in ListCol::ALL {
                            let r = egui::Rect::from_min_size(egui::pos2(rect.left() + cell_x(c), rect.top()), Vec2::new(col_w(c) - 8.0, row_h));
                            let color = if c == ListCol::Title { l.p.text_main } else { l.p.text_muted };
                            p.with_clip_rect(r).text(egui::pos2(r.left() + 8.0, r.center().y), egui::Align2::LEFT_CENTER, list_text(c, g, name, sheet.as_ref(), &self.usage), font.clone(), color);
                        }
                        if self.cur_game.as_ref() == Some(g) {
                            p.rect_stroke(rect, 0.0, Stroke::new(1.5, l.p.accent_hover), egui::StrokeKind::Inside);
                        }
                        if focused {
                            p.rect_stroke(rect, 0.0, Stroke::new(3.0, l.p.border_focus), egui::StrokeKind::Inside);
                        }
                        if resp.clicked() {
                            pick = Some(g.clone());
                        }
                        resp.context_menu(|ui| game_menu(ui, g, &mut pick, &mut info, &mut history));
                    }
                    return;
                }
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(gap_x, CARD_GAP);
                    for (i, (g, name, tex, pending)) in cards.iter().enumerate() {
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(card_w, card_h), Sense::click());
                        let p = ui.painter();
                        let cur = self.cur_game.as_ref() == Some(g);
                        let focused = focus == Some(i);
                        if resp.hovered() || focused {
                            p.rect_filled(rect, 0.0, l.p.sel_bg);
                        }
                        if focused && scroll {
                            resp.scroll_to_me(Some(egui::Align::Center));
                        }
                        let area = egui::Rect::from_min_size(rect.min + Vec2::new((card_w - img_w) / 2.0, 6.0), Vec2::new(img_w, img_h));
                        match tex {
                            Some(t) => {
                                // fit in the box without distorting (some covers are a bit taller)
                                let sz = t.size_vec2();
                                let k = (img_w / sz.x).min(img_h / sz.y);
                                let r = egui::Rect::from_center_size(area.center(), sz * k);
                                p.image(t.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                            }
                            None => {
                                // no cover: an empty "box" with the name
                                p.rect_filled(area, 0.0, l.p.bg_sidebar);
                                p.rect_stroke(area, 0.0, Stroke::new(1.0, l.p.border), egui::StrokeKind::Inside);
                                let msg = if *pending { tr("Downloading…") } else if art == Art::Boxart { tr("No cover") } else { tr("No image") };
                                p.text(area.center(), egui::Align2::CENTER_CENTER, msg, egui::FontId::proportional(l.f.sm), l.p.text_muted);
                            }
                        }
                        let label = egui::Rect::from_min_size(egui::pos2(rect.left() + 4.0, area.bottom() + 6.0), Vec2::new(card_w - 8.0, 26.0 * k));
                        p.with_clip_rect(label).text(label.center(), egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(l.f.sm * k), l.p.text_main);
                        if cur {
                            p.rect_stroke(rect, 0.0, Stroke::new(1.5, l.p.accent_hover), egui::StrokeKind::Inside);
                        }
                        if focused {
                            p.rect_stroke(rect, 0.0, Stroke::new(3.0, l.p.border_focus), egui::StrokeKind::Inside);
                        }
                        let resp = resp.on_hover_text(name.as_str());
                        if resp.clicked() {
                            pick = Some(g.clone());
                        }
                        resp.context_menu(|ui| game_menu(ui, g, &mut pick, &mut info, &mut history));
                    }
                });
            });
        });
        self.library_filter = filter;
        if let Some(a) = new_art {
            self.library_art = a;
        }
        if let Some(v) = new_list {
            self.library_list = v;
        }
        if let Some(c) = sort_click {
            self.list_sort = if self.list_sort.0 == c { (c, !self.list_sort.1) } else { (c, true) };
        }
        if bios {
            self.choose_bios_folder(None);
        }
        if folder
            && let Some(d) = rfd::FileDialog::new().set_title(tr("Change folder…")).set_directory(&self.rom_dir).pick_folder() {
                self.set_rom_dir(d);
            }
        if let Some(g) = pick {
            self.load_game(&g);
            self.nav_game_opened();
        } else if let Some(g) = info {
            self.open_info(&g);
        } else if let Some(g) = history {
            self.dialog = Dialog::History(Some(g));
        }
    }

    /// The library with no games in the folder (first run): choose the folder, where to get
    /// free games, and the controller basics. Returns whether "Choose the
    /// ROM folder…" was clicked.
    fn welcome(&self, ui: &mut egui::Ui) -> bool {
        let l = self.look;
        let brand = crate::brand::get();
        let mut choose = false;
        ui.add_space(30.0);
        ui.vertical_centered(|ui| {
            ui.set_max_width(560.0);
            ui.label(RichText::new(trf("Welcome to {name}", &[("name", &brand.name)])).font(crate::theme::bold(l.f.xxl)).color(l.p.text_main));
            ui.add_space(6.0);
            l.hint(ui, &trf("No games in {folder}.", &[("folder", &self.rom_dir.display())]));
            ui.add_space(14.0);
            choose = l.primary(ui, &tr("Choose the ROM folder…")).clicked();
        });
        if !brand.free_games.is_empty() {
            ui.add_space(22.0);
            ui.vertical_centered(|ui| {
                ui.set_max_width(560.0);
                l.card(ui, Some(&tr("FREE GAMES TO START WITH")), |ui| {
                    for (label, url) in brand.free_games {
                        if ui.link(tr(label)).on_hover_text(*url).clicked() {
                            crate::platform::open_url(url);
                        }
                    }
                    l.hint(ui, &tr("Download, unzip if needed, and put them in the ROM folder."));
                });
            });
        }
        ui.add_space(14.0);
        ui.vertical_centered(|ui| {
            l.hint(ui, &tr("With a controller: A chooses the folder; Home or Select + Start (hold) opens the menu."));
        });
        choose
    }
}

/// What the list view shows in a cell.
fn list_text(col: ListCol, path: &Path, name: &str, sheet: Option<&crate::sheets::Sheet>, usage: &crate::usage::Usage) -> String {
    let s = |f: fn(&crate::sheets::Sheet) -> &Option<String>| sheet.and_then(|s| f(s).clone()).unwrap_or_default();
    let key = path.to_string_lossy();
    match col {
        ListCol::Title => name.to_string(),
        ListCol::File => path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        ListCol::Genre => s(|s| &s.genre),
        ListCol::Year => s(|s| &s.year),
        ListCol::Developer => s(|s| &s.developer),
        ListCol::Publisher => s(|s| &s.publisher),
        ListCol::Players => s(|s| &s.players),
        ListCol::PlayTime => crate::usage::play_time(usage.played(&key)),
        ListCol::LastPlayed => usage.last_played(&key).map(crate::usage::relative).unwrap_or_default(),
    }
}

/// How the list sorts by a column: empty cells last; times by their value (most played /
/// most recent first when ascending), the rest as text.
fn list_key(col: ListCol, path: &Path, name: &str, sheet: Option<&crate::sheets::Sheet>, usage: &crate::usage::Usage) -> (bool, i64, String) {
    let key = path.to_string_lossy();
    match col {
        ListCol::PlayTime => {
            let t = usage.played(&key);
            (t <= 0.0, -(t as i64), name.to_lowercase())
        }
        ListCol::LastPlayed => match usage.last_played(&key) {
            Some(t) => (false, -(t as i64), name.to_lowercase()),
            None => (true, 0, name.to_lowercase()),
        },
        _ => {
            let text = list_text(col, path, name, sheet, usage).to_lowercase();
            (text.is_empty(), 0, text)
        }
    }
}

/// Right-click menu of a game (cards and list): Open, Game info, History. The same room
/// around the items as the menu bar's titles.
fn game_menu(ui: &mut egui::Ui, game: &Path, pick: &mut Option<PathBuf>, info: &mut Option<PathBuf>, history: &mut Option<PathBuf>) {
    ui.spacing_mut().button_padding.x = 10.0;
    if ui.button(tr("Open")).clicked() {
        *pick = Some(game.to_path_buf());
        ui.close();
    }
    if ui.button(tr("Game info…")).clicked() {
        *info = Some(game.to_path_buf());
        ui.close();
    }
    if ui.button(tr("History…")).clicked() {
        *history = Some(game.to_path_buf());
        ui.close();
    }
}

/// Column widths for the list: each column as wide as it needs. Room left over is shared
/// in proportion; when there isn't enough, the short columns keep their width and only the
/// long ones shrink, all to the same limit (their text is clipped).
fn fit_columns(need: &[f32], total: f32) -> Vec<f32> {
    let sum: f32 = need.iter().sum();
    if sum <= total {
        return need.iter().map(|n| n * total / sum.max(1.0)).collect();
    }
    // the largest limit L with sum(min(need, L)) <= total
    let mut sorted = need.to_vec();
    sorted.sort_by(f32::total_cmp);
    let (mut used, mut left) = (0.0, sorted.len());
    let mut limit = total / left.max(1) as f32;
    for n in sorted {
        if n > limit {
            break;
        }
        used += n;
        left -= 1;
        limit = (total - used) / left.max(1) as f32;
    }
    need.iter().map(|&n| n.min(limit)).collect()
}

#[cfg(test)]
mod tests {
    use super::fit_columns;

    #[test]
    fn columns_fit_the_width() {
        // room to spare: proportional, filling the width
        let w = fit_columns(&[100.0, 50.0], 300.0);
        assert_eq!(w, vec![200.0, 100.0]);
        // not enough: the short column keeps its width, the long ones share the rest
        let w = fit_columns(&[300.0, 50.0, 400.0], 450.0);
        assert_eq!(w, vec![200.0, 50.0, 200.0]);
        assert!((w.iter().sum::<f32>() - 450.0).abs() < 0.01);
    }
}
