//! Rich text for diary entries: entries are stored as Markdown, rendered with egui, and edited
//! through a toolbar that inserts Markdown syntax.
//!
//! Images are stored inside the (encrypted) diary data and referenced as `![name](img:ID)`.

use std::collections::{BTreeMap, HashMap};

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use eframe::egui::{
    self, Color32, FontFamily, FontId, Label, Stroke, TextureHandle, TextureOptions,
    text::{LayoutJob, TextFormat},
};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

pub const IMAGE_SCHEME: &str = "img:";

// ---------------------------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub(crate) struct Fmt {
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) strike: bool,
    pub(crate) code: bool,
    pub(crate) link: bool,
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Inline {
    Text(String, Fmt),
    Break,
    Image { src: String, alt: String },
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Block {
    Para(Vec<Inline>),
    Heading(u8, Vec<Inline>),
    Quote(Vec<Block>),
    List { start: Option<u64>, items: Vec<Vec<Block>> },
    Code(String),
    Rule,
}

struct Cursor<'a> {
    events: Vec<Event<'a>>,
    i: usize,
}

impl<'a> Cursor<'a> {
    fn peek(&self) -> Option<&Event<'a>> {
        self.events.get(self.i)
    }

    fn next(&mut self) -> Option<Event<'a>> {
        let e = self.events.get(self.i).cloned();
        if e.is_some() {
            self.i += 1;
        }
        e
    }

    /// Does the next event end or start a block, i.e. stop an inline run?
    fn at_block_boundary(&self) -> bool {
        match self.peek() {
            None | Some(Event::End(_)) | Some(Event::Rule) => true,
            Some(Event::Start(t)) => !matches!(
                t,
                Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. } | Tag::Image { .. }
            ),
            _ => false,
        }
    }

    fn inlines(&mut self, fmt: Fmt) -> Vec<Inline> {
        let mut out = Vec::new();
        while !self.at_block_boundary() {
            match self.next() {
                Some(Event::Text(t) | Event::Html(t) | Event::InlineHtml(t)) => out.push(Inline::Text(t.to_string(), fmt)),
                Some(Event::Code(t)) => out.push(Inline::Text(t.to_string(), Fmt { code: true, ..fmt })),
                Some(Event::SoftBreak | Event::HardBreak) => out.push(Inline::Break),
                Some(Event::Start(Tag::Strong)) => self.nested(&mut out, Fmt { bold: true, ..fmt }),
                Some(Event::Start(Tag::Emphasis)) => self.nested(&mut out, Fmt { italic: true, ..fmt }),
                Some(Event::Start(Tag::Strikethrough)) => self.nested(&mut out, Fmt { strike: true, ..fmt }),
                Some(Event::Start(Tag::Link { .. })) => self.nested(&mut out, Fmt { link: true, ..fmt }),
                Some(Event::Start(Tag::Image { dest_url, .. })) => {
                    let mut alt = String::new();
                    while let Some(e) = self.next() {
                        match e {
                            Event::Text(t) | Event::Code(t) => alt.push_str(&t),
                            Event::End(TagEnd::Image) => break,
                            _ => {}
                        }
                    }
                    out.push(Inline::Image { src: dest_url.to_string(), alt });
                }
                _ => {}
            }
        }
        out
    }

    fn nested(&mut self, out: &mut Vec<Inline>, fmt: Fmt) {
        let inner = self.inlines(fmt);
        out.extend(inner);
        // Consume the matching End tag.
        if matches!(self.peek(), Some(Event::End(_))) {
            self.i += 1;
        }
    }

    fn eat_end(&mut self) {
        if matches!(self.peek(), Some(Event::End(_))) {
            self.i += 1;
        }
    }

    /// Parse blocks until the end of the enclosing container (or the end of input).
    fn blocks(&mut self) -> Vec<Block> {
        let mut out = Vec::new();
        loop {
            match self.peek() {
                None => break,
                Some(Event::End(_)) => {
                    self.i += 1;
                    break;
                }
                Some(Event::Rule) => {
                    self.i += 1;
                    out.push(Block::Rule);
                }
                Some(Event::Start(tag)) => match tag.clone() {
                    Tag::Paragraph => {
                        self.i += 1;
                        let inl = self.inlines(Fmt::default());
                        self.eat_end();
                        out.push(Block::Para(inl));
                    }
                    Tag::Heading { level, .. } => {
                        self.i += 1;
                        let inl = self.inlines(Fmt::default());
                        self.eat_end();
                        out.push(Block::Heading(level as u8, inl));
                    }
                    Tag::BlockQuote(_) => {
                        self.i += 1;
                        out.push(Block::Quote(self.blocks()));
                    }
                    Tag::List(start) => {
                        self.i += 1;
                        let mut items = Vec::new();
                        loop {
                            match self.peek() {
                                Some(Event::Start(Tag::Item)) => {
                                    self.i += 1;
                                    items.push(self.blocks());
                                }
                                Some(Event::End(_)) => {
                                    self.i += 1;
                                    break;
                                }
                                None => break,
                                _ => self.i += 1,
                            }
                        }
                        out.push(Block::List { start, items });
                    }
                    Tag::CodeBlock(_) => {
                        self.i += 1;
                        let mut code = String::new();
                        while let Some(e) = self.next() {
                            match e {
                                Event::Text(t) => code.push_str(&t),
                                Event::End(_) => break,
                                _ => {}
                            }
                        }
                        out.push(Block::Code(code.trim_end_matches('\n').to_string()));
                    }
                    Tag::HtmlBlock => self.i += 1,
                    // Inline content directly inside a tight list item.
                    Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. } | Tag::Image { .. } => {
                        let inl = self.inlines(Fmt::default());
                        out.push(Block::Para(inl));
                    }
                    // Anything unsupported: step over the start tag, keep its text.
                    _ => self.i += 1,
                },
                Some(_) => {
                    let before = self.i;
                    let inl = self.inlines(Fmt::default());
                    if !inl.is_empty() {
                        out.push(Block::Para(inl));
                    }
                    if self.i == before {
                        self.i += 1;
                    }
                }
            }
        }
        out
    }
}

pub(crate) fn parse(md: &str) -> Vec<Block> {
    let events: Vec<Event> = Parser::new_ext(md, Options::ENABLE_STRIKETHROUGH).collect();
    Cursor { events, i: 0 }.blocks()
}

/// Plain-text one-liner for the calendar: the first paragraph, heading or list item.
pub fn summary(md: &str) -> String {
    fn flatten(inl: &[Inline]) -> String {
        let mut s = String::new();
        for i in inl {
            match i {
                Inline::Text(t, _) => s.push_str(t),
                Inline::Break => s.push(' '),
                Inline::Image { alt, .. } => s.push_str(if alt.is_empty() { "[image]" } else { alt }),
            }
        }
        s
    }
    fn first(blocks: &[Block]) -> Option<String> {
        for b in blocks {
            let s = match b {
                Block::Para(i) | Block::Heading(_, i) => Some(flatten(i)),
                Block::Quote(b) => first(b),
                Block::List { items, .. } => items.iter().find_map(|i| first(i)),
                Block::Code(c) => Some(c.lines().next().unwrap_or("").to_string()),
                Block::Rule => None,
            };
            if let Some(s) = s.filter(|s| !s.trim().is_empty()) {
                return Some(s.trim().to_string());
            }
        }
        None
    }
    first(&parse(md)).unwrap_or_default()
}

/// Ids of the images an entry refers to.
pub fn image_ids(md: &str) -> Vec<String> {
    Parser::new(md)
        .filter_map(|e| match e {
            Event::Start(Tag::Image { dest_url, .. }) => dest_url.strip_prefix(IMAGE_SCHEME).map(str::to_string),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------------------------

pub struct Style {
    pub text: Color32,
    pub link: Color32,
    pub size: f32,
}

/// Decoded images turned into textures, created on demand.
#[derive(Default)]
pub struct Media {
    textures: HashMap<String, Option<TextureHandle>>,
}

impl Media {
    fn texture(&mut self, ctx: &egui::Context, images: &BTreeMap<String, String>, id: &str) -> Option<TextureHandle> {
        self.textures
            .entry(id.to_string())
            .or_insert_with(|| {
                let bytes = B64.decode(images.get(id)?).ok()?;
                let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
                let size = [img.width() as usize, img.height() as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
                Some(ctx.load_texture(format!("diary-{id}"), color, TextureOptions::LINEAR))
            })
            .clone()
    }
}

pub fn bold_family() -> FontFamily {
    FontFamily::Name("bold".into())
}

fn job_for(inlines: &[Inline], style: &Style, size: f32, force_bold: bool) -> LayoutJob {
    let mut job = LayoutJob::default();
    for inl in inlines {
        let (text, fmt) = match inl {
            Inline::Text(t, f) => (t.as_str(), *f),
            Inline::Break => ("\n", Fmt::default()),
            Inline::Image { .. } => continue,
        };
        let family = if fmt.code {
            FontFamily::Monospace
        } else if fmt.bold || force_bold {
            bold_family()
        } else {
            FontFamily::Proportional
        };
        let color = if fmt.link { style.link } else { style.text };
        let fmt_out = TextFormat {
            font_id: FontId::new(if fmt.code { size * 0.92 } else { size }, family),
            color,
            italics: fmt.italic,
            strikethrough: if fmt.strike { Stroke::new(1.0, color) } else { Stroke::NONE },
            underline: if fmt.link { Stroke::new(1.0, color) } else { Stroke::NONE },
            background: if fmt.code { style.text.gamma_multiply(0.13) } else { Color32::TRANSPARENT },
            ..Default::default()
        };
        job.append(text, 0.0, fmt_out);
    }
    job
}

fn render_inlines(
    ui: &mut egui::Ui,
    inlines: &[Inline],
    style: &Style,
    size: f32,
    bold: bool,
    media: &mut Media,
    images: &BTreeMap<String, String>,
) {
    // Text runs are laid out together; images interrupt them.
    let mut run: Vec<Inline> = Vec::new();
    let flush = |ui: &mut egui::Ui, run: &mut Vec<Inline>| {
        if !run.is_empty() {
            ui.add(Label::new(job_for(run, style, size, bold)));
            run.clear();
        }
    };
    for inl in inlines {
        match inl {
            Inline::Image { src, alt } => {
                flush(ui, &mut run);
                show_image(ui, media, images, src, alt, style.text);
            }
            other => run.push(other.clone()),
        }
    }
    flush(ui, &mut run);
}

fn render_blocks(
    ui: &mut egui::Ui,
    blocks: &[Block],
    style: &Style,
    media: &mut Media,
    images: &BTreeMap<String, String>,
    depth: usize,
) {
    for (n, block) in blocks.iter().enumerate() {
        if n > 0 {
            ui.add_space(5.0);
        }
        match block {
            Block::Para(inl) => render_inlines(ui, inl, style, style.size, false, media, images),
            Block::Heading(level, inl) => {
                let k = match level {
                    1 => 1.7,
                    2 => 1.45,
                    3 => 1.25,
                    4 => 1.12,
                    _ => 1.0,
                };
                render_inlines(ui, inl, style, style.size * k, true, media, images);
            }
            Block::Quote(inner) => {
                ui.indent(("quote", depth, n), |ui| render_blocks(ui, inner, style, media, images, depth + 1));
            }
            Block::List { start, items } => {
                for (i, item) in items.iter().enumerate() {
                    ui.horizontal_top(|ui| {
                        let marker = match start {
                            Some(s) => format!("{}.", s + i as u64),
                            None => "\u{2022}".to_string(),
                        };
                        ui.add(Label::new(egui::RichText::new(marker).size(style.size).color(style.text)));
                        ui.vertical(|ui| render_blocks(ui, item, style, media, images, depth + 1));
                    });
                }
            }
            Block::Code(code) => {
                egui::Frame::new().fill(style.text.gamma_multiply(0.1)).corner_radius(6).inner_margin(8).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(Label::new(egui::RichText::new(code).monospace().size(style.size * 0.92).color(style.text)));
                });
            }
            Block::Rule => {
                ui.separator();
            }
        }
    }
}

/// Draw `md` into `ui`.
pub fn render(ui: &mut egui::Ui, md: &str, style: &Style, media: &mut Media, images: &BTreeMap<String, String>) {
    // Treat every line break as a line break (plain-text entries keep their layout).
    let md = md.replace("\r\n", "\n").replace('\n', "  \n");
    let blocks = parse(&md);
    ui.vertical(|ui| render_blocks(ui, &blocks, style, media, images, 0));
}

// ---------------------------------------------------------------------------------------------
// Editing helpers (toolbar actions)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Format {
    Bold,
    Italic,
    Strike,
    Code,
    Heading(u8),
    Bullet,
    Numbered,
    Quote,
}

/// Apply `format` to the selection (char indices, start <= end). Returns the new selection.
pub fn apply(text: &mut String, sel: (usize, usize), format: Format) -> (usize, usize) {
    let mut chars: Vec<char> = text.chars().collect();
    let (a, b) = (sel.0.min(chars.len()), sel.1.min(chars.len()));
    let (a, b) = (a.min(b), a.max(b));

    let result = match format {
        Format::Bold => wrap(&mut chars, a, b, "**"),
        Format::Italic => wrap(&mut chars, a, b, "*"),
        Format::Strike => wrap(&mut chars, a, b, "~~"),
        Format::Code => wrap(&mut chars, a, b, "`"),
        Format::Heading(n) => prefix_lines(&mut chars, a, b, |_| format!("{} ", "#".repeat(n as usize))),
        Format::Bullet => prefix_lines(&mut chars, a, b, |_| "- ".to_string()),
        Format::Numbered => prefix_lines(&mut chars, a, b, |i| format!("{}. ", i + 1)),
        Format::Quote => prefix_lines(&mut chars, a, b, |_| "> ".to_string()),
    };
    *text = chars.into_iter().collect();
    result
}

fn wrap(chars: &mut Vec<char>, a: usize, b: usize, marker: &str) -> (usize, usize) {
    let m: Vec<char> = marker.chars().collect();
    let n = m.len();
    let wrapped = a >= n && chars[a - n..a] == m[..] && chars.len() >= b + n && chars[b..b + n] == m[..];
    if wrapped {
        // Already wrapped: remove the markers.
        chars.drain(b..b + n);
        chars.drain(a - n..a);
        (a - n, b - n)
    } else {
        for (k, c) in m.iter().enumerate() {
            chars.insert(b + k, *c);
        }
        for (k, c) in m.iter().enumerate() {
            chars.insert(a + k, *c);
        }
        (a + n, b + n)
    }
}

/// Add (or, if every line already has it, remove) a line prefix on each selected line.
fn prefix_lines(chars: &mut Vec<char>, a: usize, b: usize, prefix: impl Fn(usize) -> String) -> (usize, usize) {
    let start = chars[..a].iter().rposition(|c| *c == '\n').map_or(0, |p| p + 1);
    let end = chars[b..].iter().position(|c| *c == '\n').map_or(chars.len(), |p| b + p);
    let block: String = chars[start..end].iter().collect();
    let lines: Vec<&str> = block.split('\n').collect();

    let strip = |line: &str| -> (usize, String) {
        // Existing list/quote/heading prefix length (in chars) and the rest of the line.
        let t = line.trim_start_matches('#');
        let hashes = line.len() - t.len();
        if hashes > 0 && hashes <= 6 && t.starts_with(' ') {
            return (hashes + 1, t[1..].to_string());
        }
        for p in ["- ", "* ", "> "] {
            if let Some(rest) = line.strip_prefix(p) {
                return (p.len(), rest.to_string());
            }
        }
        let digits = line.chars().take_while(char::is_ascii_digit).count();
        if digits > 0 && line[digits..].starts_with(". ") {
            return (digits + 2, line[digits + 2..].to_string());
        }
        (0, line.to_string())
    };

    let all_have = lines.iter().enumerate().all(|(i, l)| {
        let want = prefix(i);
        l.starts_with(&want)
    });
    let new_lines: Vec<String> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            if all_have {
                strip(l).1
            } else {
                format!("{}{}", prefix(i), strip(l).1)
            }
        })
        .collect();
    let joined = new_lines.join("\n");
    let new_len = joined.chars().count();
    chars.splice(start..end, joined.chars());
    (start, start + new_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(md: &str) -> Vec<Block> {
        parse(md)
    }

    #[test]
    fn parses_inline_formats() {
        let b = t("a **b** *c* ~~d~~ `e`");
        let Block::Para(inl) = &b[0] else { panic!("{b:?}") };
        let has = |pred: &dyn Fn(&Fmt) -> bool| inl.iter().any(|i| matches!(i, Inline::Text(_, f) if pred(f)));
        assert!(has(&|f| f.bold));
        assert!(has(&|f| f.italic));
        assert!(has(&|f| f.strike));
        assert!(has(&|f| f.code));
    }

    #[test]
    fn parses_blocks() {
        let b = t("# Title\n\n- one\n- two\n\n> quote\n\n1. a\n2. b\n\n```\ncode\n```\n\n---");
        assert!(matches!(b[0], Block::Heading(1, _)));
        assert!(matches!(&b[1], Block::List { start: None, items } if items.len() == 2));
        assert!(matches!(b[2], Block::Quote(_)));
        assert!(matches!(&b[3], Block::List { start: Some(1), items } if items.len() == 2));
        assert!(matches!(&b[4], Block::Code(c) if c == "code"));
        assert!(matches!(b[5], Block::Rule));
    }

    #[test]
    fn parses_images_and_ids() {
        let md = "hello\n\n![cat](img:i7)\n\n![x](https://example.com/a.png)";
        assert_eq!(image_ids(md), vec!["i7".to_string()]);
        assert!(t(md).iter().any(|b| matches!(b, Block::Para(i) if i.iter().any(|x| matches!(x, Inline::Image { src, .. } if src == "img:i7")))));
    }

    #[test]
    fn summaries() {
        assert_eq!(summary("# Big day\n\nmore"), "Big day");
        assert_eq!(summary("**bold** start"), "bold start");
        assert_eq!(summary("![pic](img:i1)"), "pic");
        assert_eq!(summary("- first\n- second"), "first");
        assert_eq!(summary(""), "");
    }

    #[test]
    fn wraps_and_unwraps() {
        let mut s = "hello world".to_string();
        let sel = apply(&mut s, (6, 11), Format::Bold);
        assert_eq!(s, "hello **world**");
        assert_eq!(sel, (8, 13));
        let sel = apply(&mut s, sel, Format::Bold);
        assert_eq!(s, "hello world");
        assert_eq!(sel, (6, 11));

        let mut e = "ab".to_string();
        assert_eq!(apply(&mut e, (1, 1), Format::Italic), (2, 2));
        assert_eq!(e, "a**b");
    }

    #[test]
    fn prefixes_lines() {
        let mut s = "one\ntwo\nthree".to_string();
        apply(&mut s, (0, 7), Format::Bullet);
        assert_eq!(s, "- one\n- two\nthree");
        apply(&mut s, (0, 11), Format::Bullet);
        assert_eq!(s, "one\ntwo\nthree");

        let mut s = "a\nb".to_string();
        apply(&mut s, (0, 3), Format::Numbered);
        assert_eq!(s, "1. a\n2. b");

        let mut s = "## old".to_string();
        apply(&mut s, (0, 0), Format::Heading(1));
        assert_eq!(s, "# old");
        apply(&mut s, (0, 0), Format::Heading(1));
        assert_eq!(s, "old");

        // Multi-byte characters don't break char/byte handling.
        let mut s = "héllo wörld".to_string();
        apply(&mut s, (6, 11), Format::Code);
        assert_eq!(s, "héllo `wörld`");
    }
}

/// Show the picture `src` (`img:ID`), scaled to fit; a note is shown if it can't be found.
pub fn show_image(
    ui: &mut egui::Ui,
    media: &mut Media,
    images: &BTreeMap<String, String>,
    src: &str,
    alt: &str,
    text: Color32,
) {
    let tex = src.strip_prefix(IMAGE_SCHEME).and_then(|id| media.texture(ui.ctx(), images, id));
    match tex {
        Some(tex) => {
            let natural = tex.size_vec2();
            let scale = (ui.available_width() / natural.x).min(520.0 / natural.y).min(1.0);
            ui.add_space(2.0);
            ui.image((tex.id(), natural * scale));
            ui.add_space(2.0);
        }
        None => {
            ui.add(Label::new(egui::RichText::new(format!("[missing image: {alt}]")).color(text)));
        }
    }
}

/// Number of words in an entry, ignoring Markdown marks and picture names.
pub fn word_count(md: &str) -> usize {
    let mut words = 0;
    let mut in_image = false;
    for event in Parser::new(md) {
        match event {
            Event::Start(Tag::Image { .. }) => in_image = true,
            Event::End(TagEnd::Image) => in_image = false,
            Event::Text(t) | Event::Code(t) if !in_image => words += t.split_whitespace().count(),
            _ => {}
        }
    }
    words
}

#[cfg(test)]
mod word_tests {
    use super::word_count;

    #[test]
    fn counts_words_not_marks_or_picture_names() {
        assert_eq!(word_count(""), 0);
        assert_eq!(word_count("one two  three"), 3);
        assert_eq!(word_count("# Title\n\n**bold** words and `code`\n\n- a\n- b"), 7);
        assert_eq!(word_count("![a long picture name](img:i1)\n\nafter"), 1);
        assert_eq!(word_count("line one\nline two"), 4);
    }
}
