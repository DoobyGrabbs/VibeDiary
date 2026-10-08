use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{Datelike, Local, NaiveDate};
use eframe::egui;

/// Entries keyed by ISO date (YYYY-MM-DD), one entry per day.
type Entries = BTreeMap<String, String>;

fn data_path() -> Option<PathBuf> {
    // %APPDATA% on Windows (Roaming)
    dirs::config_dir().map(|d| d.join("Diary").join("entries.json"))
}

fn load() -> Entries {
    data_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(entries: &Entries) -> Result<(), String> {
    let path = data_path().ok_or("no AppData folder")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(entries).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

fn key(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

struct DiaryApp {
    entries: Entries,
    selected: NaiveDate,
    /// First day of the month being displayed.
    month: NaiveDate,
    draft: String,
    editing: bool,
    status: String,
}

impl DiaryApp {
    fn new() -> Self {
        let today = Local::now().date_naive();
        let mut app = Self {
            entries: load(),
            selected: today,
            month: today.with_day(1).unwrap(),
            draft: String::new(),
            editing: false,
            status: String::new(),
        };
        app.select(today);
        app
    }

    fn select(&mut self, d: NaiveDate) {
        self.selected = d;
        self.draft = self.entries.get(&key(d)).cloned().unwrap_or_default();
        self.editing = !self.entries.contains_key(&key(d));
    }

    fn persist(&mut self) {
        self.status = match save(&self.entries) {
            Ok(()) => "Saved".into(),
            Err(e) => format!("Save failed: {e}"),
        };
    }

    fn shift_month(&mut self, delta: i32) {
        let total = self.month.year() * 12 + self.month.month0() as i32 + delta;
        self.month = NaiveDate::from_ymd_opt(total.div_euclid(12), total.rem_euclid(12) as u32 + 1, 1).unwrap();
    }

    fn calendar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("<").clicked() {
                self.shift_month(-1);
            }
            ui.strong(self.month.format("%B %Y").to_string());
            if ui.button(">").clicked() {
                self.shift_month(1);
            }
        });
        ui.add_space(6.0);

        let today = Local::now().date_naive();
        let mut clicked = None;
        egui::Grid::new("cal").spacing([4.0, 4.0]).show(ui, |ui| {
            for d in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"] {
                ui.label(d);
            }
            ui.end_row();

            let offset = self.month.weekday().num_days_from_monday() as usize;
            let mut col = 0;
            for _ in 0..offset {
                ui.label("");
                col += 1;
            }
            let mut day = self.month;
            while day.month() == self.month.month() {
                let flag = self.entries.contains_key(&key(day));
                let text = if flag { format!("{}*", day.day()) } else { day.day().to_string() };
                let mut btn = egui::Button::new(text).min_size(egui::vec2(34.0, 28.0));
                if day == self.selected {
                    btn = btn.selected(true);
                } else if flag {
                    btn = btn.fill(egui::Color32::from_rgb(40, 110, 70));
                }
                let mut resp = ui.add(btn);
                if day == today {
                    resp = resp.on_hover_text("Today");
                }
                if resp.clicked() {
                    clicked = Some(day);
                }
                col += 1;
                if col % 7 == 0 {
                    ui.end_row();
                }
                day = day.succ_opt().unwrap();
            }
        });
        ui.add_space(6.0);
        ui.label("* = day has an entry");
        if let Some(d) = clicked {
            self.select(d);
        }
    }

    fn editor(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.selected.format("%A, %e %B %Y").to_string());
        ui.separator();
        let exists = self.entries.contains_key(&key(self.selected));

        if self.editing {
            ui.add_sized(
                [ui.available_width(), ui.available_height() - 40.0],
                egui::TextEdit::multiline(&mut self.draft).hint_text("What's on your mind today?"),
            );
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    let text = self.draft.trim().to_string();
                    if text.is_empty() {
                        self.entries.remove(&key(self.selected));
                    } else {
                        self.draft = text.clone();
                        self.entries.insert(key(self.selected), text);
                    }
                    self.persist();
                    self.editing = false;
                }
                if exists && ui.button("Cancel").clicked() {
                    self.select(self.selected);
                }
            });
        } else {
            egui::ScrollArea::vertical()
                .max_height(ui.available_height() - 40.0)
                .show(ui, |ui| {
                    ui.add(egui::Label::new(&self.draft).wrap());
                });
            ui.horizontal(|ui| {
                if ui.button("Edit").clicked() {
                    self.editing = true;
                }
                if ui.button("Delete").clicked() {
                    self.entries.remove(&key(self.selected));
                    self.persist();
                    self.select(self.selected);
                }
            });
        }
        if !self.status.is_empty() {
            ui.weak(&self.status);
        }
    }
}

impl eframe::App for DiaryApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::left("calendar").resizable(false).show(ui, |ui| self.calendar(ui));
        egui::CentralPanel::default().show(ui, |ui| self.editor(ui));
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([760.0, 460.0]),
        ..Default::default()
    };
    eframe::run_native("Diary", options, Box::new(|_cc| Ok(Box::new(DiaryApp::new()))))
}
