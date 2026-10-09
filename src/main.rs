use std::collections::BTreeMap;

use chrono::{Datelike, Local, NaiveDate, Timelike};
use eframe::egui::{self, Align2, Color32, FontId, RichText, Sense, Stroke, vec2};
use serde::{Deserialize, Serialize};

use location::data_path;

mod icon;
mod export;
mod live;
mod location;
mod markdown;
mod stats;
mod vault;

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
    /// How the day felt, 1 (awful) to 5 (great).
    #[serde(default)]
    mood: Option<u8>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Data {
    categories: Vec<Category>,
    entries: Vec<Entry>,
    /// Images used by entries: id -> base64 of the (re-encoded) image file.
    #[serde(default)]
    images: BTreeMap<String, String>,
    #[serde(default)]
    settings: Settings,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
struct Settings {
    /// Lock after this many idle minutes; 0 turns auto-lock off.
    auto_lock_minutes: u32,
    /// Daily writing goal in words; 0 means no goal.
    daily_word_goal: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { auto_lock_minutes: 10, daily_word_goal: 0 }
    }
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


fn fresh_data() -> Data {
    Data { categories: default_categories(), entries: Vec::new(), images: BTreeMap::new(), settings: Settings::default() }
}

/// What is currently on disk.
enum Disk {
    Missing,
    Encrypted(vault::Envelope),
    /// Unencrypted data written by an earlier version of the app.
    Plain(Data),
    Unreadable(String),
}

fn inspect() -> Disk {
    let raw = match std::fs::read_to_string(data_path()) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Disk::Missing,
        Err(e) => return Disk::Unreadable(e.to_string()),
    };
    if raw.trim().is_empty() {
        return Disk::Missing;
    }
    if let Some(env) = vault::Envelope::parse(&raw) {
        return Disk::Encrypted(env);
    }
    if let Ok(data) = serde_json::from_str::<Data>(&raw) {
        return Disk::Plain(data);
    }
    // Oldest format: one text per date, e.g. {"2026-10-08": "text"}.
    if let Ok(old) = serde_json::from_str::<BTreeMap<String, String>>(&raw) {
        let mut data = fresh_data();
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
                mood: None,
            })
            .collect();
        return Disk::Plain(data);
    }
    Disk::Unreadable("it is not a diary file this app recognises".into())
}

/// Encrypt and write the diary. The result is decrypted again and checked before the old
/// file is replaced, and the replacement is atomic, so a failure can't destroy your data.
fn save(data: &Data, vault: &vault::Vault) -> Result<(), String> {
    let json = serde_json::to_vec(data).map_err(|e| e.to_string())?;
    let sealed = vault.seal(&json)?;
    let env = vault::Envelope::parse(&sealed).ok_or("could not re-read the encrypted data")?;
    if vault.open(&env)? != json {
        return Err("encryption check failed".into());
    }
    let path = data_path();
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, sealed).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
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


struct EditState {
    /// `Some` when editing an existing entry.
    id: Option<u64>,
    date: NaiveDate,
    hour: u32,
    minute: u32,
    /// Seconds are kept from the original timestamp so entries stay in order.
    second: u32,
    category: String,
    text: String,
    mood: Option<u8>,
    /// Index of the text block that last had the cursor (target of toolbar actions).
    focus: usize,
    /// Move the cursor here next frame: (text block index, char position); `usize::MAX` = last / end.
    want_focus: Option<(usize, usize)>,
}

#[derive(Clone)]
enum LockMode {
    /// No diary file yet: choose a password.
    Create,
    /// Encrypted diary: enter the password.
    Unlock(vault::Envelope),
    /// Unencrypted diary from an earlier version: choose a password to encrypt it.
    Encrypt(Data),
    /// The file exists but can't be used; never overwrite it.
    Broken(String),
}

struct Lock {
    mode: LockMode,
    password: String,
    confirm: String,
    error: String,
    focus: bool,
}

impl Lock {
    fn new(mode: LockMode) -> Self {
        Self { mode, password: String::new(), confirm: String::new(), error: String::new(), focus: true }
    }
}

const MIN_PASSWORD: usize = 8;

fn check_new_password(password: &str, confirm: &str) -> Result<(), String> {
    if password.chars().count() < MIN_PASSWORD {
        Err(format!("Use at least {MIN_PASSWORD} characters."))
    } else if password != confirm {
        Err("The two passwords don't match.".into())
    } else {
        Ok(())
    }
}

struct DiaryApp {
    /// Empty placeholder while the diary is locked.
    data: Data,
    /// Present while unlocked; used to encrypt on every save.
    vault: Option<vault::Vault>,
    /// `Some` while the lock screen is showing.
    lock: Option<Lock>,
    selected: NaiveDate,
    /// First day of the month being displayed.
    month: NaiveDate,
    editor: Option<EditState>,
    show_categories: bool,
    new_cat_name: String,
    new_cat_color: [u8; 3],
    /// Category row being renamed and its in-progress text.
    rename_buf: Option<(usize, String)>,
    show_password: bool,
    show_location: bool,
    show_shortcuts: bool,
    show_export: bool,
    export_scope: ExportScope,
    export_images: bool,
    export_msg: String,
    export_path: Option<std::path::PathBuf>,
    /// Ctrl+S was pressed while writing an entry.
    save_requested: bool,
    location_msg: String,
    pw_new: String,
    pw_confirm: String,
    pw_error: String,
    status: String,
    media: markdown::Media,
    show_marks: bool,
    view: CalView,
    /// Bumped whenever the diary changes; invalidates `stats_cache`.
    rev: u64,
    stats_cache: Option<(u64, BTreeMap<NaiveDate, stats::DayStat>)>,
    /// Entry being dragged on the calendar.
    drag: Option<u64>,
    /// Scroll the day view to the working hours on its next frame.
    day_scroll_pending: bool,
    search: String,
    last_activity: std::time::Instant,
    /// Images used by an unsaved draft, kept while the diary is locked.
    draft_images: BTreeMap<String, String>,
    /// Message shown in the entry editor (e.g. an image that failed to load).
    editor_msg: String,
}

impl DiaryApp {
    fn new() -> Self {
        let today = Local::now().date_naive();
        let mode = current_lock_mode();
        Self {
            data: fresh_data(),
            vault: None,
            lock: Some(Lock::new(mode)),
            selected: today,
            month: today.with_day(1).unwrap(),
            editor: None,
            show_categories: false,
            new_cat_name: String::new(),
            new_cat_color: [59, 130, 246],
            rename_buf: None,
            show_password: false,
            show_location: false,
            show_shortcuts: false,
            show_export: false,
            export_scope: ExportScope::Month,
            export_images: true,
            export_msg: String::new(),
            export_path: None,
            save_requested: false,
            location_msg: String::new(),
            pw_new: String::new(),
            pw_confirm: String::new(),
            pw_error: String::new(),
            status: String::new(),
            media: markdown::Media::default(),
            show_marks: false,
            view: CalView::Month,
            rev: 0,
            stats_cache: None,
            drag: None,
            day_scroll_pending: true,
            search: String::new(),
            last_activity: std::time::Instant::now(),
            draft_images: BTreeMap::new(),
            editor_msg: String::new(),
        }
    }

    fn persist(&mut self) {
        self.rev += 1;
        self.drop_unused_images();
        self.status = match &self.vault {
            Some(v) => match save(&self.data, v) {
                Ok(()) => "Saved".into(),
                Err(e) => format!("Save failed: {e}"),
            },
            None => "Save failed: the diary is locked".into(),
        };
    }

    /// Remove stored images that no entry (or open draft) refers to any more.
    fn drop_unused_images(&mut self) {
        let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
        for e in &self.data.entries {
            used.extend(markdown::image_ids(&e.text));
        }
        if let Some(ed) = &self.editor {
            used.extend(markdown::image_ids(&ed.text));
        }
        self.data.images.retain(|id, _| used.contains(id));
    }

    /// Try to unlock / create / encrypt using what was typed on the lock screen.
    fn submit_lock(&mut self) {
        let Some(lock) = self.lock.as_mut() else { return };
        let password = lock.password.clone();
        let result: Result<(vault::Vault, Data), String> = match lock.mode.clone() {
            LockMode::Unlock(env) => vault::Vault::unlock(&env, &password).and_then(|(v, plain)| {
                let data = serde_json::from_slice::<Data>(&plain).map_err(|e| e.to_string())?;
                Ok((v, data))
            }),
            LockMode::Create => create_vault(&password, &lock.confirm, fresh_data()),
            LockMode::Encrypt(data) => create_vault(&password, &lock.confirm, data),
            LockMode::Broken(_) => return,
        };
        match result {
            Ok((v, data)) => {
                self.vault = Some(v);
                self.data = data;
                self.rev += 1;
                self.data.images.extend(std::mem::take(&mut self.draft_images));
                self.last_activity = std::time::Instant::now();
                self.lock = None;
                self.status.clear();
            }
            Err(e) => {
                lock.error = e;
                lock.password.clear();
                lock.confirm.clear();
                lock.focus = true;
            }
        }
    }

    /// Forget the key and the decrypted data and go back to the lock screen.
    fn lock_now(&mut self) {
        let mode = current_lock_mode();
        // An open draft survives locking (it only exists in memory), along with its images.
        self.draft_images = match &self.editor {
            Some(ed) => markdown::image_ids(&ed.text)
                .into_iter()
                .filter_map(|id| self.data.images.get(&id).map(|b| (id, b.clone())))
                .collect(),
            None => BTreeMap::new(),
        };
        self.data = fresh_data();
        self.vault = None;
        self.media = markdown::Media::default();
        self.stats_cache = None;
        self.search.clear();
        self.show_categories = false;
        self.show_password = false;
        self.show_location = false;
        self.show_export = false;
        self.show_shortcuts = false;
        self.lock = Some(Lock::new(mode));
    }

    fn lock_screen(&mut self, ui: &mut egui::Ui) {
        let p = palette(ui.ctx());
        let dark = ui.ctx().theme() == egui::Theme::Dark;
        let Some(lock) = self.lock.as_mut() else { return };
        let (title, blurb, button) = match &lock.mode {
            LockMode::Create => (
                "Welcome to My Diary",
                "Choose a password to protect your diary. It can't be recovered if you forget it.".to_string(),
                "Create password",
            ),
            LockMode::Encrypt(_) => (
                "Protect your diary",
                "Your diary isn't encrypted yet. Choose a password to encrypt it. It can't be recovered if you forget it."
                    .to_string(),
                "Encrypt diary",
            ),
            LockMode::Unlock(_) => ("My Diary is locked", "Enter your password to open it.".to_string(), "Unlock"),
            LockMode::Broken(why) => (
                "Can't open the diary file",
                format!("{}\n\nThe file was left untouched: {why}.\nFix or move it, then restart the app.", data_path().display()),
                "",
            ),
        };
        let needs_confirm = matches!(lock.mode, LockMode::Create | LockMode::Encrypt(_));
        let broken = matches!(lock.mode, LockMode::Broken(_));
        let mut submit = false;

        ui.vertical_centered(|ui| {
            ui.add_space(((ui.available_height() - 380.0) / 2.0).max(16.0));
            egui::Frame::new()
                .fill(p.panel_bg)
                .stroke(Stroke::new(1.0, p.cell_border))
                .shadow(popup_shadow(dark))
                .inner_margin(28)
                .show(ui, |ui| {
                    ui.set_width(380.0);
                    ui.label(RichText::new(title).size(28.0).strong().color(p.title));
                    ui.add_space(6.0);
                    ui.label(RichText::new(blurb).color(p.muted));
                    ui.add_space(14.0);
                    if broken {
                        return;
                    }

                    let r = ui.add(password_field(&mut lock.password, "Password", &p));
                    if lock.focus {
                        r.request_focus();
                        lock.focus = false;
                    }
                    submit |= pressed_enter(&r, ui) && !needs_confirm;
                    if needs_confirm {
                        ui.add_space(8.0);
                        let r2 = ui.add(password_field(&mut lock.confirm, "Confirm password", &p));
                        submit |= pressed_enter(&r2, ui);
                    }
                    ui.add_space(14.0);
                    let btn = egui::Button::new(RichText::new(button).size(17.0)).min_size(vec2(ui.available_width(), 40.0));
                    if ui.add(btn).clicked() {
                        submit = true;
                    }
                    if !lock.error.is_empty() {
                        ui.add_space(8.0);
                        ui.label(RichText::new(&lock.error).color(Color32::from_rgb(239, 68, 68)));
                    }
                });
        });
        if submit {
            self.submit_lock();
        }
    }

    fn password_window(&mut self, ctx: &egui::Context) {
        if !self.show_password {
            return;
        }
        let p = palette(ctx);
        let dark = ctx.theme() == egui::Theme::Dark;
        let mut close = false;
        let mut apply = false;

        egui::Window::new("Change password")
            .title_bar(false)
            .frame(popup_frame(&p, dark))
            .resizable(false)
            .pivot(Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                close |= popup_header(ui, &p, "Change password");
                egui::Frame::new().inner_margin(14).show(ui, |ui| {
                    ui.set_width(340.0);
                    ui.add(password_field(&mut self.pw_new, "New password", &p));
                    ui.add_space(8.0);
                    let r = ui.add(password_field(&mut self.pw_confirm, "Confirm new password", &p));
                    apply |= pressed_enter(&r, ui);
                    ui.add_space(10.0);
                    if ui.button("Change password").clicked() {
                        apply = true;
                    }
                    if !self.pw_error.is_empty() {
                        ui.add_space(6.0);
                        ui.label(RichText::new(&self.pw_error).color(Color32::from_rgb(239, 68, 68)));
                    }
                });
            });

        if apply {
            match create_vault(&self.pw_new, &self.pw_confirm, self.data.clone()) {
                Ok((v, _)) => {
                    self.vault = Some(v);
                    self.status = "Password changed".into();
                    close = true;
                }
                Err(e) => self.pw_error = e,
            }
        }
        if close {
            self.show_password = false;
            self.pw_new.clear();
            self.pw_confirm.clear();
            self.pw_error.clear();
        }
    }


    fn location_window(&mut self, ctx: &egui::Context) {
        if !self.show_location {
            return;
        }
        let p = palette(ctx);
        let dark = ctx.theme() == egui::Theme::Dark;
        let mut close = false;
        let (mut reveal, mut move_it, mut use_other) = (false, false, false);

        egui::Window::new("Data location")
            .title_bar(false)
            .frame(popup_frame(&p, dark))
            .resizable(false)
            .pivot(Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                close |= popup_header(ui, &p, "Data location");
                egui::Frame::new().inner_margin(14).show(ui, |ui| {
                    ui.set_width(480.0);
                    ui.label("Your diary (always encrypted) is stored in this folder:");
                    ui.add_space(4.0);
                    ui.add(egui::Label::new(RichText::new(location::data_dir().display().to_string()).monospace().color(p.ink)).wrap());
                    ui.add_space(6.0);
                    reveal = ui.button("Show in Explorer").clicked();
                    ui.separator();
                    ui.label(RichText::new("Move it somewhere else").strong().color(p.title));
                    ui.label(RichText::new("Copies the diary to a folder you choose and uses it from there.").color(p.muted));
                    move_it = ui.button("Move diary to another folder…").clicked();
                    ui.add_space(8.0);
                    ui.label(RichText::new("Open a different diary").strong().color(p.title));
                    ui.label(RichText::new("Use a diary that already exists in another folder, such as a backup. You will be asked for its password.").color(p.muted));
                    use_other = ui.button("Use a diary from another folder…").clicked();
                    if !self.location_msg.is_empty() {
                        ui.add_space(8.0);
                        ui.label(RichText::new(&self.location_msg).color(p.ink));
                    }
                });
            });

        if reveal {
            std::process::Command::new("explorer").arg(location::data_dir()).spawn().ok();
        }
        if move_it {
            if let Some(dir) = rfd::FileDialog::new().set_directory(location::data_dir()).pick_folder() {
                self.location_msg = match location::move_to(dir) {
                    Ok(()) => format!("Moved. The diary is now in {}", location::data_dir().display()),
                    Err(e) => e,
                };
            }
        }
        if use_other {
            if let Some(dir) = rfd::FileDialog::new().set_directory(location::data_dir()).pick_folder() {
                match location::use_existing(dir) {
                    Ok(()) => {
                        // Back to the lock screen, now pointing at the other diary.
                        self.editor = None;
                        self.location_msg.clear();
                        self.show_location = false;
                        self.lock_now();
                    }
                    Err(e) => self.location_msg = e,
                }
            }
        }
        if close {
            self.show_location = false;
            self.location_msg.clear();
        }
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
        self.new_entry_at(date, None);
    }

    /// Start a new entry; `hour` picks the time (on the hour), otherwise it is now.
    fn new_entry_at(&mut self, date: NaiveDate, hour: Option<u32>) {
        let category = self.data.categories.first().map(|c| c.name.clone()).unwrap_or_default();
        let now = Local::now();
        let (hour, minute, second) = match hour {
            Some(h) => (h, 0, 0),
            None => (now.hour(), now.minute(), now.second()),
        };
        self.editor = Some(EditState {
            id: None,
            date,
            hour,
            minute,
            second,
            category,
            text: String::new(),
            mood: None,
            focus: 0,
            want_focus: Some((usize::MAX, usize::MAX)),
        });
        self.editor_msg.clear();
    }

    /// Step the calendar one month / week / day / year.
    fn step_calendar(&mut self, dir: i64) {
        match self.view {
            CalView::Month => self.shift_month(dir as i32),
            CalView::Week => self.shift_days(7 * dir),
            CalView::Day => self.shift_days(dir),
            CalView::Year => self.shift_year(dir as i32),
        }
    }

    fn go_today(&mut self) {
        let t = Local::now().date_naive();
        self.selected = t;
        self.month = t.with_day(1).unwrap();
        self.day_scroll_pending = true;
        self.search.clear();
    }

    /// Keyboard shortcuts (listed in the "Keyboard shortcuts" window).
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        if self.lock.is_some() {
            return;
        }
        let cmd = |key: Key| ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, key));
        let editing = self.editor.is_some();

        if cmd(Key::L) {
            self.lock_now();
            return;
        }
        if editing {
            // Ctrl+S saves the entry being written.
            if cmd(Key::S) || cmd(Key::Enter) {
                self.save_requested = true;
            }
            return;
        }
        if cmd(Key::N) {
            self.new_entry(self.selected);
        }
        if cmd(Key::F) {
            ctx.memory_mut(|m| m.request_focus(search_id()));
        }
        if cmd(Key::E) {
            self.show_export = true;
        }
        if cmd(Key::T) {
            self.go_today();
        }
        for (key, view) in [(Key::Num1, CalView::Day), (Key::Num2, CalView::Week), (Key::Num3, CalView::Month), (Key::Num4, CalView::Year)] {
            if cmd(key) {
                self.view = view;
                self.day_scroll_pending = true;
            }
        }
        // Arrow keys move through time, unless a text box wants them.
        if !ctx.egui_wants_keyboard_input() {
            if ctx.input(|i| i.key_pressed(Key::ArrowLeft)) {
                self.step_calendar(-1);
            }
            if ctx.input(|i| i.key_pressed(Key::ArrowRight)) {
                self.step_calendar(1);
            }
        }
    }

    /// First day, last day, a description and a file-name part for an export scope.
    fn export_range(&self, scope: ExportScope) -> (NaiveDate, NaiveDate, String, String) {
        let d = self.selected;
        match scope {
            ExportScope::Day => (d, d, d.format("%A %e %B %Y").to_string(), key(d)),
            ExportScope::Week => {
                let monday = week_start(d);
                (monday, monday + chrono::Days::new(6), week_title(d), format!("week-{}", key(monday)))
            }
            ExportScope::Month => {
                let first = d.with_day(1).unwrap();
                let last = self.shift_target(first).pred_opt().unwrap();
                (first, last, d.format("%B %Y").to_string(), d.format("%Y-%m").to_string())
            }
            ExportScope::Year => {
                let (first, last) = (NaiveDate::from_ymd_opt(d.year(), 1, 1).unwrap(), NaiveDate::from_ymd_opt(d.year(), 12, 31).unwrap());
                (first, last, d.year().to_string(), d.year().to_string())
            }
            ExportScope::All => (NaiveDate::from_ymd_opt(1, 1, 1).unwrap(), NaiveDate::from_ymd_opt(9999, 12, 31).unwrap(), "Everything".into(), "all".into()),
        }
    }

    fn entries_in(&self, from: NaiveDate, to: NaiveDate) -> Vec<&Entry> {
        let (from, to) = (key(from), key(to));
        self.data.entries.iter().filter(|e| e.date >= from && e.date <= to).collect()
    }

    fn export_window(&mut self, ctx: &egui::Context) {
        if !self.show_export {
            return;
        }
        let p = palette(ctx);
        let dark = ctx.theme() == egui::Theme::Dark;
        let mut close = false;
        let mut create = false;
        let mut open_file = false;
        let scopes = [
            (ExportScope::Day, "This day"),
            (ExportScope::Week, "This week"),
            (ExportScope::Month, "This month"),
            (ExportScope::Year, "This year"),
            (ExportScope::All, "Everything"),
        ];
        let choices: Vec<(ExportScope, String)> = scopes
            .iter()
            .map(|(scope, name)| {
                let (from, to, what, _) = self.export_range(*scope);
                let n = self.entries_in(from, to).len();
                let what = if *scope == ExportScope::All { String::new() } else { format!(" – {what}") };
                (*scope, format!("{name}{what}  ({n} {})", if n == 1 { "entry" } else { "entries" }))
            })
            .collect();

        egui::Window::new("Export to PDF")
            .title_bar(false)
            .frame(popup_frame(&p, dark))
            .resizable(false)
            .pivot(Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                close |= popup_header(ui, &p, "Export to PDF");
                egui::Frame::new().inner_margin(14).show(ui, |ui| {
                    ui.set_width(460.0);
                    ui.label(RichText::new("What to include").strong().color(p.title));
                    ui.add_space(4.0);
                    for (scope, label) in &choices {
                        ui.radio_value(&mut self.export_scope, *scope, label);
                    }
                    ui.add_space(8.0);
                    ui.checkbox(&mut self.export_images, "Include pictures");
                    ui.add_space(6.0);
                    ui.label(RichText::new("The PDF is a plain copy of your entries. It is not encrypted or password protected, so keep it somewhere safe.").small().color(p.muted));
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        create = ui.button("Create PDF…").clicked();
                        if self.export_path.is_some() {
                            open_file = ui.button("Open last PDF").clicked();
                        }
                    });
                    if !self.export_msg.is_empty() {
                        ui.add_space(8.0);
                        ui.add(egui::Label::new(RichText::new(&self.export_msg).color(p.ink)).wrap());
                    }
                });
            });

        if create {
            let (from, to, what, slug) = self.export_range(self.export_scope);
            let entries = self.entries_in(from, to);
            let name = format!("diary-{slug}.pdf");
            let dest = rfd::FileDialog::new().set_file_name(name).add_filter("PDF document", &["pdf"]).save_file();
            if let Some(dest) = dest {
                let report = export::Report {
                    title: "My Diary".into(),
                    subtitle: format!(
                        "{what} · {} {} · created {}",
                        entries.len(),
                        if entries.len() == 1 { "entry" } else { "entries" },
                        Local::now().format("%e %B %Y")
                    ),
                    entries,
                    include_images: self.export_images,
                };
                self.export_msg = match export::build_pdf(&report, &self.data.categories, &self.data.images)
                    .and_then(|bytes| std::fs::write(&dest, bytes).map_err(|e| format!("Couldn't save the PDF: {e}")))
                {
                    Ok(()) => {
                        self.export_path = Some(dest.clone());
                        format!("Saved {}", dest.display())
                    }
                    Err(e) => e,
                };
            }
        }
        if open_file {
            if let Some(path) = &self.export_path {
                std::process::Command::new("cmd").args(["/C", "start", ""]).arg(path).spawn().ok();
            }
        }
        if close {
            self.show_export = false;
        }
    }

    fn shortcuts_window(&mut self, ctx: &egui::Context) {
        if !self.show_shortcuts {
            return;
        }
        let p = palette(ctx);
        let dark = ctx.theme() == egui::Theme::Dark;
        let mut close = false;
        egui::Window::new("Keyboard shortcuts")
            .title_bar(false)
            .frame(popup_frame(&p, dark))
            .resizable(false)
            .pivot(Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                close |= popup_header(ui, &p, "Keyboard shortcuts");
                egui::Frame::new().inner_margin(14).show(ui, |ui| {
                    let rows: [(&str, &str); 14] = [
                        ("Ctrl+N", "New entry on the selected day"),
                        ("Ctrl+F", "Search entries"),
                        ("Ctrl+T", "Jump to today"),
                        ("Ctrl+L", "Lock the diary"),
                        ("Ctrl+E", "Export entries to PDF"),
                        ("← / →", "Previous / next month, week, day or year"),
                        ("Ctrl+1 … 4", "Day, Week, Month, Year view"),
                        ("Double-click", "A day: new entry. An entry: edit it"),
                        ("Drag", "Move an entry to another day or hour"),
                        ("Esc", "Cancel a drag"),
                        ("Ctrl+S", "Save the entry (while writing)"),
                        ("Ctrl+Enter", "Save the entry (while writing)"),
                        ("Ctrl+B / Ctrl+I", "Bold / italic (while writing)"),
                        ("Click again", "A mood button or toolbar format removes it"),
                    ];
                    egui::Grid::new("shortcut_grid").num_columns(2).spacing([24.0, 8.0]).show(ui, |ui| {
                        for (keys, what) in rows {
                            ui.label(RichText::new(keys).strong().monospace().color(p.title));
                            ui.label(RichText::new(what).color(p.ink));
                            ui.end_row();
                        }
                    });
                });
            });
        if close {
            self.show_shortcuts = false;
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        let p = palette(ui.ctx());
        ui.horizontal_centered(|ui| {
            ui.label(RichText::new("My Diary").size(24.0).strong().color(Color32::WHITE));
            ui.add_space(24.0);
            ui.add_space(8.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .id(search_id())
                    .hint_text("Search entries (Ctrl+F)")
                    .text_color(p.ink)
                    .margin(egui::Margin::symmetric(8, 5))
                    .desired_width(190.0),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Settings cog: the last item in the header.
                let (rect, cog) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::click());
                let cog = cog.on_hover_text("Settings");
                ui.painter().rect_filled(rect, 8.0, if cog.hovered() || cog.is_pointer_button_down_on() { BLUE_HOVER } else { BLUE });
                paint_cog(ui.painter(), rect.center(), 11.0, Color32::WHITE, if cog.hovered() { BLUE_HOVER } else { BLUE });
                let mut settings_changed = false;
                egui::Popup::menu(&cog).show(|ui| {
                    if ui.button("Change password…").clicked() {
                        self.show_password = true;
                        ui.close();
                    }
                    if ui.button("Keyboard shortcuts…").clicked() {
                        self.show_shortcuts = true;
                        ui.close();
                    }
                    if ui.button("Export to PDF…").clicked() {
                        self.show_export = true;
                        ui.close();
                    }
                    if ui.button("Data location…").clicked() {
                        self.show_location = true;
                        ui.close();
                    }
                    if ui.button("Back up encrypted copy…").clicked() {
                        self.backup();
                        ui.close();
                    }
                    ui.menu_button("Daily word goal", |ui| {
                        for (label, words) in [("Off", 0), ("100 words", 100), ("250 words", 250), ("500 words", 500), ("750 words", 750), ("1,000 words", 1000)] {
                            settings_changed |= ui.radio_value(&mut self.data.settings.daily_word_goal, words, label).changed();
                        }
                    });
                    ui.menu_button("Auto-lock", |ui| {
                        for (label, mins) in [("Off", 0), ("After 1 minute", 1), ("After 5 minutes", 5), ("After 10 minutes", 10), ("After 30 minutes", 30)] {
                            settings_changed |= ui.radio_value(&mut self.data.settings.auto_lock_minutes, mins, label).changed();
                        }
                    });
                });
                if settings_changed {
                    self.persist();
                }
                if ui.button("Lock").on_hover_text("Lock the diary (Ctrl+L)").clicked() {
                    self.lock_now();
                }
                if ui.button("🎨 Categories").clicked() {
                    self.show_categories = true;
                }
                if ui.button("➕ New entry").on_hover_text("New entry (Ctrl+N)").clicked() {
                    self.new_entry(self.selected);
                }
            });
        });
    }

    /// Copy the (already encrypted) diary file somewhere chosen by the user.
    fn backup(&mut self) {
        let name = format!("diary-backup-{}.json", Local::now().format("%Y-%m-%d"));
        let dest = rfd::FileDialog::new().set_file_name(name).add_filter("Encrypted diary", &["json"]).save_file();
        if let Some(dest) = dest {
            self.status = match std::fs::copy(data_path(), &dest) {
                Ok(_) => format!("Backup saved to {}", dest.display()),
                Err(e) => format!("Save failed: backup could not be written ({e})"),
            };
        }
    }

    /// Toolbar above the calendar: navigation, title and the Month / Week / Day switch.
    fn calendar_toolbar(&mut self, ui: &mut egui::Ui) {
        let p = palette(ui.ctx());
        ui.horizontal(|ui| {
            ui.spacing_mut().interact_size.y = 34.0;
            if ui.button("◀").clicked() {
                self.step_calendar(-1);
            }
            if ui.button("▶").clicked() {
                self.step_calendar(1);
            }
            ui.add_space(6.0);
            ui.label(RichText::new(self.calendar_title()).size(24.0).strong().color(p.title));
            ui.add_space(6.0);
            if ui.button("Today").on_hover_text("Jump to today (Ctrl+T)").clicked() {
                self.go_today();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for (view, label) in [(CalView::Day, "Day"), (CalView::Week, "Week"), (CalView::Month, "Month"), (CalView::Year, "Year")] {
                    let selected = self.view == view;
                    let mut button = egui::Button::new(RichText::new(label).strong().color(Color32::WHITE));
                    if selected {
                        button = button.fill(Color32::from_rgb(23, 37, 84)).stroke(Stroke::new(2.0, p.accent));
                    }
                    if ui.add(button).clicked() {
                        self.view = view;
                        self.day_scroll_pending = true;
                    }
                }
            });
        });
    }

    fn calendar_title(&self) -> String {
        match self.view {
            CalView::Month => self.month.format("%B %Y").to_string(),
            CalView::Week => week_title(self.selected),
            CalView::Day => self.selected.format("%A %e %B %Y").to_string(),
            CalView::Year => self.selected.year().to_string(),
        }
    }

    fn shift_days(&mut self, delta: i64) {
        let moved = if delta >= 0 {
            self.selected.checked_add_days(chrono::Days::new(delta as u64))
        } else {
            self.selected.checked_sub_days(chrono::Days::new(delta.unsigned_abs()))
        };
        if let Some(d) = moved {
            self.selected = d;
            self.month = d.with_day(1).unwrap();
            self.day_scroll_pending = true;
        }
    }

    fn calendar(&mut self, ui: &mut egui::Ui) {
        self.calendar_toolbar(ui);
        ui.add_space(6.0);

        let mut ev = CalEvents::default();
        match self.view {
            CalView::Month => {
                let first = self.month;
                let days = (self.shift_target(first) - first).num_days() as u64;
                let days: Vec<NaiveDate> = (0..days).map(|n| first + chrono::Days::new(n)).collect();
                self.grid_view(ui, &mut ev, &days, first.weekday().num_days_from_monday(), 21.0);
            }
            CalView::Week => {
                let monday = week_start(self.selected);
                let days: Vec<NaiveDate> = (0..7).map(|n| monday + chrono::Days::new(n)).collect();
                self.grid_view(ui, &mut ev, &days, 0, 28.0);
            }
            CalView::Day => self.day_timeline(ui, &mut ev),
            CalView::Year => self.year_view(ui, &mut ev),
        }

        // Escape cancels a drag in progress.
        if self.drag.is_some() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.drag = None;
        }
        if let Some(d) = ev.select {
            self.selected = d;
            self.month = d.with_day(1).unwrap();
        }
        if let Some((d, hour)) = ev.new_on {
            self.selected = d;
            self.new_entry_at(d, hour);
        }
        if let Some(id) = ev.edit {
            self.start_edit(id);
        }
        if let Some((id, date, hour)) = ev.drop {
            self.move_entry(id, date, hour);
        }
    }

    /// Month and week views: a grid of day cells, seven to a row.
    fn grid_view(&mut self, ui: &mut egui::Ui, ev: &mut CalEvents, days: &[NaiveDate], offset: u32, pill_h: f32) {
        let rows = (offset + days.len() as u32).div_ceil(7);
        let head_h = 28.0;
        let avail = ui.available_size();
        let cell_w = avail.x / 7.0;
        let min_h = if rows == 1 { 220.0 } else { 70.0 };
        let cell_h = ((avail.y - head_h) / rows as f32).max(min_h);
        let (grid, _) = ui.allocate_exact_size(vec2(avail.x, head_h + cell_h * rows as f32), Sense::hover());
        let painter = ui.painter_at(grid);
        let p = palette(ui.ctx());
        let today = Local::now().date_naive();
        let text_size = if rows == 1 { 15.0 } else { 14.0 };

        for (i, name) in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].iter().enumerate() {
            let r = egui::Rect::from_min_size(grid.min + vec2(cell_w * i as f32, 0.0), vec2(cell_w, head_h));
            painter.rect_filled(r.shrink(1.0), 4.0, p.head_bg);
            painter.text(r.center(), Align2::CENTER_CENTER, *name, FontId::proportional(16.0), p.head_text);
        }

        self.ensure_stats();
        self.ensure_stats();
        let by_day: Vec<Vec<Entry>> = days.iter().map(|d| self.day_entries(*d).into_iter().cloned().collect()).collect();
        let cats = &self.data.categories;
        let drag = &mut self.drag;
        let selected = self.selected;
        let day_stats = &self.stats_cache.as_ref().unwrap().1;
        let mut cells: Vec<(egui::Rect, NaiveDate)> = Vec::new();

        for (n, day) in days.iter().copied().enumerate() {
            let slot = offset as usize + n;
            let (col, row) = (slot % 7, slot / 7);
            let rect = egui::Rect::from_min_size(
                grid.min + vec2(cell_w * col as f32, head_h + cell_h * row as f32),
                vec2(cell_w, cell_h),
            )
            .shrink(1.5);
            cells.push((rect, day));

            let resp = ui.interact(rect, ui.id().with(("cell", day)), Sense::click());
            let bg = if day == today {
                p.today
            } else if col >= 5 {
                p.weekend
            } else if resp.hovered() && drag.is_none() {
                p.hover
            } else {
                p.cell
            };
            painter.rect_filled(rect, 6.0, bg);
            let border = if day == selected { Stroke::new(2.5, p.accent) } else { Stroke::new(1.0, p.cell_border) };
            painter.rect_stroke(rect, 6.0, border, egui::StrokeKind::Inside);
            painter.text(rect.min + vec2(8.0, 6.0), Align2::LEFT_TOP, day.day().to_string(), FontId::proportional(19.0), p.day_num);
            if let Some(m) = day_stats.get(&day).and_then(|s| s.mood()) {
                let c = egui::pos2(rect.max.x - 14.0, rect.min.y + 15.0);
                painter.circle_filled(c, 6.5, mood_color(m));
                painter.circle_stroke(c, 6.5, Stroke::new(1.0, Color32::from_black_alpha(90)));
            }

            let entries = &by_day[n];
            let top = rect.min.y + 30.0;
            let fit = (((rect.max.y - top - 4.0) / (pill_h + 2.0)).floor() as usize).max(1);
            let shown = if entries.len() > fit { fit - 1 } else { entries.len() };
            for (i, e) in entries.iter().take(shown).enumerate() {
                let pr = egui::Rect::from_min_size(
                    egui::pos2(rect.min.x + 4.0, top + i as f32 * (pill_h + 2.0)),
                    vec2(rect.width() - 8.0, pill_h),
                );
                pill(ui, &painter, pr, e, cats, text_size, drag, ev);
            }
            if entries.len() > shown {
                painter.text(
                    egui::pos2(rect.min.x + 8.0, top + shown as f32 * (pill_h + 2.0) + pill_h / 2.0),
                    Align2::LEFT_CENTER,
                    format!("+{} more", entries.len() - shown),
                    FontId::proportional(text_size),
                    p.muted,
                );
            }

            if resp.double_clicked() {
                ev.new_on = Some((day, None));
            } else if resp.clicked() {
                ev.select = Some(day);
            }
        }

        // Dropping an entry on a day moves it there; the day under the pointer is outlined.
        let pointer = ui.ctx().pointer_latest_pos();
        if let Some((id, pos)) = ev.drop_at.take() {
            if let Some((_, d)) = cells.iter().find(|(r, _)| r.contains(pos)) {
                ev.drop = Some((id, *d, None));
            }
        }
        if drag.is_some() {
            if let Some((r, _)) = pointer.and_then(|pos| cells.iter().find(|(r, _)| r.contains(pos))) {
                painter.rect_stroke(*r, 6.0, Stroke::new(3.0, p.accent), egui::StrokeKind::Inside);
            }
        }
    }

    /// Day view: the selected day hour by hour. Entries sit in the hour they were made.
    fn day_timeline(&mut self, ui: &mut egui::Ui, ev: &mut CalEvents) {
        let p = palette(ui.ctx());
        let day = self.selected;
        let now = Local::now();
        let entries: Vec<Entry> = self.day_entries(day).into_iter().cloned().collect();
        let first_hour = entries.iter().filter_map(|e| entry_time(&e.added_at).map(|t| t.0)).min().unwrap_or(8).min(8);
        let cats = &self.data.categories;
        let drag = &mut self.drag;
        let scroll_pending = &mut self.day_scroll_pending;
        let mut rows: Vec<(egui::Rect, u32)> = Vec::new();

        egui::ScrollArea::vertical().id_salt("day_scroll").auto_shrink([false, false]).show(ui, |ui| {
            let width = ui.available_width();
            let gutter = 64.0;
            for hour in 0..24u32 {
                let in_hour: Vec<&Entry> = entries.iter().filter(|e| entry_time(&e.added_at).map(|t| t.0) == Some(hour)).collect();
                let row_h = (in_hour.len() as f32 * 32.0 + 12.0).max(56.0);
                let (rect, _) = ui.allocate_exact_size(vec2(width, row_h), Sense::hover());
                if *scroll_pending && hour == first_hour {
                    ui.scroll_to_rect(rect, Some(egui::Align::TOP));
                    *scroll_pending = false;
                }
                let painter = ui.painter_at(rect);
                let slot = egui::Rect::from_min_max(rect.min + vec2(gutter, 0.0), rect.max);
                rows.push((slot, hour));
                let resp = ui.interact(slot, ui.id().with(("hour", hour)), Sense::click());
                let bg = if day == now.date_naive() && hour == now.hour() {
                    p.today
                } else if resp.hovered() && drag.is_none() {
                    p.hover
                } else if !(7..22).contains(&hour) {
                    p.weekend
                } else {
                    p.cell
                };
                painter.rect_filled(slot.shrink(1.0), 4.0, bg);
                painter.rect_stroke(slot.shrink(1.0), 4.0, Stroke::new(1.0, p.cell_border), egui::StrokeKind::Inside);
                painter.text(
                    egui::pos2(rect.min.x + gutter - 10.0, rect.min.y + 8.0),
                    Align2::RIGHT_TOP,
                    format!("{hour:02}:00"),
                    FontId::proportional(15.0),
                    p.muted,
                );
                for (i, e) in in_hour.iter().enumerate() {
                    let pr = egui::Rect::from_min_size(slot.min + vec2(8.0, 8.0 + i as f32 * 32.0), vec2(slot.width() - 16.0, 28.0));
                    pill(ui, &painter, pr, e, cats, 16.0, drag, ev);
                }
                if resp.double_clicked() {
                    ev.new_on = Some((day, Some(hour)));
                }
            }
        });

        // Dropping an entry on an hour changes its time (keeping the minutes).
        let pointer = ui.ctx().pointer_latest_pos();
        if let Some((id, pos)) = ev.drop_at.take() {
            if let Some((_, h)) = rows.iter().find(|(r, _)| r.contains(pos)) {
                ev.drop = Some((id, day, Some(*h)));
            }
        }
        if drag.is_some() {
            if let Some((r, _)) = pointer.and_then(|pos| rows.iter().find(|(r, _)| r.contains(pos))) {
                ui.painter().rect_stroke(*r, 4.0, Stroke::new(3.0, p.accent), egui::StrokeKind::Inside);
            }
        }
    }

    fn shift_year(&mut self, delta: i32) {
        let year = self.selected.year() + delta;
        // 29 February maps to 28 February in a non-leap year.
        let moved = NaiveDate::from_ymd_opt(year, self.selected.month(), self.selected.day())
            .or_else(|| NaiveDate::from_ymd_opt(year, self.selected.month(), 28));
        if let Some(d) = moved {
            self.selected = d;
            self.month = d.with_day(1).unwrap();
        }
    }

    /// Recompute the per-day totals if the diary has changed since they were last worked out.
    fn ensure_stats(&mut self) {
        if self.stats_cache.as_ref().map(|c| c.0) != Some(self.rev) {
            self.stats_cache = Some((self.rev, stats::per_day(&self.data.entries)));
        }
    }

    /// Year view: a GitHub-style heat-map of the days written, plus totals, categories and mood.
    fn year_view(&mut self, ui: &mut egui::Ui, ev: &mut CalEvents) {
        self.ensure_stats();
        let p = palette(ui.ctx());
        let dark = ui.ctx().theme() == egui::Theme::Dark;
        let year = self.selected.year();
        let today = Local::now().date_naive();
        let goal = self.data.settings.daily_word_goal;
        let (jan1, dec31) = (NaiveDate::from_ymd_opt(year, 1, 1).unwrap(), NaiveDate::from_ymd_opt(year, 12, 31).unwrap());
        let days = &self.stats_cache.as_ref().unwrap().1;
        let summary = stats::summarise(&self.data.entries, days, jan1, dec31, today, goal);
        let series = stats::mood_series(days, jan1, dec31);
        let cats = &self.data.categories;
        let selected = self.selected;
        let mut open_day = None;

        egui::ScrollArea::vertical().id_salt("year_scroll").auto_shrink([false, false]).show(ui, |ui| {
            // ---- heat-map: one column per week, one row per weekday
            let (label_w, gap, top_h) = (38.0, 3.0, 22.0);
            let start = week_start(jan1);
            let weeks = ((week_start(dec31) - start).num_days() / 7 + 1) as f32;
            let cell = ((ui.available_width() - label_w - gap * weeks) / weeks).clamp(8.0, 24.0);
            let (rect, _) = ui.allocate_exact_size(vec2(label_w + weeks * (cell + gap), top_h + 7.0 * (cell + gap)), Sense::hover());
            let painter = ui.painter_at(rect);
            let cell_pos = |day: NaiveDate| {
                let col = ((week_start(day) - start).num_days() / 7) as f32;
                let row = day.weekday().num_days_from_monday() as f32;
                rect.min + vec2(label_w + col * (cell + gap), top_h + row * (cell + gap))
            };
            for m in 1..=12 {
                let first = NaiveDate::from_ymd_opt(year, m, 1).unwrap();
                painter.text(
                    egui::pos2(cell_pos(first).x, rect.min.y),
                    Align2::LEFT_TOP,
                    first.format("%b").to_string(),
                    FontId::proportional(14.0),
                    p.muted,
                );
            }
            for (row, name) in [(0.0, "Mon"), (2.0, "Wed"), (4.0, "Fri")] {
                painter.text(
                    egui::pos2(rect.min.x, rect.min.y + top_h + row * (cell + gap)),
                    Align2::LEFT_TOP,
                    name,
                    FontId::proportional(12.0),
                    p.muted,
                );
            }
            for n in 0..=(dec31 - jan1).num_days() {
                let day = jan1 + chrono::Days::new(n as u64);
                let r = egui::Rect::from_min_size(cell_pos(day), vec2(cell, cell));
                let stat = days.get(&day);
                painter.rect_filled(r, 3.0, heat_color(stats::heat_level(stat), dark));
                if day == today {
                    painter.rect_stroke(r, 3.0, Stroke::new(2.0, p.accent), egui::StrokeKind::Outside);
                }
                if day == selected {
                    painter.rect_stroke(r, 3.0, Stroke::new(2.0, p.day_num), egui::StrokeKind::Outside);
                }
                let resp = ui.interact(r, ui.id().with(("heat", day)), Sense::click());
                let resp = resp.on_hover_ui(|ui| {
                    let (n, w) = stat.map_or((0, 0), |s| (s.entries, s.words));
                    ui.label(RichText::new(day.format("%A %e %B %Y").to_string()).strong());
                    ui.label(format!("{n} {} · {w} words", if n == 1 { "entry" } else { "entries" }));
                });
                if resp.double_clicked() {
                    open_day = Some(day);
                } else if resp.clicked() {
                    ev.select = Some(day);
                }
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new("Less").small().color(p.muted));
                for level in 0..=4u8 {
                    let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                    ui.painter().rect_filled(r, 3.0, heat_color(level, dark));
                }
                ui.label(RichText::new("More  ·  shade shows words written that day. Click a day to select it, double-click to open it.").small().color(p.muted));
            });

            // ---- totals
            ui.add_space(14.0);
            ui.label(RichText::new(format!("{year} in numbers")).size(20.0).strong().color(p.title));
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                let mut tile = |label: &str, value: String| {
                    egui::Frame::new().fill(p.cell).stroke(Stroke::new(1.0, p.cell_border)).corner_radius(8).inner_margin(12).show(ui, |ui| {
                        ui.set_min_width(110.0);
                        ui.label(RichText::new(value).size(28.0).strong().color(p.title));
                        ui.label(RichText::new(label).color(p.muted));
                    });
                };
                tile("entries", summary.entries.to_string());
                tile("days written", summary.days_written.to_string());
                tile("words", summary.words.to_string());
                tile("day streak now", summary.current_streak.to_string());
                tile("longest streak", summary.longest_streak.to_string());
                tile("average mood", summary.mood_avg.map_or("–".to_string(), |m| format!("{m:.1}")));
                if goal > 0 {
                    tile(&format!("days with {goal}+ words"), summary.goal_days.to_string());
                }
            });

            // ---- categories
            ui.add_space(14.0);
            ui.label(RichText::new("Entries by category").size(20.0).strong().color(p.title));
            ui.add_space(6.0);
            if summary.categories.is_empty() {
                ui.label(RichText::new("Nothing written in this year yet.").color(p.muted));
            }
            let most = summary.categories.first().map_or(1, |c| c.1).max(1) as f32;
            for (name, count) in &summary.categories {
                ui.horizontal(|ui| {
                    ui.add_sized([120.0, 22.0], egui::Label::new(RichText::new(name).color(p.ink)).truncate());
                    let max_w = (ui.available_width() - 50.0).max(40.0);
                    let (r, _) = ui.allocate_exact_size(vec2(max_w * *count as f32 / most, 20.0), Sense::hover());
                    ui.painter().rect_filled(r, 5.0, category_color(cats, name));
                    ui.label(RichText::new(count.to_string()).color(p.ink));
                });
            }

            // ---- mood
            ui.add_space(14.0);
            ui.label(RichText::new("Mood through the year").size(20.0).strong().color(p.title));
            ui.add_space(6.0);
            if series.is_empty() {
                ui.label(RichText::new("Rate your entries with the Mood buttons in the editor and they will be charted here.").color(p.muted));
            } else {
                mood_chart(ui, &p, &series, year);
                ui.label(RichText::new("Dots are the day's average mood; the line is a 7-day average.").small().color(p.muted));
            }
            ui.add_space(12.0);
        });

        if let Some(day) = open_day {
            ev.select = Some(day);
            self.view = CalView::Day;
            self.day_scroll_pending = true;
        }
    }

    /// Move an entry to another day (and, in the day view, another hour).
    fn move_entry(&mut self, id: u64, date: NaiveDate, hour: Option<u32>) {
        let Some(e) = self.data.entries.iter_mut().find(|e| e.id == id) else { return };
        let new_stamp = moved_timestamp(&e.added_at, date, hour);
        if e.date == key(date) && e.added_at == new_stamp {
            return;
        }
        e.date = key(date);
        e.added_at = new_stamp;
        self.selected = date;
        self.month = date.with_day(1).unwrap();
        self.persist();
    }

    /// Open the editor for an existing entry.
    fn start_edit(&mut self, id: u64) {
        let Some(e) = self.data.entries.iter().find(|e| e.id == id) else { return };
        let date = NaiveDate::parse_from_str(&e.date, "%Y-%m-%d").unwrap_or(self.selected);
        let (hour, minute, second) = entry_time(&e.added_at).unwrap_or((0, 0, 0));
        self.editor = Some(EditState {
            id: Some(id),
            date,
            hour,
            minute,
            second,
            category: e.category.clone(),
            text: e.text.clone(),
            mood: e.mood,
            focus: 0,
            want_focus: Some((usize::MAX, usize::MAX)),
        });
        self.editor_msg.clear();
    }

    fn shift_target(&self, first: NaiveDate) -> NaiveDate {
        let total = first.year() * 12 + first.month0() as i32 + 1;
        NaiveDate::from_ymd_opt(total.div_euclid(12), total.rem_euclid(12) as u32 + 1, 1).unwrap()
    }

    fn day_panel(&mut self, ui: &mut egui::Ui) {
        let p = palette(ui.ctx());
        if !self.search.trim().is_empty() {
            self.search_results(ui, &p);
            return;
        }
        let entries: Vec<Entry> = self.day_entries(self.selected).into_iter().cloned().collect();
        let memories: Vec<Entry> = stats::on_this_day(&self.data.entries, self.selected).into_iter().cloned().collect();
        let words: usize = entries.iter().map(|e| markdown::word_count(&e.text)).sum();
        let goal = self.data.settings.daily_word_goal as usize;

        ui.label(RichText::new(self.selected.format("%A").to_string()).size(14.0).color(p.muted));
        ui.label(RichText::new(self.selected.format("%e %B %Y").to_string()).size(22.0).strong().color(p.title));
        ui.add_space(6.0);
        if ui.button("➕ Add entry for this day").on_hover_text("New entry (Ctrl+N)").clicked() {
            self.new_entry(self.selected);
        }
        if goal > 0 {
            ui.add_space(6.0);
            let done = words >= goal;
            let bar = egui::ProgressBar::new((words as f32 / goal as f32).min(1.0))
                .text(RichText::new(format!("{words} / {goal} words{}", if done { "  ✓" } else { "" })).color(Color32::WHITE))
                .fill(if done { Color32::from_rgb(22, 163, 74) } else { BLUE })
                .desired_width(f32::INFINITY);
            ui.add(bar);
        } else if words > 0 {
            ui.add_space(4.0);
            ui.label(RichText::new(format!("{words} words")).small().color(p.muted));
        }
        if self.status.starts_with("Save failed") {
            ui.add_space(4.0);
            ui.label(RichText::new(&self.status).color(Color32::from_rgb(239, 68, 68)));
        } else if !self.status.is_empty() && self.status != "Saved" {
            ui.add_space(4.0);
            ui.label(RichText::new(&self.status).small().color(p.muted));
        }
        ui.add_space(8.0);
        ui.separator();

        let body = ui.style().text_styles[&egui::TextStyle::Body].size;
        let mut edit = None;
        let mut delete = None;
        let mut jump = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            if entries.is_empty() {
                ui.add_space(8.0);
                ui.label("No entries for this day yet.");
            }
            for e in &entries {
                let c = category_color(&self.data.categories, &e.category);
                let fg = text_on(c);
                egui::Frame::new()
                    .fill(c)
                    .corner_radius(10)
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&e.category).strong().color(fg));
                            if let Some(m) = e.mood {
                                mood_badge(ui, m);
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let time = e.added_at.get(11..16).unwrap_or("");
                                ui.label(RichText::new(time).small().color(fg));
                            });
                        });
                        ui.add_space(4.0);
                        let style = markdown::Style { text: fg, link: fg, size: body };
                        markdown::render(ui, &e.text, &style, &mut self.media, &self.data.images);
                        ui.add_space(6.0);
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

            // What was written on this date in earlier years.
            if !memories.is_empty() {
                ui.add_space(10.0);
                ui.separator();
                ui.label(RichText::new("On this day").size(18.0).strong().color(p.title));
                ui.add_space(4.0);
                for m in memories.iter().take(12) {
                    let c = category_color(&self.data.categories, &m.category);
                    let fg = text_on(c);
                    let card = egui::Frame::new().fill(c).corner_radius(10).inner_margin(8).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(m.date.get(0..4).unwrap_or("")).strong().color(fg));
                            if let Some(mood) = m.mood {
                                mood_badge(ui, mood);
                            }
                            ui.label(RichText::new(m.added_at.get(11..16).unwrap_or("")).small().color(fg));
                        });
                        let mut s = markdown::summary(&m.text);
                        if s.chars().count() > 100 {
                            s = s.chars().take(100).collect::<String>() + "…";
                        }
                        ui.label(RichText::new(s).color(fg));
                    });
                    let resp = ui.interact(card.response.rect, ui.id().with(("memory", m.id)), Sense::click());
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        jump = NaiveDate::parse_from_str(&m.date, "%Y-%m-%d").ok();
                    }
                    ui.add_space(4.0);
                }
            }
        });

        if let Some(id) = edit {
            self.start_edit(id);
        }
        if let Some(id) = delete {
            self.data.entries.retain(|e| e.id != id);
            self.persist();
        }
        if let Some(d) = jump {
            self.selected = d;
            self.month = d.with_day(1).unwrap();
            self.day_scroll_pending = true;
        }
    }

    /// Replaces the day panel while something is typed in the search box.
    fn search_results(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let q = self.search.trim().to_lowercase();
        let mut hits: Vec<&Entry> = self
            .data
            .entries
            .iter()
            .filter(|e| e.text.to_lowercase().contains(&q) || e.category.to_lowercase().contains(&q) || e.date.contains(&q))
            .collect();
        hits.sort_by(|a, b| b.added_at.cmp(&a.added_at));

        ui.label(RichText::new("Search results").size(22.0).strong().color(p.title));
        ui.label(RichText::new(format!("{} found. Click one to open its day.", hits.len())).color(p.muted));
        ui.add_space(6.0);
        ui.separator();

        let mut jump = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for e in hits.iter().take(100) {
                let c = category_color(&self.data.categories, &e.category);
                let fg = text_on(c);
                let card = egui::Frame::new().fill(c).corner_radius(10).inner_margin(10).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&e.date).strong().color(fg));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(&e.category).small().color(fg));
                        });
                    });
                    let mut s = markdown::summary(&e.text);
                    if s.chars().count() > 110 {
                        s = s.chars().take(110).collect::<String>() + "…";
                    }
                    ui.label(RichText::new(s).color(fg));
                });
                let resp = ui.interact(card.response.rect, ui.id().with(("hit", e.id)), Sense::click());
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    jump = NaiveDate::parse_from_str(&e.date, "%Y-%m-%d").ok();
                }
                ui.add_space(6.0);
            }
        });
        if let Some(d) = jump {
            self.selected = d;
            self.month = d.with_day(1).unwrap();
            self.search.clear();
        }
    }

    fn entry_window(&mut self, ctx: &egui::Context) {
        if self.editor.is_none() {
            return;
        }
        let p = palette(ctx);
        let dark = ctx.theme() == egui::Theme::Dark;
        let DiaryApp { editor, data, media, show_marks, editor_msg, save_requested, .. } = self;
        let ed = editor.as_mut().unwrap();
        let mut save_it = false;
        let mut cancel = false;
        if std::mem::take(save_requested) && !ed.text.trim().is_empty() {
            save_it = true;
        }

        // Keyboard shortcuts for the common formats.
        let shortcuts = [(egui::Key::B, markdown::Format::Bold), (egui::Key::I, markdown::Format::Italic)];
        for (key, format) in shortcuts {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, key)) {
                apply_format(ctx, ed, format);
            }
        }
        // Images dropped onto the window.
        for file in ctx.input(|i| i.raw.dropped_files.clone()) {
            match import_image(data, file.path()) {
                Ok(md) => insert_image(ctx, ed, &md),
                Err(e) => *editor_msg = e,
            }
        }

        egui::Window::new(if ed.id.is_some() { "Edit entry" } else { "New entry" })
            .title_bar(false)
            .frame(popup_frame(&p, dark))
            .resizable(true)
            .default_size([940.0, 620.0])
            .min_size([560.0, 380.0])
            .pivot(Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                if popup_header(ui, &p, if ed.id.is_some() { "Edit entry" } else { "New entry" }) {
                    cancel = true;
                }
                egui::Frame::new().inner_margin(14).show(ui, |ui| {
                    // Day, time and category.
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Date:");
                        let mut picked = jiff::civil::date(ed.date.year() as i16, ed.date.month() as i8, ed.date.day() as i8);
                        ui.add(egui_extras::DatePickerButton::new(&mut picked).id_salt("entry_date").show_icon(false));
                        if let Some(d) = NaiveDate::from_ymd_opt(picked.year() as i32, picked.month() as u32, picked.day() as u32) {
                            ed.date = d;
                        }
                        ui.label(RichText::new(ed.date.format("%A").to_string()).color(p.muted));
                        ui.add_space(10.0);
                        ui.label("Time:");
                        let two = |n: f64, _: std::ops::RangeInclusive<usize>| format!("{:02}", n as u32);
                        ui.add(egui::DragValue::new(&mut ed.hour).range(0..=23).custom_formatter(two));
                        ui.label(":");
                        ui.add(egui::DragValue::new(&mut ed.minute).range(0..=59).custom_formatter(two));
                        ui.add_space(10.0);
                        ui.label("Category:");
                        let cats = &data.categories;
                        let sel_color = category_color(cats, &ed.category);
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
                        ui.add_space(10.0);
                        ui.label("Mood:");
                        for m in 1..=5u8 {
                            let (r, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
                            let col = mood_color(f32::from(m));
                            let on = ed.mood == Some(m);
                            ui.painter().circle_filled(r.center(), 11.0, if on { col } else { col.gamma_multiply(0.4) });
                            if on {
                                ui.painter().circle_stroke(r.center(), 12.5, Stroke::new(2.0, p.ink));
                            }
                            ui.painter().text(
                                r.center(),
                                Align2::CENTER_CENTER,
                                m.to_string(),
                                FontId::proportional(13.0),
                                if on { text_on(col) } else { p.ink },
                            );
                            let resp = resp.on_hover_text(format!("{} (click again to clear)", MOOD_LABELS[usize::from(m) - 1]));
                            if resp.clicked() {
                                ed.mood = if on { None } else { Some(m) };
                            }
                        }
                    });
                    ui.add_space(6.0);

                    // Formatting toolbar.
                    ui.horizontal_wrapped(|ui| {
                        use markdown::Format::*;
                        let buttons: [(RichText, &str, markdown::Format); 10] = [
                            (RichText::new("B").strong().color(Color32::WHITE), "Bold (Ctrl+B)", Bold),
                            (RichText::new("I").italics().color(Color32::WHITE), "Italic (Ctrl+I)", Italic),
                            (RichText::new("S").strikethrough().color(Color32::WHITE), "Strikethrough", Strike),
                            (RichText::new("Code").monospace().color(Color32::WHITE), "Code", Code),
                            (RichText::new("H1").color(Color32::WHITE), "Large heading", Heading(1)),
                            (RichText::new("H2").color(Color32::WHITE), "Medium heading", Heading(2)),
                            (RichText::new("H3").color(Color32::WHITE), "Small heading", Heading(3)),
                            (RichText::new("• List").color(Color32::WHITE), "Bulleted list", Bullet),
                            (RichText::new("1. List").color(Color32::WHITE), "Numbered list", Numbered),
                            (RichText::new("Quote").color(Color32::WHITE), "Quote", Quote),
                        ];
                        for (label, tip, format) in buttons {
                            if ui.button(label).on_hover_text(tip).clicked() {
                                apply_format(ctx, ed, format);
                            }
                        }
                        ui.add_space(8.0);
                        if ui.button("🖼 Image…").on_hover_text("Add a picture (or drop one onto this window)").clicked() {
                            let picked = rfd::FileDialog::new()
                                .add_filter("Images", &["png", "jpg", "jpeg", "gif", "bmp", "webp"])
                                .pick_file();
                            if let Some(path) = picked {
                                match import_image(data, &path) {
                                    Ok(md) => {
                                        insert_image(ctx, ed, &md);
                                        editor_msg.clear();
                                    }
                                    Err(e) => *editor_msg = e,
                                }
                            }
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.checkbox(show_marks, "Show formatting marks");
                        });
                    });
                    if !editor_msg.is_empty() {
                        ui.label(RichText::new(editor_msg.as_str()).color(Color32::from_rgb(239, 68, 68)));
                    }
                    ui.add_space(6.0);

                    // The page: styled text blocks, with pictures shown in between.
                    let height = (ui.available_height() - 52.0).max(120.0);
                    let body = ui.style().text_styles[&egui::TextStyle::Body].size;
                    let st = live::LiveStyle { ink: p.ink, muted: p.muted, accent: p.accent, size: body };
                    let marks = *show_marks;
                    egui::Frame::new().fill(p.cell).stroke(Stroke::new(1.0, p.cell_border)).inner_margin(10).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        egui::ScrollArea::vertical().id_salt("edit_scroll").auto_shrink([false, false]).max_height(height - 24.0).show(ui, |ui| {
                            let mut segs = live::split(&ed.text);
                            let count = segs.len();
                            let is_text: Vec<bool> = segs.iter().map(|s| matches!(s, live::Seg::Text(_))).collect();
                            let mut changed = false;
                            let mut remove = None;
                            for i in 0..count {
                                match &mut segs[i] {
                                    live::Seg::Text(s) => {
                                        let id = seg_id(i);
                                        let want = match ed.want_focus {
                                            Some((w, pos)) if w == i || (w == usize::MAX && i == count - 1) => Some(pos),
                                            _ => None,
                                        };
                                        if let Some(pos) = want {
                                            focus_at(ctx, id, s, pos);
                                            ed.want_focus = None;
                                        }
                                        // Marks show on the lines the cursor or selection touches.
                                        let reveal = egui::TextEdit::load_state(ctx, id).and_then(|state| state.cursor.char_range()).map(|r| {
                                            let (a, b) = (usize::from(r.primary.index), usize::from(r.secondary.index));
                                            (a.min(b), a.max(b))
                                        });
                                        let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap: f32| {
                                            let job = live::layout(text.as_str(), reveal, marks, &st, wrap);
                                            ui.fonts_mut(|f| f.layout_job(job))
                                        };
                                        let last = i == count - 1;
                                        let resp = ui.add(
                                            egui::TextEdit::multiline(s)
                                                .id(id)
                                                .frame(egui::Frame::NONE)
                                                .hint_text(if count == 1 { "What's on your mind?" } else { "" })
                                                .desired_width(f32::INFINITY)
                                                .desired_rows(if last { 8 } else { 1 })
                                                .layouter(&mut layouter),
                                        );
                                        changed |= resp.changed();
                                        if resp.has_focus() {
                                            ed.focus = i;
                                            // Let the arrow keys travel past pictures into the next text block.
                                            let (down, up) = ctx.input(|inp| (inp.key_pressed(egui::Key::ArrowDown), inp.key_pressed(egui::Key::ArrowUp)));
                                            if down || up {
                                                let cursor = egui::TextEdit::load_state(ctx, id)
                                                    .and_then(|state| state.cursor.char_range())
                                                    .map_or(0, |r| usize::from(r.primary.index));
                                                let first_line = s.chars().take_while(|c| *c != '\n').count();
                                                if down && cursor >= s.chars().count() {
                                                    if let Some(j) = (i + 1..count).find(|&j| is_text[j]) {
                                                        ed.want_focus = Some((j, 0));
                                                    }
                                                }
                                                if up && cursor <= first_line {
                                                    if let Some(j) = (0..i).rev().find(|&j| is_text[j]) {
                                                        ed.want_focus = Some((j, usize::MAX));
                                                        ctx.request_repaint();
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    live::Seg::Image { id, alt, .. } => {
                                        ui.add_space(4.0);
                                        let src = format!("{}{id}", markdown::IMAGE_SCHEME);
                                        markdown::show_image(ui, media, &data.images, &src, alt, p.ink);
                                        let del = egui::Button::new(RichText::new("Remove picture").small().color(Color32::WHITE)).fill(RED);
                                        if ui.add(del).clicked() {
                                            remove = Some(i);
                                        }
                                        ui.add_space(4.0);
                                    }
                                }
                            }
                            if let Some(i) = remove {
                                // Cursor goes to where the two text blocks around the picture meet.
                                let join_at = match &segs[i - 1] {
                                    live::Seg::Text(t) => t.chars().count(),
                                    _ => 0,
                                };
                                segs.remove(i);
                                ed.want_focus = Some((i - 1, join_at));
                                ctx.request_repaint();
                                changed = true;
                            }
                            if changed {
                                ed.text = live::join(&segs);
                            }
                        });
                    });

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.add_enabled(!ed.text.trim().is_empty(), egui::Button::new("Save")).clicked() {
                            save_it = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                        ui.add_space(12.0);
                        let draft = markdown::word_count(&ed.text);
                        ui.label(RichText::new(format!("{draft} {}", if draft == 1 { "word" } else { "words" })).color(p.muted));
                        let goal = data.settings.daily_word_goal as usize;
                        if goal > 0 {
                            // Progress for the whole day: this draft plus the day's other entries.
                            let day = key(ed.date);
                            let others: usize = data
                                .entries
                                .iter()
                                .filter(|e| e.date == day && Some(e.id) != ed.id)
                                .map(|e| markdown::word_count(&e.text))
                                .sum();
                            let total = others + draft;
                            let done = total >= goal;
                            ui.add(
                                egui::ProgressBar::new((total as f32 / goal as f32).min(1.0))
                                    .text(RichText::new(format!("day: {total} / {goal}{}", if done { "  ✓" } else { "" })).color(Color32::WHITE))
                                    .fill(if done { Color32::from_rgb(22, 163, 74) } else { BLUE })
                                    .desired_width(210.0),
                            );
                        }
                    });
                });
            });

        if save_it {
            let ed = self.editor.take().unwrap();
            let text = ed.text.trim().to_string();
            let added_at = format!("{} {:02}:{:02}:{:02}", key(ed.date), ed.hour, ed.minute, ed.second);
            match ed.id {
                Some(id) => {
                    if let Some(e) = self.data.entries.iter_mut().find(|e| e.id == id) {
                        e.text = text;
                        e.category = ed.category;
                        e.date = key(ed.date);
                        e.added_at = added_at;
                        e.mood = ed.mood;
                    }
                }
                None => {
                    let id = self.data.entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
                    self.data.entries.push(Entry { id, date: key(ed.date), added_at, category: ed.category, text, mood: ed.mood });
                }
            }
            self.selected = ed.date;
            self.month = ed.date.with_day(1).unwrap();
            self.editor_msg.clear();
            self.persist();
        } else if cancel {
            self.editor = None;
            self.editor_msg.clear();

        }
    }


    fn categories_window(&mut self, ctx: &egui::Context) {
        let p = palette(ctx);
        if !self.show_categories {
            return;
        }
        let mut open = true;
        let dark = ctx.theme() == egui::Theme::Dark;
        let mut changed = false;
        let mut remove = None;
        let mut renames: Vec<(String, String)> = Vec::new();

        egui::Window::new("Categories")
            .title_bar(false)
            .frame(popup_frame(&p, dark))
            .resizable(true)
            .default_size([520.0, 420.0])
            .min_size([360.0, 260.0])
            .pivot(Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                if popup_header(ui, &p, "Categories") {
                    open = false;
                }
                egui::Frame::new().inner_margin(14).show(ui, |ui| {
                let can_delete = self.data.categories.len() > 1;
                let names: Vec<String> = self.data.categories.iter().map(|c| c.name.clone()).collect();
                let big = FontId::proportional(18.0);
                let row_h = 36.0;
                let field_margin = egui::Margin::symmetric(10, 0);
                for (i, c) in self.data.categories.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().interact_size = vec2(48.0, row_h);
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
                                    .min_size(vec2(0.0, row_h))
                                    .vertical_align(egui::Align::Center)
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
                    ui.spacing_mut().interact_size = vec2(48.0, row_h);
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
                                .min_size(vec2(0.0, row_h))
                                .vertical_align(egui::Align::Center)
                                .hint_text("Name")
                                .text_color(p.ink)
                                .desired_width(ui.available_width()),
                        );
                    });
                });
                ui.weak("Entries in a deleted category show in grey.");
                });
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
    v.window_shadow = popup_shadow(dark);
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

/// Use Segoe UI (with a real bold) when it is installed; otherwise fall back to egui's fonts.
fn setup_fonts(ctx: &egui::Context) {
    use egui::{FontData, FontFamily};
    let mut fonts = egui::FontDefinitions::default();
    let dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:/Windows".into());
    let read = |file: &str| std::fs::read(std::path::Path::new(&dir).join("Fonts").join(file)).ok();

    if let Some(bytes) = read("segoeui.ttf") {
        fonts.font_data.insert("segoe".into(), std::sync::Arc::new(FontData::from_owned(bytes)));
        fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "segoe".into());
    }
    let mut bold = Vec::new();
    if let Some(bytes) = read("segoeuib.ttf") {
        fonts.font_data.insert("segoe-bold".into(), std::sync::Arc::new(FontData::from_owned(bytes)));
        bold.push("segoe-bold".to_string());
    }
    bold.extend(fonts.families[&FontFamily::Proportional].clone());
    fonts.families.insert(markdown::bold_family(), bold);
    ctx.set_fonts(fonts);
}

fn setup_style(ctx: &egui::Context) {
    setup_fonts(ctx);
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
        // Auto-lock after a period without any input.
        if self.lock.is_none() {
            if ctx.input(|i| !i.events.is_empty()) {
                self.last_activity = std::time::Instant::now();
            }
            let minutes = self.data.settings.auto_lock_minutes;
            if minutes > 0 {
                let limit = std::time::Duration::from_secs(u64::from(minutes) * 60);
                let idle = self.last_activity.elapsed();
                if idle >= limit {
                    self.lock_now();
                } else {
                    ctx.request_repaint_after(limit - idle + std::time::Duration::from_millis(250));
                }
            }
        }
        self.handle_shortcuts(&ctx);
        if self.lock.is_some() {
            egui::Panel::top("header")
                .frame(egui::Frame::new().fill(p.header).inner_margin(egui::Margin::symmetric(14, 10)))
                .show(ui, |ui| {
                    ui.label(RichText::new("My Diary").size(24.0).strong().color(Color32::WHITE));
                });
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(p.page_bg))
                .show(ui, |ui| self.lock_screen(ui));
            return;
        }
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
        self.password_window(&ctx);
        self.location_window(&ctx);
        self.shortcuts_window(&ctx);
        self.export_window(&ctx);
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_icon(std::sync::Arc::new(egui::IconData { rgba: icon::rgba(256), width: 256, height: 256 })).with_inner_size([1150.0, 720.0]).with_min_inner_size([1000.0, 560.0]),
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

fn popup_shadow(dark: bool) -> egui::Shadow {
    egui::Shadow { offset: [0, 10], blur: 32, spread: 4, color: Color32::from_black_alpha(if dark { 220 } else { 150 }) }
}

/// Window frame for popups: same panel colour as the main window, square corners, dark shadow.
fn popup_frame(p: &Palette, dark: bool) -> egui::Frame {
    egui::Frame::new()
        .fill(p.panel_bg)
        .stroke(Stroke::new(1.0, p.cell_border))
        .shadow(popup_shadow(dark))
}

/// Blue title band matching the main window's header. Returns true when the close button is clicked.
fn popup_header(ui: &mut egui::Ui, p: &Palette, title: &str) -> bool {
    let mut close = false;
    egui::Frame::new().fill(p.header).inner_margin(egui::Margin::symmetric(14, 10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).size(22.0).strong().color(Color32::WHITE));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Painted cross: the "✕" glyph isn't in egui's default font and shows up as a box.
                let (rect, resp) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
                let fill = if resp.hovered() { RED } else { BLUE };
                ui.painter().rect_filled(rect, 8.0, fill);
                let s = Stroke::new(2.5, Color32::WHITE);
                let r = rect.shrink(11.0);
                ui.painter().line_segment([r.left_top(), r.right_bottom()], s);
                ui.painter().line_segment([r.left_bottom(), r.right_top()], s);
                if resp.on_hover_text("Close").clicked() {
                    close = true;
                }
            });
        });
    });
    close
}

/// Derive a key for a new password, write the encrypted file, and return the unlocked vault.
fn create_vault(password: &str, confirm: &str, data: Data) -> Result<(vault::Vault, Data), String> {
    check_new_password(password, confirm)?;
    let v = vault::Vault::create(password)?;
    save(&data, &v)?;
    Ok((v, data))
}

fn password_field<'a>(text: &'a mut String, hint: &str, p: &Palette) -> egui::TextEdit<'a> {
    egui::TextEdit::singleline(text)
        .password(true)
        .hint_text(hint)
        .font(FontId::proportional(18.0))
        .margin(egui::Margin::symmetric(12, 8))
        .text_color(p.ink)
        .desired_width(f32::INFINITY)
}

fn pressed_enter(r: &egui::Response, ui: &egui::Ui) -> bool {
    r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
}

fn category_color(cats: &[Category], name: &str) -> Color32 {
    rgb(cats.iter().find(|c| c.name == name).map_or(GREY, |c| c.color))
}

/// Apply a toolbar format to the current selection of the entry text box.
fn format_selection(ctx: &egui::Context, id: egui::Id, text: &mut String, format: markdown::Format) {
    use egui::text::{CCursor, CCursorRange};
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    let end = text.chars().count();
    let (a, b) = state
        .cursor
        .char_range()
        .map_or((end, end), |r| (usize::from(r.primary.index), usize::from(r.secondary.index)));
    let (s, e) = markdown::apply(text, (a.min(b), a.max(b)), format);
    state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(s), CCursor::new(e))));
    state.store(ctx, id);
    ctx.memory_mut(|m| m.request_focus(id));
}

const MAX_IMAGE_SIDE: u32 = 1600;

/// Read an image file, shrink it if it is large, store it in the diary data and return the
/// Markdown that shows it.
fn import_image(data: &mut Data, path: &std::path::Path) -> Result<String, String> {
    use base64::Engine;
    let bytes = std::fs::read(path).map_err(|e| format!("Couldn't read {}: {e}", path.display()))?;
    let mut img = image::load_from_memory(&bytes).map_err(|e| format!("Couldn't open that image: {e}"))?;
    if img.width() > MAX_IMAGE_SIDE || img.height() > MAX_IMAGE_SIDE {
        img = img.resize(MAX_IMAGE_SIDE, MAX_IMAGE_SIDE, image::imageops::FilterType::Lanczos3);
    }
    // Keep transparency as PNG; photos and everything else as a compact JPEG.
    let transparent = img.color().has_alpha() && img.to_rgba8().pixels().any(|p| p[3] < 255);
    let mut out = Vec::new();
    if transparent {
        img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).map_err(|e| e.to_string())?;
    } else {
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85);
        img.to_rgb8().write_with_encoder(enc).map_err(|e| e.to_string())?;
    }

    let mut id = format!("i{}", chrono::Utc::now().timestamp_millis());
    while data.images.contains_key(&id) {
        id.push('x');
    }
    data.images.insert(id.clone(), base64::engine::general_purpose::STANDARD.encode(out));
    let name = path.file_stem().map(|s| s.to_string_lossy().replace(['[', ']', '(', ')'], "")).unwrap_or_default();
    Ok(format!("![{name}]({}{id})", markdown::IMAGE_SCHEME))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_png(path: &std::path::Path, alpha: u8) {
        let img = image::RgbaImage::from_pixel(8, 6, image::Rgba([200, 30, 30, alpha]));
        img.save(path).unwrap();
    }

    #[test]
    fn imports_images_and_drops_unused_ones() {
        let dir = std::env::temp_dir().join(format!("diary-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (opaque, clear) = (dir.join("my (photo).png"), dir.join("clear.png"));
        tiny_png(&opaque, 255);
        tiny_png(&clear, 100);

        let mut data = fresh_data();
        let md1 = import_image(&mut data, &opaque).unwrap();
        let md2 = import_image(&mut data, &clear).unwrap();
        assert!(md1.starts_with("![my photo](img:i"), "{md1}");
        assert_eq!(data.images.len(), 2);
        assert_eq!(markdown::image_ids(&md1).len(), 1);
        assert!(import_image(&mut data, &dir.join("missing.png")).is_err());
        assert!(md2.contains("img:"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn renders_rich_text_with_an_image_without_panicking() {
        let dir = std::env::temp_dir().join(format!("diary-render-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        tiny_png(&dir.join("a.png"), 255);
        let mut data = fresh_data();
        let img = import_image(&mut data, &dir.join("a.png")).unwrap();
        std::fs::remove_dir_all(&dir).ok();

        let md = format!(
            "# Title\n\nSome **bold**, *italic*, ~~gone~~ and `code`.\n\n- one\n- two\n\n1. a\n2. b\n\n> quoted\n\n{img}\n\n```\nlet x = 1;\n```\n\n---\n"
        );
        let ctx = egui::Context::default();
        setup_style(&ctx);
        let mut media = markdown::Media::default();
        // The second frame is the first one that uses the fonts set up by `setup_style`.
        for _ in 0..3 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                let style = markdown::Style { text: Color32::BLACK, link: Color32::BLUE, size: 16.0 };
                markdown::render(ui, &md, &style, &mut media, &data.images);
            });
            out.textures_delta.clear();
        }
    }

    fn editor_state(text: &str) -> EditState {
        EditState {
            id: None,
            date: NaiveDate::from_ymd_opt(2026, 10, 9).unwrap(),
            hour: 9,
            minute: 30,
            second: 0,
            category: "Personal".into(),
            text: text.into(),
            mood: None,
            focus: 0,
            want_focus: None,
        }
    }

    fn set_cursor(ctx: &egui::Context, id: egui::Id, a: usize, b: usize) {
        use egui::text::{CCursor, CCursorRange};
        let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(a), CCursor::new(b))));
        state.store(ctx, id);
    }

    #[test]
    fn toolbar_formats_the_selection_of_the_focused_block() {
        let ctx = egui::Context::default();
        let mut ed = editor_state("hello world");
        set_cursor(&ctx, seg_id(0), 6, 11);
        apply_format(&ctx, &mut ed, markdown::Format::Bold);
        assert_eq!(ed.text, "hello **world**");
        // The selection now covers the word, so applying it again removes the format.
        apply_format(&ctx, &mut ed, markdown::Format::Bold);
        assert_eq!(ed.text, "hello world");
    }

    #[test]
    fn pictures_are_inserted_at_the_cursor_between_text_blocks() {
        let ctx = egui::Context::default();
        let mut ed = editor_state("hello world");
        set_cursor(&ctx, seg_id(0), 5, 5);
        insert_image(&ctx, &mut ed, "![p](img:i9)");
        assert_eq!(ed.text, "hello\n![p](img:i9)\n world");
        assert_eq!(ed.want_focus, Some((2, 0)));
        let segs = live::split(&ed.text);
        assert_eq!(segs.len(), 3);
        assert!(matches!(&segs[1], live::Seg::Image { id, .. } if id == "i9"));
    }

    #[test]
    fn live_layout_works_inside_a_text_edit() {
        let ctx = egui::Context::default();
        setup_style(&ctx);
        let st = live::LiveStyle { ink: Color32::BLACK, muted: Color32::GRAY, accent: Color32::BLUE, size: 16.0 };
        let mut text = "# Title **b** `c`\n- item *i*\n\nplain".to_string();
        for _ in 0..3 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                let mut layouter = |ui: &egui::Ui, t: &dyn egui::TextBuffer, wrap: f32| {
                    ui.fonts_mut(|f| f.layout_job(live::layout(t.as_str(), None, false, &st, wrap)))
                };
                let shown = egui::TextEdit::multiline(&mut text).layouter(&mut layouter).show(ui);
                assert_eq!(shown.galley.text(), "# Title **b** `c`\n- item *i*\n\nplain");
            });
            out.textures_delta.clear();
        }
    }

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn week_helpers() {
        // 9 October 2026 is a Friday.
        assert_eq!(week_start(d(2026, 10, 9)), d(2026, 10, 5));
        assert_eq!(week_start(d(2026, 10, 5)), d(2026, 10, 5));
        assert_eq!(week_start(d(2026, 10, 11)), d(2026, 10, 5));
        assert_eq!(week_title(d(2026, 10, 9)), "5 – 11 October 2026");
        assert_eq!(week_title(d(2026, 9, 30)), "28 Sep – 4 Oct 2026");
        assert_eq!(week_title(d(2026, 12, 31)), "28 Dec 2026 – 3 Jan 2027");
    }

    #[test]
    fn moving_an_entry_keeps_its_time_unless_an_hour_is_given() {
        assert_eq!(entry_time("2026-10-08 16:11:27"), Some((16, 11, 27)));
        assert_eq!(entry_time("garbage"), None);
        assert_eq!(moved_timestamp("2026-10-08 16:11:27", d(2026, 10, 12), None), "2026-10-12 16:11:27");
        assert_eq!(moved_timestamp("2026-10-08 16:11:27", d(2026, 10, 8), Some(9)), "2026-10-08 09:11:27");
        assert_eq!(moved_timestamp("bad", d(2026, 1, 2), None), "2026-01-02 00:00:00");
    }

    fn app_with_entries() -> DiaryApp {
        let mut app = DiaryApp::new();
        app.lock = None;
        app.data = fresh_data();
        app.selected = d(2026, 10, 9);
        app.month = d(2026, 10, 1);
        for (id, date, time, text) in [
            (1, "2026-10-09", "09:15:00", "# Morning **run**"),
            (2, "2026-10-09", "09:40:30", "Coffee with Sam"),
            (3, "2026-10-09", "21:05:00", "Evening notes\n\nmore"),
            (4, "2026-10-12", "12:00:00", "Next week"),
        ] {
            app.data.entries.push(Entry {
                id,
                date: date.into(),
                added_at: format!("{date} {time}"),
                category: "Work".into(),
                text: text.into(),
                mood: None,
            });
        }
        app.data.entries[0].mood = Some(4);
        app.data.entries[2].mood = Some(2);
        app.data.settings.daily_word_goal = 5;
        app
    }

    #[test]
    fn entries_move_between_days_and_hours() {
        let mut app = app_with_entries();
        app.move_entry(1, d(2026, 10, 14), None);
        let e = app.data.entries.iter().find(|e| e.id == 1).unwrap();
        assert_eq!((e.date.as_str(), e.added_at.as_str()), ("2026-10-14", "2026-10-14 09:15:00"));
        assert_eq!(app.selected, d(2026, 10, 14));

        app.move_entry(2, d(2026, 10, 9), Some(17));
        let e = app.data.entries.iter().find(|e| e.id == 2).unwrap();
        assert_eq!((e.date.as_str(), e.added_at.as_str()), ("2026-10-09", "2026-10-09 17:40:30"));
        assert!(app.day_entries(d(2026, 10, 9)).iter().any(|e| e.id == 2));
        assert!(app.day_entries(d(2026, 10, 14)).iter().any(|e| e.id == 1));
    }


    #[test]
    fn dragging_a_pill_to_another_day_moves_the_entry() {
        let ctx = egui::Context::default();
        setup_style(&ctx);
        let mut app = app_with_entries();
        app.view = CalView::Month;
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(1200.0, 800.0));
        let (mut time, root) = (0.0f64, std::cell::Cell::new(None));
        let mut frame = |app: &mut DiaryApp, events: Vec<egui::Event>| {
            time += 0.1;
            let input = egui::RawInput { screen_rect: Some(screen), time: Some(time), events, ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                root.set(Some(ui.id()));
                app.calendar(ui);
            });
            out.textures_delta.clear();
        };
        frame(&mut app, vec![]);
        frame(&mut app, vec![]);

        let root = root.get().unwrap();
        let center = |ctx: &egui::Context, id: egui::Id| ctx.read_response(id).expect("widget was laid out").rect.center();
        let from = center(&ctx, root.with(("pill", 1u64)));
        let to = center(&ctx, root.with(("cell", d(2026, 10, 14))));
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };

        frame(&mut app, vec![egui::Event::PointerMoved(from)]);
        frame(&mut app, vec![button(from, true)]);
        frame(&mut app, vec![egui::Event::PointerMoved(from + vec2(30.0, 30.0))]);
        assert_eq!(app.drag, Some(1), "the drag should have started");
        frame(&mut app, vec![egui::Event::PointerMoved(to)]);
        frame(&mut app, vec![button(to, false)]);
        frame(&mut app, vec![]);

        assert_eq!(app.drag, None);
        let e = app.data.entries.iter().find(|e| e.id == 1).unwrap();
        assert_eq!(e.date, "2026-10-14");
        assert_eq!(e.added_at, "2026-10-14 09:15:00");
    }

    fn press(app: &mut DiaryApp, key: egui::Key, modifiers: egui::Modifiers) {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            events: vec![egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }],
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| app.handle_shortcuts(ui.ctx()));
        out.textures_delta.clear();
    }

    #[test]
    fn keyboard_shortcuts() {
        let ctrl = egui::Modifiers::COMMAND;
        let mut app = app_with_entries();
        app.view = CalView::Month;

        press(&mut app, egui::Key::Num2, ctrl);
        assert_eq!(app.view, CalView::Week);
        press(&mut app, egui::Key::Num4, ctrl);
        assert_eq!(app.view, CalView::Year);
        press(&mut app, egui::Key::Num1, ctrl);
        assert_eq!(app.view, CalView::Day);

        let before = app.selected;
        press(&mut app, egui::Key::ArrowRight, egui::Modifiers::NONE);
        assert_eq!(app.selected, before + chrono::Days::new(1));
        press(&mut app, egui::Key::ArrowLeft, egui::Modifiers::NONE);
        assert_eq!(app.selected, before);

        press(&mut app, egui::Key::T, ctrl);
        assert_eq!(app.selected, Local::now().date_naive());

        assert!(app.editor.is_none());
        press(&mut app, egui::Key::N, ctrl);
        assert!(app.editor.is_some(), "Ctrl+N starts a new entry");
        // While writing, Ctrl+S asks for a save and Ctrl+N does not start another entry.
        app.editor.as_mut().unwrap().text = "hello".into();
        press(&mut app, egui::Key::S, ctrl);
        assert!(app.save_requested);

        press(&mut app, egui::Key::L, ctrl);
        assert!(app.lock.is_some(), "Ctrl+L locks");
    }

    #[test]
    fn export_ranges_and_counts() {
        let app = app_with_entries(); // selected 2026-10-09 (a Friday); entries on the 9th (x3) and 12th
        let (from, to, _, slug) = app.export_range(ExportScope::Month);
        assert_eq!((from, to, slug.as_str()), (d(2026, 10, 1), d(2026, 10, 31), "2026-10"));
        assert_eq!(app.entries_in(from, to).len(), 4);
        let (from, to, _, slug) = app.export_range(ExportScope::Week);
        assert_eq!((from, to, slug.as_str()), (d(2026, 10, 5), d(2026, 10, 11), "week-2026-10-05"));
        assert_eq!(app.entries_in(from, to).len(), 3);
        let (from, to, ..) = app.export_range(ExportScope::Day);
        assert_eq!(app.entries_in(from, to).len(), 3);
        let (from, to, ..) = app.export_range(ExportScope::Year);
        assert_eq!((from, to), (d(2026, 1, 1), d(2026, 12, 31)));
        let (from, to, ..) = app.export_range(ExportScope::All);
        assert_eq!(app.entries_in(from, to).len(), 4);
        // And the real PDF builder accepts what the window would give it.
        let entries = app.entries_in(d(2026, 10, 1), d(2026, 10, 31));
        let report = export::Report { title: "T".into(), subtitle: "S".into(), entries, include_images: true };
        assert!(export::build_pdf(&report, &app.data.categories, &app.data.images).unwrap().starts_with(b"%PDF"));
    }
    #[test]
    fn all_calendar_views_render() {
        let ctx = egui::Context::default();
        setup_style(&ctx);
        let mut app = app_with_entries();
        for view in [CalView::Month, CalView::Week, CalView::Day, CalView::Year, CalView::Month] {
            app.view = view;
            for _ in 0..3 {
                let mut out = ctx.run_ui(egui::RawInput::default(), |ui| app.calendar(ui));
                out.textures_delta.clear();
            }
        }
        // Navigation follows the view.
        app.view = CalView::Week;
        app.shift_days(7);
        assert_eq!(app.selected, d(2026, 10, 16));
        app.view = CalView::Day;
        app.shift_days(-1);
        assert_eq!(app.selected, d(2026, 10, 15));
        assert_eq!(app.calendar_title(), "Thursday 15 October 2026");
    }
}

fn seg_id(i: usize) -> egui::Id {
    egui::Id::new(("entry_seg", i))
}

/// The text block toolbar actions apply to: the one that last had the cursor.
fn focused_text_seg(segs: &[live::Seg], focus: usize) -> usize {
    if matches!(segs.get(focus), Some(live::Seg::Text(_))) {
        focus
    } else {
        segs.iter().rposition(|s| matches!(s, live::Seg::Text(_))).unwrap_or(0)
    }
}

fn apply_format(ctx: &egui::Context, ed: &mut EditState, format: markdown::Format) {
    let mut segs = live::split(&ed.text);
    let idx = focused_text_seg(&segs, ed.focus);
    if let live::Seg::Text(s) = &mut segs[idx] {
        format_selection(ctx, seg_id(idx), s, format);
    }
    ed.text = live::join(&segs);
}

/// Put the cursor at char `pos` (clamped) in a text block and give it keyboard focus.
fn focus_at(ctx: &egui::Context, id: egui::Id, text: &str, pos: usize) {
    use egui::text::{CCursor, CCursorRange};
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(pos.min(text.chars().count())))));
    state.store(ctx, id);
    ctx.memory_mut(|m| m.request_focus(id));
}

/// Insert a picture (`![alt](img:ID)`) at the cursor, splitting the text block around it.
fn insert_image(ctx: &egui::Context, ed: &mut EditState, md: &str) {
    let Some(picture) = live::split(md).into_iter().find(|s| matches!(s, live::Seg::Image { .. })) else {
        return;
    };
    let mut segs = live::split(&ed.text);
    let idx = focused_text_seg(&segs, ed.focus);
    let chars: Vec<char> = match &segs[idx] {
        live::Seg::Text(s) => s.chars().collect(),
        _ => Vec::new(),
    };
    let at = egui::TextEdit::load_state(ctx, seg_id(idx))
        .and_then(|state| state.cursor.char_range())
        .map_or(chars.len(), |r| usize::from(r.primary.index).max(usize::from(r.secondary.index)))
        .min(chars.len());
    let before: String = chars[..at].iter().collect();
    let after: String = chars[at..].iter().collect();
    segs.splice(idx..=idx, [live::Seg::Text(before), picture, live::Seg::Text(after)]);
    ed.text = live::join(&segs);
    ed.want_focus = Some((idx + 2, 0));
}

/// A gear: eight teeth around a disc with a hole in the middle.
fn paint_cog(painter: &egui::Painter, center: egui::Pos2, r: f32, color: Color32, hole: Color32) {
    const TEETH: usize = 8;
    for k in 0..TEETH {
        let (s, c) = (k as f32 * std::f32::consts::TAU / TEETH as f32).sin_cos();
        let (dir, side) = (vec2(c, s), vec2(-s, c) * (r * 0.22));
        let (inner, outer) = (center + dir * (r * 0.6), center + dir * r);
        painter.add(egui::Shape::convex_polygon(
            vec![inner + side, outer + side, outer - side, inner - side],
            color,
            Stroke::NONE,
        ));
    }
    painter.circle_filled(center, r * 0.74, color);
    painter.circle_filled(center, r * 0.32, hole);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CalView {
    Month,
    Week,
    Year,
    Day,
}

/// What the user did on the calendar this frame.
#[derive(Default)]
struct CalEvents {
    select: Option<NaiveDate>,
    /// Double-click on empty space: new entry on this day (and hour, in the day view).
    new_on: Option<(NaiveDate, Option<u32>)>,
    edit: Option<u64>,
    /// An entry was released here; the view works out which day/hour that is.
    drop_at: Option<(u64, egui::Pos2)>,
    /// Move this entry to this day (and hour).
    drop: Option<(u64, NaiveDate, Option<u32>)>,
}

fn week_start(d: NaiveDate) -> NaiveDate {
    d - chrono::Days::new(u64::from(d.weekday().num_days_from_monday()))
}

fn week_title(d: NaiveDate) -> String {
    let (mon, sun) = (week_start(d), week_start(d) + chrono::Days::new(6));
    if mon.month() == sun.month() && mon.year() == sun.year() {
        format!("{} – {} {}", mon.day(), sun.day(), sun.format("%B %Y"))
    } else if mon.year() == sun.year() {
        format!("{} {} – {} {}", mon.day(), mon.format("%b"), sun.day(), sun.format("%b %Y"))
    } else {
        format!("{} {} – {} {}", mon.day(), mon.format("%b %Y"), sun.day(), sun.format("%b %Y"))
    }
}

/// (hour, minute, second) from an `added_at` timestamp.
fn entry_time(added_at: &str) -> Option<(u32, u32, u32)> {
    let mut parts = added_at.get(11..)?.split(':').map(|p| p.parse::<u32>().ok());
    Some((parts.next()??, parts.next()??, parts.next()??))
}

/// The timestamp for an entry moved to `date` (and `hour`, keeping its minutes and seconds).
fn moved_timestamp(added_at: &str, date: NaiveDate, hour: Option<u32>) -> String {
    let (h, m, s) = entry_time(added_at).unwrap_or((0, 0, 0));
    format!("{} {:02}:{:02}:{:02}", key(date), hour.unwrap_or(h).min(23), m, s)
}

/// One entry on the calendar: a coloured pill that can be clicked, double-clicked (edit) or dragged.
#[allow(clippy::too_many_arguments)]
fn pill(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    rect: egui::Rect,
    e: &Entry,
    cats: &[Category],
    size: f32,
    drag: &mut Option<u64>,
    ev: &mut CalEvents,
) {
    let c = category_color(cats, &e.category);
    let resp = ui.interact(rect, ui.id().with(("pill", e.id)), Sense::click_and_drag());
    let being_dragged = *drag == Some(e.id);
    let label = format!("{} {}", e.added_at.get(11..16).unwrap_or(""), markdown::summary(&e.text));
    let fg = text_on(c);

    painter.rect_filled(rect, 9.0, if being_dragged { c.gamma_multiply(0.3) } else { c });
    if resp.hovered() && !being_dragged {
        painter.rect_stroke(rect, 9.0, Stroke::new(1.5, fg), egui::StrokeKind::Inside);
    }
    painter.with_clip_rect(rect.shrink2(vec2(4.0, 0.0))).text(
        egui::pos2(rect.min.x + 7.0, rect.center().y),
        Align2::LEFT_CENTER,
        &label,
        FontId::proportional(size),
        if being_dragged { fg.gamma_multiply(0.5) } else { fg },
    );
    if resp.hovered() || being_dragged {
        ui.ctx().set_cursor_icon(if being_dragged { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
    }

    if resp.drag_started() {
        *drag = Some(e.id);
    }
    if resp.dragged() && *drag == Some(e.id) {
        // A copy of the pill follows the pointer.
        if let Some(pos) = ui.ctx().pointer_latest_pos() {
            let gp = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("drag_ghost")));
            let r = egui::Rect::from_min_size(pos + vec2(12.0, 6.0), vec2(rect.width().min(300.0), rect.height()));
            gp.rect_filled(r.translate(vec2(2.0, 3.0)), 9.0, Color32::from_black_alpha(80));
            gp.rect_filled(r, 9.0, c);
            gp.with_clip_rect(r.shrink2(vec2(4.0, 0.0))).text(
                egui::pos2(r.min.x + 7.0, r.center().y),
                Align2::LEFT_CENTER,
                &label,
                FontId::proportional(size),
                fg,
            );
        }
    }
    if resp.drag_stopped() && drag.take() == Some(e.id) {
        if let Some(pos) = ui.ctx().pointer_latest_pos() {
            ev.drop_at = Some((e.id, pos));
        }
    }
    if resp.double_clicked() {
        ev.edit = Some(e.id);
    } else if resp.clicked() {
        ev.select = NaiveDate::parse_from_str(&e.date, "%Y-%m-%d").ok();
    }
}

/// What the lock screen should ask for, judging by the diary file on disk.
fn current_lock_mode() -> LockMode {
    match inspect() {
        Disk::Missing => LockMode::Create,
        Disk::Encrypted(env) => LockMode::Unlock(env),
        Disk::Plain(data) => LockMode::Encrypt(data),
        Disk::Unreadable(why) => LockMode::Broken(why),
    }
}

const MOOD_LABELS: [&str; 5] = ["Awful", "Low", "Okay", "Good", "Great"];

/// Red (1) through amber to green (5); fractional values blend.
fn mood_color(value: f32) -> Color32 {
    const RAMP: [[f32; 3]; 5] = [[239.0, 68.0, 68.0], [249.0, 115.0, 22.0], [234.0, 179.0, 8.0], [132.0, 204.0, 22.0], [34.0, 197.0, 94.0]];
    let v = (value.clamp(1.0, 5.0) - 1.0).min(3.999);
    let (i, t) = (v.floor() as usize, v.fract());
    let c = |k: usize| RAMP[i][k] + (RAMP[i + 1][k] - RAMP[i][k]) * t;
    Color32::from_rgb(c(0) as u8, c(1) as u8, c(2) as u8)
}

/// A small coloured circle holding the mood number.
fn mood_badge(ui: &mut egui::Ui, mood: u8) {
    let (rect, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
    let col = mood_color(f32::from(mood));
    ui.painter().circle_filled(rect.center(), 9.0, col);
    ui.painter().circle_stroke(rect.center(), 9.0, Stroke::new(1.0, Color32::from_black_alpha(90)));
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, mood.to_string(), FontId::proportional(12.0), text_on(col));
    resp.on_hover_text(format!("Mood: {}", MOOD_LABELS[usize::from(mood.clamp(1, 5)) - 1]));
}

/// Heat-map shade for a level from 0 (nothing written) to 4 (a lot).
fn heat_color(level: u8, dark: bool) -> Color32 {
    const LIGHT: [[u8; 3]; 5] = [[226, 232, 240], [191, 219, 254], [147, 197, 253], [59, 130, 246], [29, 78, 216]];
    const DARK: [[u8; 3]; 5] = [[30, 41, 59], [30, 58, 138], [37, 99, 235], [96, 165, 250], [191, 219, 254]];
    rgb(if dark { DARK } else { LIGHT }[usize::from(level.min(4))])
}

/// Daily mood dots with a 7-day average line, over one year.
fn mood_chart(ui: &mut egui::Ui, p: &Palette, series: &[(NaiveDate, f32, f32)], year: i32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 190.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let plot = egui::Rect::from_min_max(rect.min + vec2(34.0, 8.0), rect.max - vec2(8.0, 24.0));
    painter.rect_filled(plot, 4.0, p.cell);
    painter.rect_stroke(plot, 4.0, Stroke::new(1.0, p.cell_border), egui::StrokeKind::Inside);

    let y_of = |v: f32| plot.max.y - (v - 1.0) / 4.0 * plot.height();
    for v in 1..=5 {
        let y = y_of(v as f32);
        painter.line_segment([egui::pos2(plot.min.x, y), egui::pos2(plot.max.x, y)], Stroke::new(1.0, p.cell_border.gamma_multiply(0.5)));
        painter.text(egui::pos2(plot.min.x - 6.0, y), Align2::RIGHT_CENTER, v.to_string(), FontId::proportional(12.0), p.muted);
    }
    let days_in_year = NaiveDate::from_ymd_opt(year, 12, 31).unwrap().ordinal() as f32;
    let x_of = |d: NaiveDate| plot.min.x + (d.ordinal0() as f32 + 0.5) / days_in_year * plot.width();
    for m in 1..=12 {
        let first = NaiveDate::from_ymd_opt(year, m, 1).unwrap();
        painter.text(egui::pos2(x_of(first), plot.max.y + 4.0), Align2::LEFT_TOP, first.format("%b").to_string(), FontId::proportional(12.0), p.muted);
    }
    for (d, m, _) in series {
        painter.circle_filled(egui::pos2(x_of(*d), y_of(*m)), 3.5, mood_color(*m));
    }
    if series.len() >= 2 {
        let points: Vec<egui::Pos2> = series.iter().map(|(d, _, avg)| egui::pos2(x_of(*d), y_of(*avg))).collect();
        painter.add(egui::Shape::line(points, Stroke::new(2.5, p.accent)));
    }
}

fn search_id() -> egui::Id {
    egui::Id::new("search_box")
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ExportScope {
    Day,
    Week,
    Month,
    Year,
    All,
}
