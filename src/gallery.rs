//! The Photos view: every picture in the diary, newest first, plus a full-size viewer.

use super::*;

/// One picture and where it came from.
pub(crate) struct Photo {
    pub id: String,
    pub entry_id: u64,
    pub date: String,
    pub time: String,
    pub caption: String,
    /// Pictures in private entries stay hidden until the entry is revealed.
    pub private: bool,
}

/// All pictures used by `entries`, newest entry first, skipping ones missing from `images`.
pub(crate) fn collect_photos(entries: &[Entry], images: &BTreeMap<String, String>) -> Vec<Photo> {
    let mut sorted: Vec<&Entry> = entries.iter().collect();
    sorted.sort_by(|a, b| b.added_at.cmp(&a.added_at));
    let mut photos = Vec::new();
    for e in sorted {
        for (id, caption) in markdown::images(&e.text) {
            if images.contains_key(&id) {
                photos.push(Photo {
                    id,
                    entry_id: e.id,
                    date: e.date.clone(),
                    time: e.added_at.get(11..16).unwrap_or("").to_string(),
                    caption,
                    private: e.sensitive,
                });
            }
        }
    }
    photos
}

impl DiaryApp {
    fn ensure_photos(&mut self) {
        if self.photos_cache.as_ref().map(|c| c.0) != Some(self.rev) {
            self.photos_cache = Some((self.rev, collect_photos(&self.data.entries, &self.data.images)));
        }
    }

    /// The Photos view: thumbnails grouped by month.
    pub(crate) fn gallery_view(&mut self, ui: &mut egui::Ui) {
        self.ensure_photos();
        let p = palette(ui.ctx());
        let ctx = ui.ctx().clone();
        let photos = &self.photos_cache.as_ref().unwrap().1;
        let mut open = None;
        let mut reveal = None;

        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} {}", photos.len(), if photos.len() == 1 { "picture" } else { "pictures" })).color(p.muted));
            ui.add_space(12.0);
            ui.label("Size");
            ui.add(egui::Slider::new(&mut self.thumb_size, 80.0..=260.0).show_value(false));
        });
        ui.add_space(6.0);
        if photos.is_empty() {
            ui.label(RichText::new("No pictures yet. Add one with the Image button in the editor.").color(p.muted));
            return;
        }

        // Group by month (the list is already newest first).
        let mut months: Vec<(String, Vec<usize>)> = Vec::new();
        for (i, ph) in photos.iter().enumerate() {
            let label = NaiveDate::parse_from_str(&ph.date, "%Y-%m-%d").map(|d| d.format("%B %Y").to_string()).unwrap_or_default();
            match months.last_mut() {
                Some((m, list)) if *m == label => list.push(i),
                _ => months.push((label, vec![i])),
            }
        }

        let tile = self.thumb_size;
        egui::ScrollArea::vertical().id_salt("photo_scroll").auto_shrink([false, false]).show(ui, |ui| {
            for (label, indexes) in &months {
                ui.label(RichText::new(label).size(20.0).strong().color(p.title));
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                    for &i in indexes {
                        let ph = &photos[i];
                        let hidden = ph.private && !self.revealed.contains(&ph.entry_id);
                        let (rect, resp) = ui.allocate_exact_size(vec2(tile, tile), Sense::click());
                        ui.painter().rect_filled(rect, 8.0, p.head_bg);
                        if hidden {
                            ui.painter().text(rect.center(), Align2::CENTER_CENTER, "Private\nclick to show", FontId::proportional(14.0), p.head_text);
                        } else if ui.is_rect_visible(rect) {
                            // Only pictures on screen are decoded.
                            if let Some(tex) = self.media.thumbnail(&ctx, &self.data.images, &ph.id) {
                                let natural = tex.size_vec2();
                                let scale = (tile - 8.0) / natural.x.max(natural.y);
                                let img = egui::Rect::from_center_size(rect.center(), natural * scale);
                                ui.painter().image(tex.id(), img, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                            }
                        }
                        if resp.hovered() {
                            ui.painter().rect_stroke(rect, 8.0, Stroke::new(2.5, p.accent), egui::StrokeKind::Inside);
                        }
                        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                        let resp = if hidden { resp } else { resp.on_hover_text(format!("{} {}\n{}", ph.date, ph.time, ph.caption)) };
                        if resp.clicked() {
                            if hidden {
                                reveal = Some(ph.entry_id);
                            } else {
                                open = Some(i);
                            }
                        }
                    }
                });
                ui.add_space(10.0);
            }
        });
        if let Some(id) = reveal {
            self.revealed.insert(id);
        }
        if let Some(i) = open {
            self.lightbox = Some(i);
        }
    }

    /// A picture shown large, with previous / next and a way to jump to its entry.
    pub(crate) fn lightbox_window(&mut self, ctx: &egui::Context) {
        let Some(mut i) = self.lightbox else { return };
        self.ensure_photos();
        let count = self.photos_cache.as_ref().map_or(0, |c| c.1.len());
        if count == 0 || i >= count {
            self.lightbox = None;
            return;
        }
        let p = palette(ctx);
        let dark = ctx.theme() == egui::Theme::Dark;
        let mut close = false;
        let mut goto = None;

        // Arrow keys and Esc work while the viewer is open.
        let (left, right, esc) = ctx.input_mut(|inp| {
            (
                inp.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft),
                inp.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight),
                inp.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
            )
        });
        if left && i > 0 {
            i -= 1;
        }
        if right && i + 1 < count {
            i += 1;
        }
        close |= esc;

        let photo = &self.photos_cache.as_ref().unwrap().1[i];
        let hidden = photo.private && !self.revealed.contains(&photo.entry_id);
        let title = format!("{} {}", photo.date, photo.time);
        let (id, caption, entry_date) = (photo.id.clone(), photo.caption.clone(), photo.date.clone());
        let tex = if hidden { None } else { self.media.texture(ctx, &self.data.images, &id) };
        let mut step = 0i32;

        egui::Window::new("Photo")
            .title_bar(false)
            .frame(popup_frame(&p, dark))
            .resizable(true)
            .default_size([860.0, 660.0])
            .min_size([360.0, 300.0])
            .pivot(Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                close |= popup_header(ui, &p, &title);
                egui::Frame::new().inner_margin(12).show(ui, |ui| {
                    let room = vec2(ui.available_width(), (ui.available_height() - 74.0).max(120.0));
                    ui.allocate_ui_with_layout(room, egui::Layout::top_down(egui::Align::Center), |ui| {
                        match &tex {
                            Some(t) => {
                                let natural = t.size_vec2();
                                let scale = (room.x / natural.x).min(room.y / natural.y).min(1.0);
                                ui.image((t.id(), natural * scale));
                            }
                            None => {
                                ui.add_space(room.y / 2.0 - 10.0);
                                ui.label(RichText::new("This picture is in a private entry.").color(p.muted));
                            }
                        }
                    });
                    if !caption.is_empty() && !hidden {
                        ui.label(RichText::new(&caption).color(p.ink));
                    }
                    ui.horizontal(|ui| {
                        if ui.add_enabled(i > 0, egui::Button::new("Previous")).clicked() {
                            step = -1;
                        }
                        ui.label(RichText::new(format!("{} of {count}", i + 1)).color(p.muted));
                        if ui.add_enabled(i + 1 < count, egui::Button::new("Next")).clicked() {
                            step = 1;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Go to this entry").clicked() {
                                goto = NaiveDate::parse_from_str(&entry_date, "%Y-%m-%d").ok();
                            }
                        });
                    });
                });
            });

        i = (i as i32 + step).clamp(0, count as i32 - 1) as usize;
        self.lightbox = if close { None } else { Some(i) };
        if let Some(d) = goto {
            self.selected = d;
            self.month = d.with_day(1).unwrap();
            self.view = CalView::Month;
            self.lightbox = None;
        }
    }
}
