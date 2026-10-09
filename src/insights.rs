//! The Insights window: mood patterns, writing statistics, and a review of a week, month or year.

use super::*;
use crate::reports::{self, Avg, MoodReport, Period, Review, WritingReport};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Mood,
    Writing,
    Review,
}

struct Cache {
    key: (u64, usize, usize),
    mood: MoodReport,
    writing: WritingReport,
    review: Review,
}

pub(crate) struct InsightsState {
    pub open: bool,
    pub tab: Tab,
    /// Index into `RANGES` (for the Mood and Writing tabs).
    pub range: usize,
    /// Index into `Period::ALL` (for the Review tab).
    pub period: usize,
    msg: String,
    cache: Option<Cache>,
}

impl Default for InsightsState {
    fn default() -> Self {
        Self { open: false, tab: Tab::Mood, range: 2, period: 2, msg: String::new(), cache: None }
    }
}

impl InsightsState {
    /// Forget the computed numbers (the diary was locked).
    pub fn clear(&mut self) {
        self.cache = None;
        self.open = false;
        self.msg.clear();
    }
}

const RANGES: [&str; 4] = ["Last 30 days", "Last 90 days", "This year", "All time"];

fn range_bounds(range: usize, today: NaiveDate, entries: &[Entry]) -> (NaiveDate, NaiveDate) {
    match range {
        0 => (today - chrono::Days::new(29), today),
        1 => (today - chrono::Days::new(89), today),
        2 => Period::ThisYear.bounds(today),
        _ => {
            let first = entries.iter().filter_map(|e| NaiveDate::parse_from_str(&e.date, "%Y-%m-%d").ok()).min().unwrap_or(today);
            let last = entries.iter().filter_map(|e| NaiveDate::parse_from_str(&e.date, "%Y-%m-%d").ok()).max().unwrap_or(today).max(today);
            (first, last)
        }
    }
}

fn tile(ui: &mut egui::Ui, p: &Palette, label: &str, value: String) {
    egui::Frame::new().fill(p.cell).stroke(Stroke::new(1.0, p.cell_border)).corner_radius(8).inner_margin(10).show(ui, |ui| {
        ui.set_min_width(100.0);
        ui.label(RichText::new(value).size(24.0).strong().color(p.title));
        ui.label(RichText::new(label).small().color(p.muted));
    });
}

/// One labelled horizontal bar. `value` is out of `max`; `shade` colours the bar.
fn bar_row(ui: &mut egui::Ui, p: &Palette, label: &str, fraction: Option<f32>, shade: Color32, text: String) {
    ui.horizontal(|ui| {
        ui.add_sized([150.0, 20.0], egui::Label::new(RichText::new(label).color(p.ink)).truncate());
        let max_w = (ui.available_width() - 130.0).clamp(40.0, 420.0);
        let (r, _) = ui.allocate_exact_size(vec2(max_w, 18.0), Sense::hover());
        ui.painter().rect_filled(r, 4.0, p.head_bg.gamma_multiply(0.6));
        if let Some(f) = fraction {
            let fill = egui::Rect::from_min_size(r.min, vec2(r.width() * f.clamp(0.0, 1.0), r.height()));
            ui.painter().rect_filled(fill, 4.0, shade);
        }
        ui.label(RichText::new(text).small().color(p.muted));
    });
}

fn mood_rows(ui: &mut egui::Ui, p: &Palette, title: &str, rows: &[(String, Avg)]) {
    ui.label(RichText::new(title).size(17.0).strong().color(p.title));
    ui.add_space(2.0);
    for (label, avg) in rows {
        match avg.mean() {
            Some(m) => bar_row(ui, p, label, Some(m / 5.0), mood_color(m), format!("{m:.1}  ({} {})", avg.n, if avg.n == 1 { "entry" } else { "entries" })),
            None => bar_row(ui, p, label, None, p.muted, "no ratings".into()),
        }
    }
    ui.add_space(10.0);
}

fn word_cloud(ui: &mut egui::Ui, p: &Palette, words: &[(String, usize)]) {
    let most = words.first().map_or(1, |w| w.1).max(1) as f32;
    let colors = [p.accent, p.title, Color32::from_rgb(20, 184, 166), Color32::from_rgb(168, 85, 247), Color32::from_rgb(249, 115, 22)];
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(12.0, 4.0);
        // Alphabetical, so the size (not the order) shows what is common.
        let mut sorted: Vec<&(String, usize)> = words.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        for (i, (w, n)) in sorted.iter().map(|x| (&x.0, x.1)).enumerate() {
            let size = 14.0 + 22.0 * (n as f32 / most).sqrt();
            ui.label(RichText::new(w).size(size).strong().color(colors[i % colors.len()])).on_hover_text(format!("{n} times"));
        }
    });
}

/// Columns for hours of the day (24) or days of the week (7).
fn column_chart(ui: &mut egui::Ui, p: &Palette, values: &[usize], labels: &[String], tint: Color32) {
    let most = values.iter().copied().max().unwrap_or(0).max(1) as f32;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(620.0), 120.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let n = values.len() as f32;
    let slot = rect.width() / n;
    for (i, v) in values.iter().enumerate() {
        let h = (rect.height() - 22.0) * (*v as f32 / most);
        let x = rect.min.x + slot * i as f32;
        let bar = egui::Rect::from_min_max(egui::pos2(x + slot * 0.15, rect.max.y - 20.0 - h), egui::pos2(x + slot * 0.85, rect.max.y - 20.0));
        painter.rect_filled(bar, 3.0, if *v > 0 { tint } else { p.head_bg.gamma_multiply(0.6) });
        if !labels[i].is_empty() {
            painter.text(egui::pos2(x + slot * 0.5, rect.max.y - 2.0), Align2::CENTER_BOTTOM, &labels[i], FontId::proportional(12.0), p.muted);
        }
        if *v > 0 && slot > 18.0 {
            painter.text(egui::pos2(x + slot * 0.5, bar.min.y - 1.0), Align2::CENTER_BOTTOM, v.to_string(), FontId::proportional(11.0), p.ink);
        }
    }
}

impl DiaryApp {
    pub(crate) fn insights_window(&mut self, ctx: &egui::Context) {
        if !self.insights.open {
            return;
        }
        self.ensure_stats();
        let today = Local::now().date_naive();
        let goal = self.data.settings.daily_word_goal;

        // Work out the numbers again only when something they depend on has changed.
        let key = (self.rev, self.insights.range, self.insights.period);
        if self.insights.cache.as_ref().map(|c| c.key) != Some(key) {
            let (from, to) = range_bounds(self.insights.range, today, &self.data.entries);
            let per_day = &self.stats_cache.as_ref().unwrap().1;
            let entries = &self.data.entries;
            self.insights.cache = Some(Cache {
                key,
                mood: reports::mood_report(entries, from, to),
                writing: reports::writing_report(entries, from, to, 40),
                review: reports::review(entries, per_day, &self.data.images, Period::ALL[self.insights.period].0, today, goal),
            });
        }

        let p = palette(ctx);
        let dark = ctx.theme() == egui::Theme::Dark;
        let DiaryApp { insights, data, media, .. } = self;
        let cache = insights.cache.as_ref().unwrap();
        let mut close = false;
        let mut jump = None;
        let mut export = false;
        let (mut tab, mut range, mut period) = (insights.tab, insights.range, insights.period);

        egui::Window::new("Insights")
            .title_bar(false)
            .frame(popup_frame(&p, dark))
            .resizable(true)
            .default_size([880.0, 680.0])
            .min_size([520.0, 360.0])
            .pivot(Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                close |= popup_header(ui, &p, "Insights");
                egui::Frame::new().inner_margin(14).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (t, name) in [(Tab::Mood, "Mood patterns"), (Tab::Writing, "Writing"), (Tab::Review, "Review")] {
                            let mut b = egui::Button::new(RichText::new(name).strong().color(Color32::WHITE));
                            if tab == t {
                                b = b.fill(Color32::from_rgb(23, 37, 84)).stroke(Stroke::new(2.0, p.accent));
                            }
                            if ui.add(b).clicked() {
                                tab = t;
                            }
                        }
                    });
                    ui.add_space(6.0);
                    if tab == Tab::Review {
                        ui.horizontal_wrapped(|ui| {
                            for (i, (_, name)) in Period::ALL.iter().enumerate() {
                                ui.radio_value(&mut period, i, *name);
                            }
                        });
                    } else {
                        ui.horizontal_wrapped(|ui| {
                            for (i, name) in RANGES.iter().enumerate() {
                                ui.radio_value(&mut range, i, *name);
                            }
                        });
                    }
                    ui.separator();

                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match tab {
                        Tab::Mood => {
                            let m = &cache.mood;
                            ui.label(RichText::new(format!("{} {} with a mood rating in this range.", m.rated, if m.rated == 1 { "entry" } else { "entries" })).color(p.muted));
                            ui.add_space(6.0);
                            for note in &m.notes {
                                ui.label(RichText::new(note).size(16.0).color(p.ink));
                                ui.add_space(4.0);
                            }
                            ui.add_space(8.0);
                            let rows = |names: &[&str], avgs: &[Avg]| -> Vec<(String, Avg)> { names.iter().zip(avgs).map(|(n, a)| (n.to_string(), *a)).collect() };
                            mood_rows(ui, &p, "By day of the week", &rows(&reports::WEEKDAYS, &m.weekday));
                            mood_rows(ui, &p, "By time of day", &rows(&reports::DAYPARTS, &m.daypart));
                            mood_rows(ui, &p, "By length of entry", &rows(&reports::LENGTHS, &m.length));
                            let cats: Vec<(String, Avg)> = m.categories.iter().map(|(c, mean, n)| (c.clone(), Avg::from_mean(*mean, *n))).collect();
                            mood_rows(ui, &p, "By category", &cats);
                            ui.label(RichText::new("These are patterns in your own ratings, not causes. Groups with fewer than three entries are left out of the written notes.").small().color(p.muted));
                        }
                        Tab::Writing => {
                            let w = &cache.writing;
                            ui.horizontal_wrapped(|ui| {
                                tile(ui, &p, "entries", w.entries.to_string());
                                tile(ui, &p, "words", w.words.to_string());
                                tile(ui, &p, "words per entry", w.avg_words.to_string());
                            });
                            ui.add_space(12.0);
                            ui.label(RichText::new("Words you use most").size(17.0).strong().color(p.title));
                            ui.label(RichText::new("Common filler words and private entries are left out.").small().color(p.muted));
                            ui.add_space(4.0);
                            if w.top_words.is_empty() {
                                ui.label(RichText::new("Write a few more entries and your favourite words will appear here.").color(p.muted));
                            } else {
                                word_cloud(ui, &p, &w.top_words);
                            }
                            ui.add_space(14.0);
                            ui.label(RichText::new("When you write").size(17.0).strong().color(p.title));
                            ui.label(RichText::new("Entries by hour of the day").small().color(p.muted));
                            let hours: Vec<String> = (0..24).map(|h| if h % 3 == 0 { format!("{h}") } else { String::new() }).collect();
                            column_chart(ui, &p, &w.by_hour, &hours, p.accent);
                            ui.add_space(8.0);
                            ui.label(RichText::new("Entries by day of the week").small().color(p.muted));
                            let days: Vec<String> = reports::WEEKDAYS.iter().map(|d| d.to_string()).collect();
                            column_chart(ui, &p, &w.by_weekday, &days, Color32::from_rgb(20, 184, 166));
                            ui.add_space(14.0);
                            ui.label(RichText::new("Longest entries").size(17.0).strong().color(p.title));
                            for (id, date, words) in &w.longest {
                                let label = data.entries.iter().find(|e| e.id == *id).map(entry_label).unwrap_or_default();
                                let mut label = label;
                                if label.chars().count() > 60 {
                                    label = label.chars().take(60).collect::<String>() + "…";
                                }
                                if ui.link(format!("{date}  ·  {words} words  ·  {label}")).clicked() {
                                    jump = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok();
                                }
                            }
                        }
                        Tab::Review => {
                            let r = &cache.review;
                            let s = &r.summary;
                            ui.label(RichText::new(r.title).size(24.0).strong().color(p.title));
                            ui.label(RichText::new(format!("{} to {}", r.from.format("%e %B %Y"), r.to.format("%e %B %Y"))).color(p.muted));
                            ui.add_space(8.0);
                            ui.horizontal_wrapped(|ui| {
                                tile(ui, &p, "entries", s.entries.to_string());
                                tile(ui, &p, "days written", s.days_written.to_string());
                                tile(ui, &p, "words", s.words.to_string());
                                tile(ui, &p, "longest streak", s.longest_streak.to_string());
                                tile(ui, &p, "average mood", s.mood_avg.map_or("–".into(), |m| format!("{m:.1}")));
                            });
                            ui.add_space(8.0);
                            if let (Some(m), Some(prev)) = (s.mood_avg, r.previous_mood) {
                                let diff = m - prev;
                                let text = if diff.abs() < 0.05 {
                                    "Your mood was about the same as in the period before.".to_string()
                                } else {
                                    format!("Your mood was {} by {:.1} compared with the period before.", if diff > 0.0 { "up" } else { "down" }, diff.abs())
                                };
                                ui.label(RichText::new(text).color(p.ink));
                            }
                            if let Some((d, m)) = r.best_day {
                                ui.label(RichText::new(format!("Happiest day: {} ({m:.1})", d.format("%A %e %B"))).color(p.ink));
                            }
                            if let Some((d, m)) = r.hardest_day.filter(|h| Some(h.0) != r.best_day.map(|b| b.0)) {
                                ui.label(RichText::new(format!("Hardest day: {} ({m:.1})", d.format("%A %e %B"))).color(p.ink));
                            }
                            if s.goal_days > 0 {
                                ui.label(RichText::new(format!("You reached your daily word goal on {} {}.", s.goal_days, if s.goal_days == 1 { "day" } else { "days" })).color(p.ink));
                            }
                            if !s.categories.is_empty() {
                                ui.add_space(10.0);
                                ui.label(RichText::new("Where you wrote").size(17.0).strong().color(p.title));
                                let most = s.categories.first().map_or(1, |c| c.1).max(1) as f32;
                                for (name, n) in &s.categories {
                                    bar_row(ui, &p, name, Some(*n as f32 / most), category_color(&data.categories, name), n.to_string());
                                }
                            }
                            if !r.top_words.is_empty() {
                                ui.add_space(10.0);
                                ui.label(RichText::new("Words that kept coming up").size(17.0).strong().color(p.title));
                                word_cloud(ui, &p, &r.top_words);
                            }
                            if !r.pictures.is_empty() {
                                ui.add_space(10.0);
                                ui.label(RichText::new("Pictures").size(17.0).strong().color(p.title));
                                ui.horizontal_wrapped(|ui| {
                                    for (id, caption) in &r.pictures {
                                        if let Some(tex) = media.thumbnail(ctx, &data.images, id) {
                                            let natural = tex.size_vec2();
                                            let scale = 110.0 / natural.x.max(natural.y);
                                            ui.image((tex.id(), natural * scale)).on_hover_text(caption);
                                        }
                                    }
                                });
                            }
                            if !r.highlights.is_empty() {
                                ui.add_space(10.0);
                                ui.label(RichText::new("Entries to remember").size(17.0).strong().color(p.title));
                                for h in &r.highlights {
                                    let c = category_color(&data.categories, &h.category);
                                    let fg = text_on(c);
                                    let card = egui::Frame::new().fill(c).corner_radius(8).inner_margin(8).show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.label(RichText::new(format!("{}   {}   {}{}", h.date, h.time, h.category, if h.pinned { "   (pinned)" } else { "" })).strong().small().color(fg));
                                        ui.label(RichText::new(&h.snippet).color(fg));
                                    });
                                    if ui.interact(card.response.rect, ui.id().with(("hl", h.entry_id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                        jump = NaiveDate::parse_from_str(&h.date, "%Y-%m-%d").ok();
                                    }
                                    ui.add_space(4.0);
                                }
                            }
                            ui.add_space(12.0);
                            if ui.button("Save this review as a PDF…").clicked() {
                                export = true;
                            }
                            if !insights.msg.is_empty() {
                                ui.add(egui::Label::new(RichText::new(&insights.msg).color(p.ink)).wrap());
                            }
                        }
                    });
                });
            });

        insights.tab = tab;
        insights.range = range;
        insights.period = period;
        if export {
            let review = &insights.cache.as_ref().unwrap().review;
            let name = format!("diary-{}-{}.pdf", review.title.to_lowercase().replace(' ', "-"), review.from.format("%Y-%m-%d"));
            if let Some(dest) = rfd::FileDialog::new().set_file_name(name).add_filter("PDF document", &["pdf"]).save_file() {
                insights.msg = match export::build_review_pdf(review, &data.categories, &data.images, true)
                    .and_then(|bytes| std::fs::write(&dest, bytes).map_err(|e| format!("Couldn't save the PDF: {e}")))
                {
                    Ok(()) => format!("Saved {}", dest.display()),
                    Err(e) => e,
                };
            }
        }
        if let Some(d) = jump {
            self.selected = d;
            self.month = d.with_day(1).unwrap();
            self.day_scroll_pending = true;
        }
        if close {
            self.insights.open = false;
        }
    }
}

