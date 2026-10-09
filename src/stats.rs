//! Numbers about what has been written: totals, streaks, heat-map levels, mood over time and
//! "On this day". Everything here is pure so it can be tested without a window.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, NaiveDate};

use crate::{Entry, markdown};

fn parse(date: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct DayStat {
    pub entries: usize,
    pub words: usize,
    mood_sum: u32,
    mood_n: u32,
}

impl DayStat {
    /// Average mood of the day's rated entries.
    pub fn mood(&self) -> Option<f32> {
        (self.mood_n > 0).then(|| self.mood_sum as f32 / self.mood_n as f32)
    }
}

/// Entries, words and mood for every day that has an entry.
pub fn per_day(entries: &[Entry]) -> BTreeMap<NaiveDate, DayStat> {
    let mut days: BTreeMap<NaiveDate, DayStat> = BTreeMap::new();
    for e in entries {
        let Some(d) = parse(&e.date) else { continue };
        let s = days.entry(d).or_default();
        s.entries += 1;
        s.words += markdown::word_count(&e.text);
        if let Some(m) = e.mood.filter(|m| (1..=5).contains(m)) {
            s.mood_sum += u32::from(m);
            s.mood_n += 1;
        }
    }
    days
}

/// Heat-map shade from 0 (nothing written) to 4 (a lot), based on the words written that day.
pub fn heat_level(stat: Option<&DayStat>) -> u8 {
    match stat {
        None => 0,
        Some(s) if s.words == 0 => 1,
        Some(s) => match s.words {
            0..=49 => 1,
            50..=149 => 2,
            150..=349 => 3,
            _ => 4,
        },
    }
}

/// Consecutive days with an entry, counting back from `today` (or from yesterday, if nothing has
/// been written today yet).
pub fn current_streak(days: &BTreeSet<NaiveDate>, today: NaiveDate) -> u32 {
    let mut day = if days.contains(&today) {
        today
    } else {
        match today.pred_opt() {
            Some(y) if days.contains(&y) => y,
            _ => return 0,
        }
    };
    let mut n = 0;
    while days.contains(&day) {
        n += 1;
        match day.pred_opt() {
            Some(p) => day = p,
            None => break,
        }
    }
    n
}

/// The longest run of consecutive days with an entry.
pub fn longest_streak(days: &BTreeSet<NaiveDate>) -> u32 {
    let (mut best, mut run, mut prev): (u32, u32, Option<NaiveDate>) = (0, 0, None);
    for &d in days {
        run = if prev.and_then(|p| p.succ_opt()) == Some(d) { run + 1 } else { 1 };
        best = best.max(run);
        prev = Some(d);
    }
    best
}

#[derive(Default, Debug, PartialEq)]
pub struct Summary {
    pub entries: usize,
    pub days_written: usize,
    pub words: usize,
    pub longest_streak: u32,
    pub current_streak: u32,
    /// Days where the daily word goal was reached.
    pub goal_days: usize,
    pub mood_avg: Option<f32>,
    /// Entries per category, most used first.
    pub categories: Vec<(String, usize)>,
}

/// Totals for the entries dated `from..=to`. The current streak looks at all entries.
pub fn summarise(
    entries: &[Entry],
    per_day: &BTreeMap<NaiveDate, DayStat>,
    from: NaiveDate,
    to: NaiveDate,
    today: NaiveDate,
    goal: u32,
) -> Summary {
    let in_range: Vec<Entry> = entries
        .iter()
        .filter(|e| parse(&e.date).is_some_and(|d| d >= from && d <= to))
        .cloned()
        .collect();
    let days: BTreeMap<NaiveDate, &DayStat> = per_day.range(from..=to).map(|(d, s)| (*d, s)).collect();
    let day_set: BTreeSet<NaiveDate> = days.keys().copied().collect();
    let all_days: BTreeSet<NaiveDate> = entries.iter().filter_map(|e| parse(&e.date)).collect();

    let mut by_cat: BTreeMap<&str, usize> = BTreeMap::new();
    for e in &in_range {
        *by_cat.entry(e.category.as_str()).or_default() += 1;
    }
    let mut categories: Vec<(String, usize)> = by_cat.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    categories.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let moods: Vec<f32> = days.values().filter_map(|d| d.mood()).collect();
    Summary {
        entries: in_range.len(),
        days_written: days.len(),
        words: days.values().map(|d| d.words).sum(),
        longest_streak: longest_streak(&day_set),
        current_streak: current_streak(&all_days, today),
        goal_days: if goal == 0 { 0 } else { days.values().filter(|d| d.words >= goal as usize).count() },
        mood_avg: (!moods.is_empty()).then(|| moods.iter().sum::<f32>() / moods.len() as f32),
        categories,
    }
}

/// Daily average mood for `from..=to`, with a trailing 7-day moving average (over the days that
/// have a rating). Days without a rating are left out.
pub fn mood_series(days: &BTreeMap<NaiveDate, DayStat>, from: NaiveDate, to: NaiveDate) -> Vec<(NaiveDate, f32, f32)> {
    let rated: Vec<(NaiveDate, f32)> = days
        .iter()
        .filter(|(d, _)| **d >= from && **d <= to)
        .filter_map(|(d, s)| s.mood().map(|m| (*d, m)))
        .collect();
    rated
        .iter()
        .map(|&(d, m)| {
            let window: Vec<f32> = rated.iter().filter(|(o, _)| *o <= d && (d - *o).num_days() < 7).map(|(_, v)| *v).collect();
            (d, m, window.iter().sum::<f32>() / window.len() as f32)
        })
        .collect()
}

/// Entries written on the same calendar day in earlier years, most recent year first.
pub fn on_this_day(entries: &[Entry], date: NaiveDate) -> Vec<&Entry> {
    let mut found: Vec<&Entry> = entries
        .iter()
        .filter(|e| parse(&e.date).is_some_and(|d| d.month() == date.month() && d.day() == date.day() && d.year() < date.year()))
        .collect();
    found.sort_by(|a, b| b.added_at.cmp(&a.added_at));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn entry(id: u64, date: &str, cat: &str, text: &str, mood: Option<u8>) -> Entry {
        Entry { id, date: date.into(), added_at: format!("{date} 10:00:00"), category: cat.into(), text: text.into(), mood }
    }

    fn set(dates: &[(i32, u32, u32)]) -> BTreeSet<NaiveDate> {
        dates.iter().map(|&(y, m, day)| d(y, m, day)).collect()
    }

    #[test]
    fn streaks() {
        let days = set(&[(2026, 10, 1), (2026, 10, 2), (2026, 10, 3), (2026, 10, 7), (2026, 10, 8)]);
        assert_eq!(longest_streak(&days), 3);
        assert_eq!(longest_streak(&BTreeSet::new()), 0);
        // Written today: counts back from today.
        assert_eq!(current_streak(&days, d(2026, 10, 8)), 2);
        // Nothing yet today: yesterday keeps the streak alive.
        assert_eq!(current_streak(&days, d(2026, 10, 9)), 2);
        // A gap of a full day ends it.
        assert_eq!(current_streak(&days, d(2026, 10, 10)), 0);
    }

    #[test]
    fn heat_levels() {
        let stat = |words| DayStat { entries: 1, words, ..Default::default() };
        assert_eq!(heat_level(None), 0);
        assert_eq!(heat_level(Some(&stat(0))), 1);
        assert_eq!(heat_level(Some(&stat(10))), 1);
        assert_eq!(heat_level(Some(&stat(100))), 2);
        assert_eq!(heat_level(Some(&stat(200))), 3);
        assert_eq!(heat_level(Some(&stat(1000))), 4);
    }

    #[test]
    fn summary_totals() {
        let entries = vec![
            entry(1, "2026-10-01", "Work", "one two three", Some(4)),
            entry(2, "2026-10-01", "Work", "four five", Some(2)),
            entry(3, "2026-10-02", "Health", "six", None),
            entry(4, "2025-10-02", "Work", "old", Some(5)),
        ];
        let days = per_day(&entries);
        let s = summarise(&entries, &days, d(2026, 1, 1), d(2026, 12, 31), d(2026, 10, 2), 5);
        assert_eq!((s.entries, s.days_written, s.words), (3, 2, 6));
        assert_eq!(s.longest_streak, 2);
        assert_eq!(s.current_streak, 2);
        assert_eq!(s.goal_days, 1, "only 2026-10-01 reached 5 words");
        assert_eq!(s.mood_avg, Some(3.0));
        assert_eq!(s.categories, vec![("Work".to_string(), 2), ("Health".to_string(), 1)]);
        assert_eq!(summarise(&entries, &days, d(2026, 1, 1), d(2026, 12, 31), d(2026, 10, 2), 0).goal_days, 0);
        assert_eq!(summarise(&[], &BTreeMap::new(), d(2026, 1, 1), d(2026, 12, 31), d(2026, 10, 2), 0), Summary::default());
    }

    #[test]
    fn mood_series_averages_and_smooths() {
        let entries = vec![
            entry(1, "2026-10-01", "Work", "a", Some(2)),
            entry(2, "2026-10-01", "Work", "b", Some(4)),
            entry(3, "2026-10-02", "Work", "c", Some(5)),
            entry(4, "2026-10-03", "Work", "unrated", None),
            entry(5, "2026-10-20", "Work", "d", Some(1)),
        ];
        let series = mood_series(&per_day(&entries), d(2026, 10, 1), d(2026, 10, 31));
        assert_eq!(series.len(), 3, "unrated days are skipped");
        assert_eq!((series[0].0, series[0].1), (d(2026, 10, 1), 3.0));
        assert_eq!(series[1].2, 4.0, "average of 3 and 5");
        assert_eq!(series[2].2, 1.0, "the earlier days are more than a week back");
    }

    #[test]
    fn on_this_day_finds_earlier_years_only() {
        let entries = vec![
            entry(1, "2025-10-09", "Work", "last year", None),
            entry(2, "2024-10-09", "Work", "two years ago", None),
            entry(3, "2026-10-09", "Work", "today", None),
            entry(4, "2025-10-10", "Work", "other day", None),
        ];
        let found = on_this_day(&entries, d(2026, 10, 9));
        assert_eq!(found.iter().map(|e| e.id).collect::<Vec<_>>(), vec![1, 2]);
        assert!(on_this_day(&entries, d(2024, 10, 9)).is_empty());
    }
}
