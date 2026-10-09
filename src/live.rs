//! "What you see is what you get" editing support.
//!
//! The entry text stays Markdown, but while you type it is laid out with its formatting applied:
//! bold looks bold, headings are large, and the Markdown marks (`**`, `#`, backticks...) are
//! shrunk to nothing, except on the line the cursor is on, where they show so they can be edited.
//! The styled layout always contains exactly the characters of the text, so the cursor and
//! selection behave normally.
//!
//! Pictures can't live inside a text box, so the entry is cut into text blocks and picture
//! blocks (a picture is a line of its own); each text block gets its own editor.

use eframe::egui::{
    Color32, FontFamily, FontId, Stroke,
    text::{LayoutJob, TextFormat},
};

use crate::markdown::{IMAGE_SCHEME, bold_family};

pub struct LiveStyle {
    pub ink: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub size: f32,
}

#[derive(Clone, Copy, Default)]
struct Look {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    link: bool,
    /// Heading size multiplier (1.0 for body text).
    scale: f32,
}

struct Out<'a> {
    job: &'a mut LayoutJob,
    st: &'a LiveStyle,
    /// Show Markdown marks on this line?
    marks: bool,
}

impl Out<'_> {
    fn text(&mut self, s: &str, look: Look) {
        if s.is_empty() {
            return;
        }
        let size = self.st.size * if look.scale > 0.0 { look.scale } else { 1.0 };
        let family = if look.code {
            FontFamily::Monospace
        } else if look.bold || look.scale > 1.0 {
            bold_family()
        } else {
            FontFamily::Proportional
        };
        let color = if look.link { self.st.accent } else { self.st.ink };
        let fmt = TextFormat {
            font_id: FontId::new(if look.code { size * 0.92 } else { size }, family),
            color,
            italics: look.italic,
            strikethrough: if look.strike { Stroke::new(1.0, color) } else { Stroke::NONE },
            underline: if look.link { Stroke::new(1.0, color) } else { Stroke::NONE },
            background: if look.code { self.st.ink.gamma_multiply(0.13) } else { Color32::TRANSPARENT },
            ..Default::default()
        };
        self.job.append(s, 0.0, fmt);
    }

    /// A Markdown mark: dim when shown, (almost) zero width when hidden.
    fn mark(&mut self, s: &str, scale: f32) {
        if s.is_empty() {
            return;
        }
        let size = self.st.size * if scale > 0.0 { scale } else { 1.0 };
        let fmt = if self.marks {
            TextFormat { font_id: FontId::proportional(size * 0.9), color: self.st.muted, ..Default::default() }
        } else {
            TextFormat { font_id: FontId::proportional(1.0), color: Color32::TRANSPARENT, ..Default::default() }
        };
        self.job.append(s, 0.0, fmt);
    }

    /// A mark that stays visible (list bullets, quote bars, code fences).
    fn visible_mark(&mut self, s: &str, scale: f32) {
        let size = self.st.size * if scale > 0.0 { scale } else { 1.0 };
        let fmt = TextFormat {
            font_id: FontId::new(size, bold_family()),
            color: self.st.accent,
            ..Default::default()
        };
        self.job.append(s, 0.0, fmt);
    }
}

fn starts(chars: &[char], pat: &str) -> bool {
    let mut it = chars.iter();
    pat.chars().all(|p| it.next() == Some(&p))
}

fn find(chars: &[char], from: usize, pat: &str) -> Option<usize> {
    let n = pat.chars().count();
    (from..chars.len().saturating_sub(n - 1)).find(|&i| starts(&chars[i..], pat))
}

/// First lone `*` at or after `from` (not part of `**`).
fn find_star(chars: &[char], from: usize) -> Option<usize> {
    (from..chars.len()).find(|&j| chars[j] == '*' && chars.get(j + 1) != Some(&'*') && (j == 0 || chars[j - 1] != '*'))
}

fn s(chars: &[char]) -> String {
    chars.iter().collect()
}

fn inline(chars: &[char], look: Look, out: &mut Out) {
    let mut run = String::new();
    let mut i = 0;
    macro_rules! flush {
        () => {
            if !run.is_empty() {
                out.text(&run, look);
                run.clear();
            }
        };
    }
    while i < chars.len() {
        let rest = &chars[i..];
        if starts(rest, "**") {
            if let Some(j) = find(chars, i + 2, "**").filter(|&j| j > i + 2) {
                flush!();
                out.mark("**", look.scale);
                inline(&chars[i + 2..j], Look { bold: true, ..look }, out);
                out.mark("**", look.scale);
                i = j + 2;
                continue;
            }
        }
        if starts(rest, "~~") {
            if let Some(j) = find(chars, i + 2, "~~").filter(|&j| j > i + 2) {
                flush!();
                out.mark("~~", look.scale);
                inline(&chars[i + 2..j], Look { strike: true, ..look }, out);
                out.mark("~~", look.scale);
                i = j + 2;
                continue;
            }
        }
        if chars[i] == '*' && !starts(rest, "**") {
            if let Some(j) = find_star(chars, i + 1).filter(|&j| j > i + 1) {
                flush!();
                out.mark("*", look.scale);
                inline(&chars[i + 1..j], Look { italic: true, ..look }, out);
                out.mark("*", look.scale);
                i = j + 1;
                continue;
            }
        }
        if chars[i] == '`' {
            if let Some(j) = find(chars, i + 1, "`").filter(|&j| j > i + 1) {
                flush!();
                out.mark("`", look.scale);
                out.text(&s(&chars[i + 1..j]), Look { code: true, ..look });
                out.mark("`", look.scale);
                i = j + 1;
                continue;
            }
        }
        if chars[i] == '[' {
            if let Some(mid) = find(chars, i + 1, "](").filter(|&m| m > i + 1) {
                if let Some(end) = find(chars, mid + 2, ")") {
                    flush!();
                    out.mark("[", look.scale);
                    inline(&chars[i + 1..mid], Look { link: true, ..look }, out);
                    out.mark(&s(&chars[mid..=end]), look.scale);
                    i = end + 1;
                    continue;
                }
            }
        }
        run.push(chars[i]);
        i += 1;
    }
    flush!();
}

fn heading_level(line: &str) -> Option<usize> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    (1..=6).contains(&hashes).then_some(hashes).filter(|&h| line.chars().nth(h) == Some(' '))
}

fn style_line(line: &str, in_code: &mut bool, out: &mut Out) {
    let plain = Look { scale: 1.0, ..Look::default() };
    let trimmed = line.trim_start();

    if trimmed.starts_with("```") {
        *in_code = !*in_code;
        out.visible_mark(line, 1.0);
        return;
    }
    if *in_code {
        out.text(line, Look { code: true, ..plain });
        return;
    }
    if let Some(level) = heading_level(line) {
        let scale = match level {
            1 => 1.7,
            2 => 1.45,
            3 => 1.25,
            4 => 1.12,
            _ => 1.0,
        };
        let chars: Vec<char> = line.chars().collect();
        out.mark(&s(&chars[..level + 1]), scale);
        inline(&chars[level + 1..], Look { bold: true, scale, ..Look::default() }, out);
        return;
    }
    if line == "---" || line == "***" || line == "___" {
        out.visible_mark(line, 1.0);
        return;
    }
    let indent = line.len() - trimmed.len();
    let bullet = ["- ", "* ", "+ "].iter().find(|p| trimmed.starts_with(**p)).map(|p| p.len());
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    let numbered = (digits > 0 && trimmed[digits..].starts_with(". ")).then_some(digits + 2);
    let quote = trimmed.starts_with("> ").then_some(2);
    if let Some(len) = bullet.or(numbered).or(quote) {
        let chars: Vec<char> = line.chars().collect();
        let marker_end = line[..indent + len].chars().count();
        out.visible_mark(&s(&chars[..marker_end]), 1.0);
        inline(&chars[marker_end..], plain, out);
        return;
    }
    let chars: Vec<char> = line.chars().collect();
    inline(&chars, plain, out);
}

/// Lay out `text` with its Markdown styling. `reveal` is the selected char range: lines it
/// touches show their marks; `show_all` shows marks everywhere.
pub fn layout(text: &str, reveal: Option<(usize, usize)>, show_all: bool, st: &LiveStyle, wrap: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    let plain = TextFormat { font_id: FontId::proportional(st.size), color: st.ink, ..Default::default() };

    let mut in_code = false;
    let mut offset = 0usize;
    let lines: Vec<&str> = text.split('\n').collect();
    for (n, line) in lines.iter().enumerate() {
        let len = line.chars().count();
        let marks = show_all || reveal.is_some_and(|(a, b)| offset <= b && offset + len >= a);
        style_line(line, &mut in_code, &mut Out { job: &mut job, st, marks });
        if n + 1 < lines.len() {
            job.append("\n", 0.0, plain.clone());
        }
        offset += len + 1;
    }
    if text.is_empty() {
        job.append("", 0.0, plain);
    }
    job
}

// ---------------------------------------------------------------------------------------------
// Text blocks and picture blocks
// ---------------------------------------------------------------------------------------------

#[derive(Clone, PartialEq, Debug)]
pub enum Seg {
    /// Zero or more lines of text, joined with `\n` (an empty string means no lines at all).
    Text(String),
    /// A picture on a line of its own: `![alt](img:ID)`.
    Image { line: String, id: String, alt: String },
}

/// `![alt](img:ID)` and nothing else on the line.
pub fn image_line(line: &str) -> Option<(String, String)> {
    let t = line.trim();
    let rest = t.strip_prefix("![")?;
    let close = rest.find("](")?;
    let alt = &rest[..close];
    let target = rest[close + 2..].strip_suffix(')')?;
    let id = target.strip_prefix(IMAGE_SCHEME)?;
    (!id.is_empty() && !id.contains(')') && !alt.contains('\n')).then(|| (alt.to_string(), id.to_string()))
}

/// Cut a document into text and picture blocks. Always starts and ends with a text block, and
/// puts one between neighbouring pictures, so there is somewhere to type.
pub fn split(doc: &str) -> Vec<Seg> {
    let mut segs = Vec::new();
    let mut lines: Vec<&str> = Vec::new();
    for line in doc.split('\n') {
        if let Some((alt, id)) = image_line(line) {
            segs.push(Seg::Text(lines.join("\n")));
            segs.push(Seg::Image { line: line.to_string(), id, alt });
            lines.clear();
        } else {
            lines.push(line);
        }
    }
    segs.push(Seg::Text(lines.join("\n")));
    // A lone empty document is a single empty text block, as `join` expects.
    segs
}

/// The inverse of [`split`] (an empty text block contributes no line).
pub fn join(segs: &[Seg]) -> String {
    let mut lines: Vec<&str> = Vec::new();
    for seg in segs {
        match seg {
            Seg::Text(t) => {
                if !t.is_empty() {
                    lines.extend(t.split('\n'));
                }
            }
            Seg::Image { line, .. } => lines.push(line),
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style() -> LiveStyle {
        LiveStyle { ink: Color32::BLACK, muted: Color32::GRAY, accent: Color32::BLUE, size: 16.0 }
    }

    #[test]
    fn layout_always_contains_exactly_the_text() {
        let docs = [
            "",
            "plain",
            "# Heading **bold** and *it*",
            "a **b ~~c~~ d** e\n\n- item `code` [link](http://x)\n1. one\n> quote\n---\n```\n**not styled**\n```\nend",
            "unclosed ** and * and ` and [x](",
            "héllo **wörld** ✓\n\n\n",
            "**",
            "****",
            "* * *",
        ];
        for doc in docs {
            for reveal in [None, Some((0, 0)), Some((0, 1000))] {
                let job = layout(doc, reveal, false, &style(), 400.0);
                assert_eq!(job.text, doc, "doc {doc:?} reveal {reveal:?}");
            }
            assert_eq!(layout(doc, None, true, &style(), 400.0).text, doc);
        }
    }

    #[test]
    fn marks_are_hidden_unless_revealed() {
        let doc = "x **bold** y\nnext";
        let sizes = |reveal| {
            layout(doc, reveal, false, &style(), 400.0)
                .sections
                .iter()
                .map(|s| (s.byte_range.clone(), s.format.font_id.size))
                .collect::<Vec<_>>()
        };
        let hidden = sizes(None);
        let marker = hidden.iter().find(|(r, _)| doc[usize::from(r.start)..usize::from(r.end)] == *"**").unwrap();
        assert_eq!(marker.1, 1.0);
        // Cursor on the first line: marks shown at a normal size.
        let shown = sizes(Some((3, 3)));
        let marker = shown.iter().find(|(r, _)| doc[usize::from(r.start)..usize::from(r.end)] == *"**").unwrap();
        assert!(marker.1 > 10.0);
        // Cursor on the other line: still hidden.
        let other = sizes(Some((14, 14)));
        let marker = other.iter().find(|(r, _)| doc[usize::from(r.start)..usize::from(r.end)] == *"**").unwrap();
        assert_eq!(marker.1, 1.0);
    }

    #[test]
    fn bold_text_uses_the_bold_family() {
        let job = layout("a **b** c", None, false, &style(), 400.0);
        let bold = job.sections.iter().find(|s| &job.text[usize::from(s.byte_range.start)..usize::from(s.byte_range.end)] == "b").unwrap();
        assert_eq!(bold.format.font_id.family, bold_family());
    }

    #[test]
    fn parses_image_lines() {
        assert_eq!(image_line("![cat](img:i12)"), Some(("cat".into(), "i12".into())));
        assert_eq!(image_line("  ![](img:i1)  "), Some(("".into(), "i1".into())));
        assert_eq!(image_line("![x](https://a/b.png)"), None);
        assert_eq!(image_line("text ![x](img:i1)"), None);
        assert_eq!(image_line("![x](img:)"), None);
    }

    #[test]
    fn split_and_join_round_trip() {
        let docs = [
            "",
            "hello",
            "a\n![x](img:i1)\nb",
            "![x](img:i1)",
            "![x](img:i1)\n![y](img:i2)",
            "text\n\n![x](img:i1)\n\nmore\n![y](img:i2)\n",
            "\n\n",
        ];
        for doc in docs {
            let segs = split(doc);
            assert!(matches!(segs.first(), Some(Seg::Text(_))) && matches!(segs.last(), Some(Seg::Text(_))));
            // Pictures never touch: a text block sits between them.
            for w in segs.windows(2) {
                assert!(!matches!((&w[0], &w[1]), (Seg::Image { .. }, Seg::Image { .. })));
            }
            // Splitting again after joining changes nothing (idempotent), though a lone empty
            // line next to a picture may collapse.
            let joined = join(&segs);
            assert_eq!(split(&joined), segs, "doc {doc:?}");
        }
        assert_eq!(join(&split("a\n![x](img:i1)\nb")), "a\n![x](img:i1)\nb");
        assert_eq!(join(&split("hello")), "hello");
    }
}
