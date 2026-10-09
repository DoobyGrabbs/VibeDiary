//! The "Pinned" window: every pinned entry in one list.

use super::*;

impl DiaryApp {
    pub(crate) fn pinned_window(&mut self, ctx: &egui::Context) {
        if !self.show_pinned {
            return;
        }
        let p = palette(ctx);
        let dark = ctx.theme() == egui::Theme::Dark;
        let mut close = false;
        let mut jump = None;
        let mut unpin = None;
        let mut pinned: Vec<&Entry> = self.data.entries.iter().filter(|e| e.pinned).collect();
        pinned.sort_by(|a, b| b.added_at.cmp(&a.added_at));

        egui::Window::new("Pinned entries")
            .title_bar(false)
            .frame(popup_frame(&p, dark))
            .resizable(true)
            .default_size([460.0, 480.0])
            .min_size([320.0, 200.0])
            .pivot(Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                close |= popup_header(ui, &p, "Pinned entries");
                egui::Frame::new().inner_margin(14).show(ui, |ui| {
                    if pinned.is_empty() {
                        ui.label(RichText::new("Nothing is pinned yet. Click the star on an entry to pin it.").color(p.muted));
                    } else {
                        ui.label(RichText::new("Click an entry to open its day.").color(p.muted));
                    }
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        for e in &pinned {
                            let c = category_color(&self.data.categories, &e.category);
                            let fg = text_on(c);
                            let card = egui::Frame::new().fill(c).corner_radius(10).inner_margin(10).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    let (r, star) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                                    paint_star(ui.painter(), r.center(), 9.0, fg, true);
                                    if star.on_hover_text("Unpin").clicked() {
                                        unpin = Some(e.id);
                                    }
                                    ui.label(RichText::new(&e.date).strong().color(fg));
                                    ui.label(RichText::new(e.added_at.get(11..16).unwrap_or("")).small().color(fg));
                                    ui.label(RichText::new(&e.category).small().color(fg));
                                    if let Some(m) = e.mood.filter(|_| !e.sensitive) {
                                        mood_badge(ui, m);
                                    }
                                });
                                let mut s = entry_label(e);
                                if s.chars().count() > 120 {
                                    s = s.chars().take(120).collect::<String>() + "…";
                                }
                                ui.label(RichText::new(s).color(fg));
                            });
                            let resp = ui.interact(card.response.rect, ui.id().with(("pinned", e.id)), Sense::click());
                            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                jump = NaiveDate::parse_from_str(&e.date, "%Y-%m-%d").ok();
                            }
                            ui.add_space(6.0);
                        }
                    });
                });
            });

        if let Some(id) = unpin {
            if let Some(e) = self.data.entries.iter_mut().find(|e| e.id == id) {
                e.pinned = false;
            }
            self.persist();
        }
        if let Some(d) = jump {
            self.selected = d;
            self.month = d.with_day(1).unwrap();
            self.day_scroll_pending = true;
        }
        if close {
            self.show_pinned = false;
        }
    }
}
