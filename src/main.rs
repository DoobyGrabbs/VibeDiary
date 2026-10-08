use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{Datelike, Local, NaiveDate};
use eframe::egui::{self, Align2, Color32, FontId, RichText, Sense, Stroke, vec2};
use serde::{Deserialize, Serialize};

const BLUE: Color32 = Color32::from_rgb(37, 99, 235);
const BLUE_HOVER: Color32 = Color32::from_rgb(29, 78, 216);
const RED: Color32 = Color32::from_rgb(220, 38, 38);
const GREY: [u8; 3] = [156, 163, 175];

/// Colours that differ between light and dark mode.
struct Palette {
    page_bg: Color32,
    panel_bg: Color32,
    header: Color32,
    accent: Color32,
    cell: Color32,
    cell_border: Color32,
    weekend: Color32,
    today: Color32,
    hover: Color32,
    head_bg: Color32,
    head_text: Color32,
    day_num: Color32,
    muted: Color32,
    title: Color32,
    ink: Color32,
}

fn palette(ctx: &egui::Context) -> Palette {
    let c = |r, g, b| Color32::from_rgb(r, g, b);
    if ctx.theme() == egui::Theme::Dark {
        Palette {
            page_bg: c(15, 23, 42),
            panel_bg: c(30, 41, 59),
            header: c(30, 58, 138),
            accent: c(96, 165, 250),
            cell: c(30, 41, 59),
            cell_border: c(71, 85, 105),
            weekend: c(22, 33, 62),
            today: c(20, 83, 45),
            hover: c(66, 56, 20),
            head_bg: c(30, 58, 138),
            head_text: c(191, 219, 254),
            day_num: c(226, 232, 240),
            muted: c(148, 163, 184),
            title: c(147, 197, 253),
            ink: c(241, 245, 249),
        }
    } else {
        Palette {
            page_bg: c(240, 247, 255),
            panel_bg: Color32::WHITE,
            header: BLUE,
            accent: BLUE,
            cell: Color32::WHITE,
            cell_border: c(203, 213, 225),
            weekend: c(224, 242, 254),
            today: c(220, 252, 231),
            hover: c(254, 249, 195),
            head_bg: c(219, 234, 254),
            head_text: c(30, 64, 175),
            day_num: c(51, 65, 85),
            muted: c(71, 85, 105),
            title: c(30, 64, 175),
            ink: c(30, 41, 59),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct Category {
    name: String,
    color: [u8; 3],
}

#[derive(Serialize, Deserialize, Clone)]
struct Entry {
    id: u64,
    /// The diary day the entry belongs to, YYYY-MM-DD.
    date: String,
    /// When the entry was first written, YYYY-MM-DD HH:MM:SS (local time).
    added_at: String,
    category: String,
    text: String,
}

#[derive(Serialize, Deserialize)]
struct Data {
    categories: Vec<Category>,
    entries: Vec<Entry>,
}

fn default_categories() -> Vec<Category> {
    [
        ("Personal", [96, 165, 250]),
        ("Work", [251, 191, 36]),
        ("Health", [74, 222, 128]),
        ("Ideas", [192, 132, 252]),
        ("Important", [248, 113, 113]),
    ]
    .into_iter()
    .map(|(n, c)| Category { name: n.into(), color: c })
    .collect()
}

fn data_path() -> PathBuf {
    // Project folder, fixed at compile time so it doesn't depend on the working directory.
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("entries.json")
}

fn load() -> Data {
    let fresh = || Data { categories: default_categories(), entries: Vec::new() };
    let Ok(raw) = std::fs::read_to_string(data_path()) else { return fresh() };
    if let Ok(data) = serde_json::from_str::<Data>(&raw) {
        return data;
    }
    // Older format: one text per date, e.g. {"2026-10-08": "text"}.
    if let Ok(old) = serde_json::from_str::<BTreeMap<String, String>>(&raw) {
        let mut data = fresh();
        let category = data.categories[0].name.clone();
        data.entries = old
            .into_iter()
            .enumerate()
            .map(|(i, (date, text))| Entry {
                id: i as u64 + 1,
                added_at: format!("{date} 00:00:00"),
                date,
                category: category.clone(),
                text,
            })
            .collect();
        return data;
    }
    fresh()
}

fn save(data: &Data) -> Result<(), String> {
    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    std::fs::write(data_path(), json).map_err(|e| e.to_string())
}

fn key(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// Readable text colour for a given background.
fn text_on(bg: Color32) -> Color32 {
    let lum = 0.299 * bg.r() as f32 + 0.587 * bg.g() as f32 + 0.114 * bg.b() as f32;
    if lum > 150.0 { Color32::from_rgb(30, 41, 59) } else { Color32::WHITE }
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("")
}

struct EditState {
    /// `Some` when editing an existing entry.
    id: Option<u64>,
    date: NaiveDate,
    category: String,
    text: String,
}

struct DiaryApp {
    data: Data,
    selected: NaiveDate,
    /// First day of the month being displayed.
    month: NaiveDate,
    editor: Option<EditState>,
    show_categories: bool,
    new_cat_name: String,
    new_cat_color: [u8; 3],
    /// Category row being renamed and its in-progress text.
    rename_buf: Option<(usize, String)>,
    status: String,
}

impl DiaryApp {
    fn new() -> Self {
        let today = Local::now().date_naive();
        Self {
            data: load(),
            selected: today,
            month: today.with_day(1).unwrap(),
            editor: None,
            show_categories: false,
            new_cat_name: String::new(),
            new_cat_color: [59, 130, 246],
            rename_buf: None,
            status: String::new(),
        }
    }

    fn persist(&mut self) {
        self.status = match save(&self.data) {
            Ok(()) => "Saved".into(),
            Err(e) => format!("Save failed: {e}"),
        };
    }

    fn cat_color(&self, name: &str) -> Color32 {
        rgb(self.data.categories.iter().find(|c| c.name == name).map_or(GREY, |c| c.color))
    }

    fn day_entries(&self, d: NaiveDate) -> Vec<&Entry> {
        let k = key(d);
        let mut v: Vec<&Entry> = self.data.entries.iter().filter(|e| e.date == k).collect();
        v.sort_by(|a, b| a.added_at.cmp(&b.added_at));
        v
    }

    fn shift_month(&mut self, delta: i32) {
        let total = self.month.year() * 12 + self.month.month0() as i32 + delta;
        self.month = NaiveDate::from_ymd_opt(total.div_euclid(12), total.rem_euclid(12) as u32 + 1, 1).unwrap();
    }

    fn new_entry(&mut self, date: NaiveDate) {
        let category = self.data.categories.first().map(|c| c.name.clone()).unwrap_or_default();
        self.editor = Some(EditState { id: None, date, category, text: String::new() });
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.label(RichText::new("My Diary").size(24.0).strong().color(Color32::WHITE));
            ui.add_space(24.0);
            let white = |s: &str| RichText::new(s).size(18.0).strong().color(Color32::WHITE);
            if ui.button("◀").clicked() {
                self.shift_month(-1);
            }
            ui.label(white(&self.month.format("%B %Y").to_string()));
            if ui.button("▶").clicked() {
                self.shift_month(1);
            }
            if ui.button("Today").clicked() {
                let t = Local::now().date_naive();
                self.selected = t;
                self.month = t.with_day(1).unwrap();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("🎨 Categories").clicked() {
                    self.show_categories = true;
                }
                if ui.button("➕ New entry").clicked() {
                    self.new_entry(self.selected);
                }
            });
        });
    }

    fn calendar(&mut self, ui: &mut egui::Ui) {
        let first = self.month;
        let next = self.shift_target(first);
        let days = (next - first).num_days() as u32;
        let offset = first.weekday().num_days_from_monday();
        let rows = (offset + days).div_ceil(7);

        let head_h = 28.0;
        let avail = ui.available_size();
        let cell_w = avail.x / 7.0;
        let cell_h = ((avail.y - head_h) / rows as f32).max(70.0);
        let (grid, _) = ui.allocate_exact_size(vec2(avail.x, head_h + cell_h * rows as f32), Sense::hover());
        let painter = ui.painter_at(grid);
        let p = palette(ui.ctx());
        let today = Local::now().date_naive();

        for (i, name) in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].iter().enumerate() {
            let r = egui::Rect::from_min_size(grid.min + vec2(cell_w * i as f32, 0.0), vec2(cell_w, head_h));
            painter.rect_filled(r.shrink(1.0), 4.0, p.head_bg);
            painter.text(r.center(), Align2::CENTER_CENTER, *name, FontId::proportional(16.0), p.head_text);
        }

        let mut clicked = None;
        let mut double_clicked = None;
        for n in 0..days {
            let day = first + chrono::Days::new(n as u64);
            let slot = offset + n;
            let (col, row) = (slot % 7, slot / 7);
            let rect = egui::Rect::from_min_size(
                grid.min + vec2(cell_w * col as f32, head_h + cell_h * row as f32),
                vec2(cell_w, cell_h),
            )
            .shrink(1.5);

            let resp = ui.interact(rect, ui.id().with(("day", n)), Sense::click());
            let bg = if day == today {
                p.today
            } else if col >= 5 {
                p.weekend
            } else if resp.hovered() {
                p.hover
            } else {
                p.cell
            };
            painter.rect_filled(rect, 6.0, bg);
            let border = if day == self.selected {
                Stroke::new(2.5, p.accent)
            } else {
                Stroke::new(1.0, p.cell_border)
            };
            painter.rect_stroke(rect, 6.0, border, egui::StrokeKind::Inside);
            painter.text(
                rect.min + vec2(8.0, 6.0),
                Align2::LEFT_TOP,
                day.day().to_string(),
                FontId::proportional(19.0),
                p.day_num,
            );

            let entries = self.day_entries(day);
            let pill_h = 21.0;
            let top = rect.min.y + 30.0;
            let fit = (((rect.max.y - top - 4.0) / (pill_h + 2.0)).floor() as usize).max(1);
            let shown = if entries.len() > fit { fit - 1 } else { entries.len() };
            for (i, e) in entries.iter().take(shown).enumerate() {
                let c = self.cat_color(&e.category);
                let pr = egui::Rect::from_min_size(
                    egui::pos2(rect.min.x + 4.0, top + i as f32 * (pill_h + 2.0)),
                    vec2(rect.width() - 8.0, pill_h),
                );
                painter.rect_filled(pr, 9.0, c);
                painter.with_clip_rect(pr.shrink2(vec2(4.0, 0.0))).text(
                    egui::pos2(pr.min.x + 7.0, pr.center().y),
                    Align2::LEFT_CENTER,
                    format!("{} {}", &e.added_at[11..16.min(e.added_at.len())], first_line(&e.text)),
                    FontId::proportional(14.0),
                    text_on(c),
                );
            }
            if entries.len() > shown {
                painter.text(
                    egui::pos2(rect.min.x + 8.0, top + shown as f32 * (pill_h + 2.0) + pill_h / 2.0),
                    Align2::LEFT_CENTER,
                    format!("+{} more", entries.len() - shown),
                    FontId::proportional(14.0),
                    p.muted,
                );
            }

            if resp.clicked() {
                clicked = Some(day);
            }
            if resp.double_clicked() {
                double_clicked = Some(day);
            }
        }
        if let Some(d) = clicked {
            self.selected = d;
        }
        if let Some(d) = double_clicked {
            self.new_entry(d);
        }
    }

    fn shift_target(&self, first: NaiveDate) -> NaiveDate {
        let total = first.year() * 12 + first.month0() as i32 + 1;
        NaiveDate::from_ymd_opt(total.div_euclid(12), total.rem_euclid(12) as u32 + 1, 1).unwrap()
    }

    fn day_panel(&mut self, ui: &mut egui::Ui) {
        let p = palette(ui.ctx());
        ui.label(RichText::new(self.selected.format("%A").to_string()).size(14.0).color(p.muted));
        ui.label(RichText::new(self.selected.format("%e %B %Y").to_string()).size(22.0).strong().color(p.title));
        ui.add_space(6.0);
        if ui.button("➕ Add entry for this day").clicked() {
            self.new_entry(self.selected);
        }
        ui.add_space(8.0);
        ui.separator();

        let entries: Vec<Entry> = self.day_entries(self.selected).into_iter().cloned().collect();
        let mut edit = None;
        let mut delete = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            if entries.is_empty() {
                ui.add_space(8.0);
                ui.label("No entries for this day yet.");
            }
            for e in &entries {
                let c = self.cat_color(&e.category);
                let fg = text_on(c);
                egui::Frame::new()
                    .fill(c)
                    .corner_radius(10)
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&e.category).strong().color(fg));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let time = e.added_at.get(11..16).unwrap_or("");
                                ui.label(RichText::new(format!("Added {time}")).small().color(fg));
                            });
                        });
                        ui.add(egui::Label::new(RichText::new(&e.text).color(fg)).wrap());
                        ui.horizontal(|ui| {
                            if ui.button("Edit").clicked() {
                                edit = Some(e.id);
                            }
                            if ui.add(egui::Button::new(RichText::new("Delete").color(Color32::WHITE)).fill(RED)).clicked() {
                                delete = Some(e.id);
                            }
                        });
                    });
                ui.add_space(6.0);
            }
        });

        if let Some(id) = edit {
            if let Some(e) = self.data.entries.iter().find(|e| e.id == id) {
                self.editor = Some(EditState {
                    id: Some(id),
                    date: self.selected,
                    category: e.category.clone(),
                    text: e.text.clone(),
                });
            }
        }
        if let Some(id) = delete {
            self.data.entries.retain(|e| e.id != id);
            self.persist();
        }
    }

    fn entry_window(&mut self, ctx: &egui::Context) {
        let p = palette(ctx);
        let Some(ed) = &mut self.editor else { return };
        let cats = &self.data.categories;
        let mut open = true;
        let mut save_it = false;
        let mut cancel = false;

        egui::Window::new(if ed.id.is_some() { "Edit entry" } else { "New entry" })
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([480.0, 380.0])
            .min_size([320.0, 240.0])
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(RichText::new(ed.date.format("%A, %e %B %Y").to_string()).strong());
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Category:");
                    let sel_color = rgb(cats.iter().find(|c| c.name == ed.category).map_or(GREY, |c| c.color));
                    egui::ComboBox::from_id_salt("cat")
                        .selected_text(RichText::new(&ed.category).strong().color(text_on(sel_color)).background_color(sel_color))
                        .show_ui(ui, |ui| {
                            for c in cats {
                                let col = rgb(c.color);
                                ui.selectable_value(
                                    &mut ed.category,
                                    c.name.clone(),
                                    RichText::new(&c.name).strong().color(text_on(col)).background_color(col),
                                );
                            }
                        });
                });
                ui.add_space(6.0);
                // Fill the window, leaving room for the buttons underneath.
                let height = (ui.available_height() - 44.0).max(80.0);
                ui.add_sized(
                    [ui.available_width(), height],
                    egui::TextEdit::multiline(&mut ed.text)
                        .hint_text("What's on your mind?")
                        .text_color(p.ink),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let can_save = !ed.text.trim().is_empty();
                    if ui.add_enabled(can_save, egui::Button::new("Save")).clicked() {
                        save_it = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });

        if save_it {
            let ed = self.editor.take().unwrap();
            let text = ed.text.trim().to_string();
            match ed.id {
                Some(id) => {
                    if let Some(e) = self.data.entries.iter_mut().find(|e| e.id == id) {
                        e.text = text;
                        e.category = ed.category;
                    }
                }
                None => {
                    let id = self.data.entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
                    self.data.entries.push(Entry {
                        id,
                        date: key(ed.date),
                        added_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                        category: ed.category,
                        text,
                    });
                }
            }
            self.selected = ed.date;
            self.persist();
        } else if cancel || !open {
            self.editor = None;
        }
    }

    fn categories_window(&mut self, ctx: &egui::Context) {
        let p = palette(ctx);
        if !self.show_categories {
            return;
        }
        let mut open = true;
        let mut changed = false;
        let mut remove = None;
        let mut renames: Vec<(String, String)> = Vec::new();

        egui::Window::new("Categories")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([520.0, 420.0])
            .min_size([360.0, 260.0])
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let can_delete = self.data.categories.len() > 1;
                let names: Vec<String> = self.data.categories.iter().map(|c| c.name.clone()).collect();
                let big = FontId::proportional(22.0);
                let field_margin = egui::Margin::symmetric(10, 8);
                for (i, c) in self.data.categories.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        changed |= ui.color_edit_button_srgb(&mut c.color).changed();

                        // Delete sits at the right edge; the name field takes the space in between.
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let del = egui::Button::new(RichText::new("Delete").color(Color32::WHITE)).fill(RED);
                            if ui.add_enabled(can_delete, del).clicked() {
                                remove = Some(i);
                            }

                            // Edit a buffered copy and only apply the rename when editing finishes.
                            let mut text = match &self.rename_buf {
                                Some((j, b)) if *j == i => b.clone(),
                                _ => c.name.clone(),
                            };
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut text)
                                    .font(big.clone())
                                    .margin(field_margin)
                                    .text_color(p.ink)
                                    .desired_width(ui.available_width()),
                            );
                            if resp.changed() {
                                self.rename_buf = Some((i, text));
                            } else if resp.lost_focus() {
                                if let Some((j, new)) = self.rename_buf.take() {
                                    let new = new.trim().to_string();
                                    let clash = names.iter().enumerate().any(|(k, n)| k != j && n.eq_ignore_ascii_case(&new));
                                    if j == i && !new.is_empty() && !clash && new != c.name {
                                        renames.push((std::mem::replace(&mut c.name, new.clone()), new));
                                    }
                                }
                            }
                        });
                    });
                    ui.add_space(4.0);
                }
                ui.weak("Click a name to rename it; press Enter to apply.");
                ui.separator();
                ui.label("Add a category");
                ui.horizontal(|ui| {
                    ui.color_edit_button_srgb(&mut self.new_cat_color);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let name = self.new_cat_name.trim().to_string();
                        let dup = self.data.categories.iter().any(|c| c.name.eq_ignore_ascii_case(&name));
                        if ui.add_enabled(!name.is_empty() && !dup, egui::Button::new("Add")).clicked() {
                            self.data.categories.push(Category { name, color: self.new_cat_color });
                            self.new_cat_name.clear();
                            changed = true;
                        }
                        ui.add(
                            egui::TextEdit::singleline(&mut self.new_cat_name)
                                .font(big.clone())
                                .margin(field_margin)
                                .hint_text("Name")
                                .text_color(p.ink)
                                .desired_width(ui.available_width()),
                        );
                    });
                });
                ui.weak("Entries in a deleted category show in grey.");
            });

        for (old, new) in renames {
            for e in self.data.entries.iter_mut().filter(|e| e.category == old) {
                e.category = new.clone();
            }
            if let Some(ed) = self.editor.as_mut().filter(|ed| ed.category == old) {
                ed.category = new;
            }
            changed = true;
        }
        if let Some(i) = remove {
            self.data.categories.remove(i);
            changed = true;
        }
        if changed {
            self.persist();
        }
        if !open {
            self.show_categories = false;
        }
    }
}

fn make_visuals(dark: bool) -> egui::Visuals {
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    let (page, window, stroke) = if dark {
        (Color32::from_rgb(15, 23, 42), Color32::from_rgb(30, 41, 59), Color32::from_rgb(148, 163, 184))
    } else {
        (Color32::from_rgb(240, 247, 255), Color32::WHITE, Color32::from_rgb(100, 116, 139))
    };
    v.panel_fill = page;
    v.window_fill = window;
    v.window_corner_radius = 0.into();
    v.window_stroke = Stroke::new(1.0, stroke);
    v.window_shadow = egui::Shadow { offset: [0, 10], blur: 32, spread: 4, color: Color32::from_black_alpha(if dark { 220 } else { 150 }) };
    v.extreme_bg_color = if dark { Color32::from_rgb(15, 23, 42) } else { Color32::WHITE };
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
        w.corner_radius = 8.into();
        w.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    }
    v.widgets.inactive.weak_bg_fill = BLUE;
    v.widgets.inactive.bg_fill = BLUE;
    v.widgets.hovered.weak_bg_fill = BLUE_HOVER;
    v.widgets.hovered.bg_fill = BLUE_HOVER;
    v.widgets.active.weak_bg_fill = BLUE_HOVER;
    v.widgets.active.bg_fill = BLUE_HOVER;
    v.selection.bg_fill = if dark { Color32::from_rgb(30, 64, 175) } else { Color32::from_rgb(147, 197, 253) };
    v
}

fn setup_style(ctx: &egui::Context) {
    // Follow the system light/dark setting.
    ctx.options_mut(|o| o.theme_preference = egui::ThemePreference::System);
    ctx.set_visuals_of(egui::Theme::Light, make_visuals(false));
    ctx.set_visuals_of(egui::Theme::Dark, make_visuals(true));
    ctx.global_style_mut(|s| {
        s.spacing.button_padding = vec2(12.0, 6.0);
        for font in s.text_styles.values_mut() {
            font.size *= 1.25;
        }
    });
}

impl eframe::App for DiaryApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let p = palette(&ctx);
        egui::Panel::top("header")
            .frame(egui::Frame::new().fill(p.header).inner_margin(egui::Margin::symmetric(14, 10)))
            .show(ui, |ui| self.header(ui));
        egui::Panel::right("day")
            .default_size(330.0)
            .frame(egui::Frame::new().fill(p.panel_bg).inner_margin(14))
            .show(ui, |ui| self.day_panel(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.page_bg).inner_margin(10))
            .show(ui, |ui| self.calendar(ui));

        self.entry_window(&ctx);
        self.categories_window(&ctx);
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1150.0, 720.0]).with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Diary",
        options,
        Box::new(|cc| {
            setup_style(&cc.egui_ctx);
            Ok(Box::new(DiaryApp::new()))
        }),
    )
}
