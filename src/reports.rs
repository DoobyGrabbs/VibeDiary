//! The numbers behind the Insights window: mood patterns, writing statistics and reviews of a
//! week, month or year. Everything here is pure, so it can be tested without a window.
//!
//! Private entries count towards totals and moods, but their *text* never feeds the word lists,
//! highlights or pictures.

use std::collections::{BTreeMap, HashMap};

use chrono::{Datelike, NaiveDate};

use crate::stats::{self, DayStat, Summary};
use crate::{Entry, markdown};

pub const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const WEEKDAY_NAMES: [&str; 7] = ["Mondays", "Tuesdays", "Wednesdays", "Thursdays", "Fridays", "Saturdays", "Sundays"];
pub const DAYPARTS: [&str; 4] = ["Morning", "Afternoon", "Evening", "Night"];
pub const LENGTHS: [&str; 3] = ["Short (under 50 words)", "Medium (50 to 199)", "Long (200+)"];

/// Fewest entries in a group before it is mentioned in a written observation.
const MIN_SAMPLE: usize = 3;

fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

fn hour_of(e: &Entry) -> Option<usize> {
    e.added_at.get(11..13)?.parse().ok()
}

fn daypart(hour: usize) -> usize {
    match hour {
        5..=11 => 0,
        12..=16 => 1,
        17..=21 => 2,
        _ => 3,
    }
}

fn in_range(e: &Entry, from: NaiveDate, to: NaiveDate) -> bool {
    parse_date(&e.date).is_some_and(|d| d >= from && d <= to)
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Avg {
    sum: f32,
    pub n: usize,
}

impl Avg {
    fn add(&mut self, v: f32) {
        self.sum += v;
        self.n += 1;
    }

    /// An average that was already worked out, for drawing.
    pub fn from_mean(mean: f32, n: usize) -> Avg {
        Avg { sum: mean * n as f32, n }
    }

    pub fn mean(&self) -> Option<f32> {
        (self.n > 0).then(|| self.sum / self.n as f32)
    }
}

// ---------------------------------------------------------------------------------------------
// Mood patterns
// ---------------------------------------------------------------------------------------------

#[derive(Default, Clone, Debug)]
pub struct MoodReport {
    /// Entries with a mood rating in the range.
    pub rated: usize,
    pub weekday: [Avg; 7],
    pub daypart: [Avg; 4],
    pub length: [Avg; 3],
    /// (category, average mood, entries), happiest first.
    pub categories: Vec<(String, f32, usize)>,
    /// Plain-language observations.
    pub notes: Vec<String>,
}

pub fn mood_report(entries: &[Entry], from: NaiveDate, to: NaiveDate) -> MoodReport {
    let mut r = MoodReport::default();
    let mut cats: BTreeMap<&str, Avg> = BTreeMap::new();
    for e in entries.iter().filter(|e| in_range(e, from, to)) {
        let Some(mood) = e.mood.filter(|m| (1..=5).contains(m)).map(f32::from) else { continue };
        r.rated += 1;
        if let Some(d) = parse_date(&e.date) {
            r.weekday[d.weekday().num_days_from_monday() as usize].add(mood);
        }
        if let Some(h) = hour_of(e) {
            r.daypart[daypart(h)].add(mood);
        }
        let words = markdown::word_count(&e.text);
        r.length[if words < 50 { 0 } else if words < 200 { 1 } else { 2 }].add(mood);
        cats.entry(e.category.as_str()).or_default().add(mood);
    }
    r.categories = cats.into_iter().filter_map(|(c, a)| a.mean().map(|m| (c.to_string(), m, a.n))).collect();
    r.categories.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    r.notes = notes(&r);
    r
}

/// Highest and lowest groups that have enough entries to mean something.
fn extremes(groups: &[Avg]) -> Option<((usize, f32, usize), (usize, f32, usize))> {
    let ok: Vec<(usize, f32, usize)> = groups.iter().enumerate().filter(|(_, a)| a.n >= MIN_SAMPLE).filter_map(|(i, a)| a.mean().map(|m| (i, m, a.n))).collect();
    if ok.len() < 2 {
        return None;
    }
    let best = *ok.iter().max_by(|a, b| a.1.total_cmp(&b.1))?;
    let worst = *ok.iter().min_by(|a, b| a.1.total_cmp(&b.1))?;
    Some((best, worst))
}

fn notes(r: &MoodReport) -> Vec<String> {
    let mut out = Vec::new();
    if let Some((best, worst)) = extremes(&r.weekday) {
        if best.1 - worst.1 >= 0.3 {
            out.push(format!(
                "You tend to feel happiest on {} (average {:.1} from {} entries) and least happy on {} ({:.1}).",
                WEEKDAY_NAMES[best.0], best.1, best.2, WEEKDAY_NAMES[worst.0], worst.1
            ));
        }
    }
    if let Some((best, worst)) = extremes(&r.daypart) {
        if best.1 - worst.1 >= 0.3 {
            out.push(format!(
                "Entries written in the {} are your happiest ({:.1}); {} ones are the lowest ({:.1}).",
                DAYPARTS[best.0].to_lowercase(),
                best.1,
                DAYPARTS[worst.0].to_lowercase(),
                worst.1
            ));
        }
    }
    let ok: Vec<&(String, f32, usize)> = r.categories.iter().filter(|c| c.2 >= MIN_SAMPLE).collect();
    if ok.len() >= 2 {
        let (best, worst) = (ok[0], ok[ok.len() - 1]);
        if best.1 - worst.1 >= 0.3 {
            out.push(format!("“{}” entries are your happiest ({:.1}) and “{}” entries the lowest ({:.1}).", best.0, best.1, worst.0, worst.1));
        }
    }
    if let Some((best, worst)) = extremes(&r.length) {
        if best.1 - worst.1 >= 0.4 {
            out.push(format!(
                "{} entries tend to be happier ({:.1}) than {} ones ({:.1}).",
                LENGTHS[best.0].split(' ').next().unwrap_or(""),
                best.1,
                LENGTHS[worst.0].split(' ').next().unwrap_or("").to_lowercase(),
                worst.1
            ));
        }
    }
    if out.is_empty() {
        out.push(if r.rated < MIN_SAMPLE * 2 {
            format!("Rate a few more entries with a mood to see patterns here. So far {} {} rated.", r.rated, if r.rated == 1 { "entry is" } else { "entries are" })
        } else {
            "Your mood looks fairly even across days, times and categories.".to_string()
        });
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Writing statistics
// ---------------------------------------------------------------------------------------------

#[derive(Default, Clone, Debug)]
pub struct WritingReport {
    pub entries: usize,
    pub words: usize,
    pub avg_words: usize,
    /// Most used words, most used first.
    pub top_words: Vec<(String, usize)>,
    /// Entries written in each hour of the day.
    pub by_hour: [usize; 24],
    /// Entries per weekday.
    pub by_weekday: [usize; 7],
    /// Longest entries: (entry id, date, words). Private entries are left out.
    pub longest: Vec<(u64, String, usize)>,
}

const STOP_WORDS: &str = "the and for are but not you all any can had her was one our out has have him his how its let may new now old see \
two way who did get got him she too use that with this from they will would there their what about which when were been \
into than then them these those your more some such only over also just like very much even most other could should \
because while where after before again here does done doing being each both few own same off out our ours yours mine \
ive im its dont didnt cant wont isnt wasnt thats theres youre were weve theyre hes shes ill youll hed";

/// Words worth counting: lower case, at least three letters, not a common filler word.
pub fn words_of(text: &str) -> Vec<String> {
    let stop: std::collections::HashSet<&str> = STOP_WORDS.split_whitespace().collect();
    text.split(|c: char| !(c.is_alphabetic() || c == '\''))
        .map(|w| w.trim_matches('\'').to_lowercase())
        .filter(|w| w.chars().count() >= 3 && !stop.contains(w.replace('\'', "").as_str()))
        .collect()
}

pub fn writing_report(entries: &[Entry], from: NaiveDate, to: NaiveDate, top: usize) -> WritingReport {
    let mut r = WritingReport::default();
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut longest: Vec<(u64, String, usize)> = Vec::new();
    for e in entries.iter().filter(|e| in_range(e, from, to)) {
        let words = markdown::word_count(&e.text);
        r.entries += 1;
        r.words += words;
        if let Some(h) = hour_of(e) {
            r.by_hour[h.min(23)] += 1;
        }
        if let Some(d) = parse_date(&e.date) {
            r.by_weekday[d.weekday().num_days_from_monday() as usize] += 1;
        }
        if e.sensitive {
            continue; // counted above, but its words are not looked at
        }
        longest.push((e.id, e.date.clone(), words));
        for w in words_of(&markdown::plain_text(&e.text)) {
            *counts.entry(w).or_default() += 1;
        }
    }
    r.avg_words = if r.entries == 0 { 0 } else { r.words / r.entries };
    let mut words: Vec<(String, usize)> = counts.into_iter().filter(|(_, n)| *n >= 2).collect();
    words.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    words.truncate(top);
    r.top_words = words;
    longest.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| b.1.cmp(&a.1)));
    longest.truncate(5);
    r.longest = longest;
    r
}

// ---------------------------------------------------------------------------------------------
// Reviews
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Period {
    ThisWeek,
    LastWeek,
    ThisMonth,
    LastMonth,
    ThisYear,
    LastYear,
}

impl Period {
    pub const ALL: [(Period, &'static str); 6] = [
        (Period::ThisWeek, "This week"),
        (Period::LastWeek, "Last week"),
        (Period::ThisMonth, "This month"),
        (Period::LastMonth, "Last month"),
        (Period::ThisYear, "This year"),
        (Period::LastYear, "Last year"),
    ];

    /// First and last day of the period that contains (or precedes) `today`.
    pub fn bounds(self, today: NaiveDate) -> (NaiveDate, NaiveDate) {
        let monday = |d: NaiveDate| d - chrono::Days::new(u64::from(d.weekday().num_days_from_monday()));
        let month = |y: i32, m: u32| {
            let first = NaiveDate::from_ymd_opt(y, m, 1).unwrap();
            let next = if m == 12 { NaiveDate::from_ymd_opt(y + 1, 1, 1) } else { NaiveDate::from_ymd_opt(y, m + 1, 1) }.unwrap();
            (first, next.pred_opt().unwrap())
        };
        let year = |y: i32| (NaiveDate::from_ymd_opt(y, 1, 1).unwrap(), NaiveDate::from_ymd_opt(y, 12, 31).unwrap());
        match self {
            Period::ThisWeek => (monday(today), monday(today) + chrono::Days::new(6)),
            Period::LastWeek => {
                let m = monday(today) - chrono::Days::new(7);
                (m, m + chrono::Days::new(6))
            }
            Period::ThisMonth => month(today.year(), today.month()),
            Period::LastMonth => {
                if today.month() == 1 { month(today.year() - 1, 12) } else { month(today.year(), today.month() - 1) }
            }
            Period::ThisYear => year(today.year()),
            Period::LastYear => year(today.year() - 1),
        }
    }

    /// The period just before this one, for comparison.
    pub fn previous_bounds(self, today: NaiveDate) -> (NaiveDate, NaiveDate) {
        let (from, to) = self.bounds(today);
        let days = (to - from).num_days() + 1;
        match self {
            Period::ThisMonth | Period::LastMonth => {
                let last = from.pred_opt().unwrap();
                (last.with_day(1).unwrap(), last)
            }
            Period::ThisYear | Period::LastYear => {
                (NaiveDate::from_ymd_opt(from.year() - 1, 1, 1).unwrap(), NaiveDate::from_ymd_opt(from.year() - 1, 12, 31).unwrap())
            }
            _ => (from - chrono::Days::new(days as u64), from.pred_opt().unwrap()),
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Period::ThisWeek | Period::LastWeek => "Week in review",
            Period::ThisMonth | Period::LastMonth => "Month in review",
            Period::ThisYear | Period::LastYear => "Year in review",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Highlight {
    pub entry_id: u64,
    pub date: String,
    pub time: String,
    pub category: String,
    pub snippet: String,
    pub pinned: bool,
}

#[derive(Clone, Debug)]
pub struct Review {
    pub title: &'static str,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub summary: Summary,
    /// Average mood in the period before this one.
    pub previous_mood: Option<f32>,
    pub best_day: Option<(NaiveDate, f32)>,
    pub hardest_day: Option<(NaiveDate, f32)>,
    pub top_words: Vec<(String, usize)>,
    /// Up to six pictures: (picture id, caption).
    pub pictures: Vec<(String, String)>,
    pub highlights: Vec<Highlight>,
}

pub fn review(
    entries: &[Entry],
    per_day: &BTreeMap<NaiveDate, DayStat>,
    images: &BTreeMap<String, String>,
    period: Period,
    today: NaiveDate,
    goal: u32,
) -> Review {
    let (from, to) = period.bounds(today);
    let (pfrom, pto) = period.previous_bounds(today);
    let summary = stats::summarise(entries, per_day, from, to, today, goal);
    let previous_mood = stats::summarise(entries, per_day, pfrom, pto, today, goal).mood_avg;

    let rated: Vec<(NaiveDate, f32)> = per_day.range(from..=to).filter_map(|(d, s)| s.mood().map(|m| (*d, m))).collect();
    let best_day = rated.iter().copied().max_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.0.cmp(&a.0)));
    let hardest_day = rated.iter().copied().min_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));

    let public: Vec<&Entry> = entries.iter().filter(|e| in_range(e, from, to) && !e.sensitive).collect();
    let top_words = writing_report(entries, from, to, 8).top_words;

    let mut pictures = Vec::new();
    for e in &public {
        for (id, caption) in markdown::images(&e.text) {
            if images.contains_key(&id) && pictures.len() < 6 {
                pictures.push((id, caption));
            }
        }
    }

    // Entries to remember: pinned ones first, then the longest.
    let mut ranked: Vec<(&Entry, usize)> = public.iter().map(|e| (*e, markdown::word_count(&e.text))).collect();
    ranked.sort_by(|a, b| b.0.pinned.cmp(&a.0.pinned).then_with(|| b.1.cmp(&a.1)).then_with(|| a.0.added_at.cmp(&b.0.added_at)));
    let highlights = ranked
        .into_iter()
        .take(5)
        .map(|(e, _)| {
            let text = markdown::plain_text(&e.text);
            let snippet = if text.chars().count() > 160 { text.chars().take(160).collect::<String>() + "…" } else { text };
            Highlight {
                entry_id: e.id,
                date: e.date.clone(),
                time: e.added_at.get(11..16).unwrap_or("").to_string(),
                category: e.category.clone(),
                snippet,
                pinned: e.pinned,
            }
        })
        .collect();

    Review { title: period.title(), from, to, summary, previous_mood, best_day, hardest_day, top_words, pictures, highlights }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn entry(id: u64, date: &str, time: &str, cat: &str, text: &str, mood: Option<u8>) -> Entry {
        Entry { id, date: date.into(), added_at: format!("{date} {time}"), category: cat.into(), text: text.into(), mood, ..Default::default() }
    }

    #[test]
    fn periods() {
        let today = d(2026, 10, 9); // a Friday
        assert_eq!(Period::ThisWeek.bounds(today), (d(2026, 10, 5), d(2026, 10, 11)));
        assert_eq!(Period::LastWeek.bounds(today), (d(2026, 9, 28), d(2026, 10, 4)));
        assert_eq!(Period::ThisMonth.bounds(today), (d(2026, 10, 1), d(2026, 10, 31)));
        assert_eq!(Period::LastMonth.bounds(today), (d(2026, 9, 1), d(2026, 9, 30)));
        assert_eq!(Period::LastMonth.bounds(d(2026, 1, 15)), (d(2025, 12, 1), d(2025, 12, 31)));
        assert_eq!(Period::ThisYear.bounds(today), (d(2026, 1, 1), d(2026, 12, 31)));
        assert_eq!(Period::LastYear.bounds(today), (d(2025, 1, 1), d(2025, 12, 31)));
        assert_eq!(Period::ThisWeek.previous_bounds(today), (d(2026, 9, 28), d(2026, 10, 4)));
        assert_eq!(Period::ThisMonth.previous_bounds(today), (d(2026, 9, 1), d(2026, 9, 30)));
        assert_eq!(Period::ThisYear.previous_bounds(today), (d(2025, 1, 1), d(2025, 12, 31)));
    }

    #[test]
    fn mood_patterns_need_enough_entries_and_find_the_extremes() {
        // Saturdays are happy (5), Mondays low (2), three of each; Tuesday has too few to count.
        let mut entries = Vec::new();
        let mut id = 0;
        for week in 0..3 {
            let sat = d(2026, 9, 5) + chrono::Days::new(7 * week);
            let mon = d(2026, 9, 7) + chrono::Days::new(7 * week);
            id += 1;
            entries.push(entry(id, &sat.format("%Y-%m-%d").to_string(), "09:00:00", "Health", "x", Some(5)));
            id += 1;
            entries.push(entry(id, &mon.format("%Y-%m-%d").to_string(), "21:00:00", "Work", "x", Some(2)));
        }
        entries.push(entry(99, "2026-09-08", "09:00:00", "Work", "x", Some(1)));
        let r = mood_report(&entries, d(2026, 9, 1), d(2026, 9, 30));
        assert_eq!(r.rated, 7);
        assert_eq!(r.weekday[5].mean(), Some(5.0));
        assert_eq!(r.weekday[0].mean(), Some(2.0));
        assert_eq!(r.weekday[1].n, 1);
        assert!(r.notes[0].contains("Saturdays") && r.notes[0].contains("Mondays"), "{:?}", r.notes);
        assert!(!r.notes.iter().any(|n| n.contains("Tuesdays")), "a single entry proves nothing");
        assert_eq!(r.categories[0].0, "Health");
        assert_eq!(r.daypart[0].n, 4); // morning entries (Saturdays and the Tuesday one)
        assert_eq!(r.daypart[2].n, 3); // evening

        let few = mood_report(&entries[..2], d(2026, 9, 1), d(2026, 9, 30));
        assert!(few.notes[0].contains("Rate a few more"), "{:?}", few.notes);
    }

    #[test]
    fn writing_stats_skip_private_text_and_filler_words() {
        let mut secret = entry(2, "2026-10-02", "23:30:00", "Personal", "banana banana banana secretword secretword", None);
        secret.sensitive = true;
        let entries = vec![
            entry(1, "2026-10-01", "08:15:00", "Work", "The garden garden garden was lovely. I walked to the market and bought apples; apples were great.", None),
            secret,
            entry(3, "2026-10-03", "08:45:00", "Work", "garden again, with apples", Some(4)),
        ];
        let r = writing_report(&entries, d(2026, 10, 1), d(2026, 10, 31), 5);
        assert_eq!(r.entries, 3);
        assert_eq!(r.by_hour[8], 2);
        assert_eq!(r.by_hour[23], 1);
        assert_eq!(r.top_words[0], ("garden".to_string(), 4));
        assert_eq!(r.top_words[1], ("apples".to_string(), 3));
        assert!(!r.top_words.iter().any(|(w, _)| w == "the" || w == "banana" || w == "secretword"));
        assert!(!r.longest.iter().any(|(id, ..)| *id == 2), "private entries are not listed");
        assert!(r.words >= 20 && r.avg_words > 0);
        assert_eq!(words_of("Don't stop: it's THE end"), vec!["stop".to_string(), "end".to_string()], "filler words, contractions included, are skipped");
    }

    #[test]
    fn a_review_summarises_the_period_and_hides_private_entries() {
        let mut private = entry(3, "2026-10-07", "10:00:00", "Personal", "a very private long entry with many words in it indeed", Some(1));
        private.sensitive = true;
        let mut pinned = entry(2, "2026-10-06", "10:00:00", "Health", "short pinned note", Some(5));
        pinned.pinned = true;
        let entries = vec![
            entry(1, "2026-10-05", "09:00:00", "Work", "![pic](img:i1) a medium sized entry about the garden garden", Some(3)),
            pinned,
            private,
            entry(4, "2026-09-30", "09:00:00", "Work", "last week", Some(2)),
        ];
        let images = BTreeMap::from([("i1".to_string(), "x".to_string())]);
        let per_day = stats::per_day(&entries);
        let r = review(&entries, &per_day, &images, Period::ThisWeek, d(2026, 10, 9), 0);
        assert_eq!(r.title, "Week in review");
        assert_eq!((r.summary.entries, r.summary.days_written), (3, 3));
        assert_eq!(r.previous_mood, Some(2.0));
        assert_eq!(r.best_day, Some((d(2026, 10, 6), 5.0)));
        assert_eq!(r.hardest_day, Some((d(2026, 10, 7), 1.0)), "private moods still count in the numbers");
        assert_eq!(r.pictures, vec![("i1".to_string(), "pic".to_string())]);
        assert_eq!(r.highlights.len(), 2, "the private entry is not a highlight");
        assert!(r.highlights[0].pinned, "pinned entries come first");
        assert!(!r.highlights.iter().any(|h| h.snippet.contains("private")));
    }
}
