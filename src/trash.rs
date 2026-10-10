//! "Recently deleted": deleted entries wait here for 30 days and can be restored.

use super::*;

/// How long a deleted entry is kept before it is removed for good.
pub(crate) const TRASH_DAYS: i64 = 30;

/// An entry in the bin, with when it was deleted (YYYY-MM-DD HH:MM:SS).
#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct Trashed {
    pub entry: Entry,
    pub deleted_at: String,
}

impl Trashed {
    /// Whole days left before this is removed for good (0 means today).
    pub fn days_left(&self, today: NaiveDate) -> i64 {
        let deleted = self.deleted_at.get(..10).and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()).unwrap_or(today);
        (TRASH_DAYS - (today - deleted).num_days()).max(0)
    }
}

impl DiaryApp {
    /// Move an entry to the bin instead of destroying it.
    pub(crate) fn trash_entry(&mut self, id: u64) {
        let Some(pos) = self.data.entries.iter().position(|e| e.id == id) else { return };
        let entry = self.data.entries.remove(pos);
        self.data.trash.push(Trashed { entry, deleted_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string() });
        self.last_trashed = Some(id);
        self.revealed.remove(&id);
        self.persist();
    }

    /// Put a deleted entry back. It gets a new id if its old one has been used since.
    pub(crate) fn restore_trashed(&mut self, id: u64) {
        let Some(pos) = self.data.trash.iter().position(|t| t.entry.id == id) else { return };
        let mut entry = self.data.trash.remove(pos).entry;
        if self.data.entries.iter().any(|e| e.id == entry.id) {
            entry.id = self.data.entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
        }
        if let Ok(d) = NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d") {
            self.selected = d;
            self.month = d.with_day(1).unwrap();
        }
        self.data.entries.push(entry);
        if self.last_trashed == Some(id) {
            self.last_trashed = None;
        }
        self.persist();
    }

    /// Remove for good whatever has been in the bin for more than 30 days.
    pub(crate) fn purge_trash(&mut self) {
        let today = Local::now().date_naive();
        self.data.trash.retain(|t| t.days_left(today) > 0);
    }

    pub(crate) fn trash_window(&mut self, ctx: &egui::Context) {
        if !self.show_trash {
            return;
        }
        let p = palette(ctx);
        let dark = ctx.theme() == egui::Theme::Dark;
        let today = Local::now().date_naive();
        let mut close = false;
        let mut restore = None;
        let mut forever = None;
        let mut empty = false;
        let mut items: Vec<&Trashed> = self.data.trash.iter().collect();
        items.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at));
        let count = items.len();

        egui::Window::new("Recently deleted")
            .title_bar(false)
            .frame(popup_frame(&p, dark))
            .resizable(true)
            .default_size([520.0, 520.0])
            .min_size([360.0, 240.0])
            .pivot(Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                close |= popup_header(ui, &p, "Recently deleted");
                egui::Frame::new().inner_margin(14).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("Deleted entries are kept for {TRASH_DAYS} days, then removed for good.")).color(p.muted),
                        );
                    });
                    ui.add_space(6.0);
                    if count == 0 {
                        ui.label(RichText::new("Nothing here. Entries you delete will wait here for a while.").color(p.ink));
                    } else {
                        let label = if self.trash_arm { "Click again to delete everything for good" } else { "Empty the bin" };
                        let b = egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(RED);
                        if ui.add(b).clicked() {
                            if self.trash_arm {
                                empty = true;
                            }
                            self.trash_arm = !self.trash_arm;
                        }
                    }
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        for t in &items {
                            let e = &t.entry;
                            let c = category_color(&self.data.categories, &e.category);
                            let fg = text_on(c);
                            egui::Frame::new().fill(c).corner_radius(10).inner_margin(10).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&e.date).strong().color(fg));
                                    ui.label(RichText::new(e.added_at.get(11..16).unwrap_or("")).small().color(fg));
                                    ui.label(RichText::new(&e.category).small().color(fg));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let left = t.days_left(today);
                                        ui.label(RichText::new(format!("{left} {} left", if left == 1 { "day" } else { "days" })).small().color(fg));
                                    });
                                });
                                let mut s = entry_label(e);
                                if s.chars().count() > 110 {
                                    s = s.chars().take(110).collect::<String>() + "…";
                                }
                                ui.label(RichText::new(s).color(fg));
                                ui.horizontal(|ui| {
                                    if ui.button("Restore").clicked() {
                                        restore = Some(e.id);
                                    }
                                    if ui.add(egui::Button::new(RichText::new("Delete for good").color(Color32::WHITE)).fill(RED)).clicked() {
                                        forever = Some(e.id);
                                    }
                                });
                            });
                            ui.add_space(6.0);
                        }
                    });
                });
            });

        if let Some(id) = restore {
            self.restore_trashed(id);
        }
        if let Some(id) = forever {
            self.data.trash.retain(|t| t.entry.id != id);
            self.persist();
        }
        if empty {
            self.data.trash.clear();
            self.trash_arm = false;
            self.last_trashed = None;
            self.persist();
        }
        if close {
            self.show_trash = false;
            self.trash_arm = false;
        }
    }
}
