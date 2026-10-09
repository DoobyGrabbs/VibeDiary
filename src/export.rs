//! Export entries to a PDF. Entries are written day by day with their formatting (headings, bold,
//! italic, lists, quotes, code) and, optionally, their pictures.
//!
//! The PDF is **not** encrypted, so it is a plain-text copy of what was written.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use base64::{Engine, engine::general_purpose::STANDARD as B64};
use chrono::NaiveDate;
use genpdf::{
    Alignment, Document, Element, Margins, PaperSize, SimplePageDecorator,
    elements::{self, Break, FramedElement, LinearLayout, Paragraph},
    fonts::{Font, FontData, FontFamily},
    style::{Color, Style, StyledString},
};

use crate::markdown::{Block, Fmt, IMAGE_SCHEME, Inline, parse};
use crate::{Category, Entry};

pub struct Report<'a> {
    pub title: String,
    pub subtitle: String,
    pub entries: Vec<&'a Entry>,
    pub include_images: bool,
}

const BODY: u8 = 11;
const BLUE: Color = Color::Rgb(30, 64, 175);
const GREY: Color = Color::Greyscale(110);

fn fonts_dir() -> PathBuf {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:/Windows".into());
    Path::new(&windir).join("Fonts")
}

/// Load a four-file font family (regular, bold, italic, bold italic) from the system fonts.
fn load_family(files: [&str; 4]) -> Option<FontFamily<FontData>> {
    let dir = fonts_dir();
    let load = |i: usize| FontData::new(std::fs::read(dir.join(files[i])).ok()?, None).ok();
    Some(FontFamily { regular: load(0)?, bold: load(1)?, italic: load(2)?, bold_italic: load(3)? })
}

const SEGOE: [&str; 4] = ["segoeui.ttf", "segoeuib.ttf", "segoeuii.ttf", "segoeuiz.ttf"];
const ARIAL: [&str; 4] = ["arial.ttf", "arialbd.ttf", "ariali.ttf", "arialbi.ttf"];
const CONSOLAS: [&str; 4] = ["consola.ttf", "consolab.ttf", "consolai.ttf", "consolaz.ttf"];
const COURIER_NEW: [&str; 4] = ["cour.ttf", "courbd.ttf", "couri.ttf", "courbi.ttf"];

struct Ctx<'a> {
    mono: FontFamily<Font>,
    images: &'a BTreeMap<String, String>,
    include_images: bool,
}

fn run_style(base: Style, fmt: &Fmt, ctx: &Ctx) -> Style {
    let mut s = base;
    if fmt.bold {
        s = s.bold();
    }
    if fmt.italic {
        s = s.italic();
    }
    if fmt.code {
        s = s.with_font_family(ctx.mono).with_color(Color::Rgb(100, 40, 130));
    }
    if fmt.link {
        s = s.with_color(Color::Rgb(37, 99, 235));
    }
    if fmt.strike {
        s = s.with_color(GREY); // the PDF library can't strike text through, so it is greyed out
    }
    s
}

fn clean(text: &str) -> String {
    text.replace('\t', "    ").replace('\r', "")
}

/// Longest side, in pixels, that pictures are shrunk to in the PDF (it stores them uncompressed,
/// so big pictures would make huge files).
const PDF_IMAGE_MAX: u32 = 900;

/// Prepare a stored picture for the PDF library: no transparency (it can't cope with it), shrunk,
/// and re-encoded. Returns the encoded bytes and the width in pixels.
fn prepare_picture(bytes: &[u8]) -> Option<(Vec<u8>, u32)> {
    let mut img = image::load_from_memory(bytes).ok()?;
    if img.width() > PDF_IMAGE_MAX || img.height() > PDF_IMAGE_MAX {
        img = img.resize(PDF_IMAGE_MAX, PDF_IMAGE_MAX, image::imageops::FilterType::Triangle);
    }
    // Flatten any transparency onto white paper.
    let rgba = img.to_rgba8();
    let mut rgb = image::RgbImage::new(rgba.width(), rgba.height());
    for (x, y, p) in rgba.enumerate_pixels() {
        let a = u32::from(p[3]);
        let on_white = |c: u8| ((u32::from(c) * a + 255 * (255 - a)) / 255) as u8;
        rgb.put_pixel(x, y, image::Rgb([on_white(p[0]), on_white(p[1]), on_white(p[2])]));
    }
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 88).encode_image(&rgb).ok()?;
    Some((out, rgb.width()))
}

/// A picture as a PDF element, or a short note if it can't be shown.
fn picture(src: &str, alt: &str, ctx: &Ctx, out: &mut LinearLayout) {
    let note = |text: String| Paragraph::new(StyledString::new(text, Style::new().italic().with_color(GREY)));
    if !ctx.include_images {
        out.push(note(format!("[picture: {}]", if alt.is_empty() { "untitled" } else { alt })));
        return;
    }
    let image = src
        .strip_prefix(IMAGE_SCHEME)
        .and_then(|id| ctx.images.get(id))
        .and_then(|b64| B64.decode(b64).ok())
        .and_then(|bytes| prepare_picture(&bytes))
        .and_then(|(bytes, width)| {
            // About 150 dpi, but never wider than 130 mm.
            let target_mm = (f64::from(width) * 25.4 / 150.0).min(130.0);
            let dpi = f64::from(width) * 25.4 / target_mm;
            elements::Image::from_reader(std::io::Cursor::new(bytes)).ok().map(|i| i.with_dpi(dpi))
        });
    match image {
        Some(img) => {
            out.push(Break::new(0.3));
            out.push(img.with_alignment(Alignment::Left));
            out.push(Break::new(0.3));
        }
        None => out.push(note("[picture could not be shown]".into())),
    }
}

/// The PDF library stores fonts and pictures uncompressed; compressing the streams makes the
/// file much smaller. If anything goes wrong the original bytes are used.
fn compress(pdf: Vec<u8>) -> Vec<u8> {
    let Ok(mut doc) = lopdf::Document::load_mem(&pdf) else { return pdf };
    doc.compress();
    let mut out = Vec::new();
    match doc.save_to(&mut out) {
        Ok(_) if out.starts_with(b"%PDF") && out.len() < pdf.len() => out,
        _ => pdf,
    }
}


fn inlines(inl: &[Inline], base: Style, ctx: &Ctx, out: &mut LinearLayout) {
    let mut para: Option<Paragraph> = None;
    for i in inl {
        match i {
            Inline::Text(t, fmt) => {
                para.get_or_insert_with(Paragraph::default).push_styled(clean(t), run_style(base, fmt, ctx));
            }
            Inline::Break => match para.take() {
                Some(p) => out.push(p),
                None => out.push(Break::new(1)),
            },
            Inline::Image { src, alt } => {
                if let Some(p) = para.take() {
                    out.push(p);
                }
                picture(src, alt, ctx, out);
            }
        }
    }
    if let Some(p) = para {
        out.push(p);
    }
}

fn blocks(bl: &[Block], ctx: &Ctx) -> LinearLayout {
    let mut out = LinearLayout::vertical();
    for (n, block) in bl.iter().enumerate() {
        if n > 0 {
            out.push(Break::new(0.4));
        }
        match block {
            Block::Para(inl) => inlines(inl, Style::new(), ctx, &mut out),
            Block::Heading(level, inl) => {
                let size = match level {
                    1 => 18,
                    2 => 15,
                    3 => 13,
                    _ => 12,
                };
                inlines(inl, Style::new().bold().with_font_size(size), ctx, &mut out);
            }
            Block::Quote(inner) => {
                let quoted = blocks(inner, ctx).styled(Style::new().italic().with_color(Color::Greyscale(80)));
                out.push(quoted.padded(Margins::trbl(0, 0, 0, 6)));
            }
            Block::List { start: None, items } => {
                let mut list = elements::UnorderedList::new();
                for item in items {
                    list.push(blocks(item, ctx));
                }
                out.push(list);
            }
            Block::List { start: Some(start), items } => {
                let mut list = elements::OrderedList::with_start(*start as usize);
                for item in items {
                    list.push(blocks(item, ctx));
                }
                out.push(list);
            }
            Block::Code(code) => {
                let mut lines = LinearLayout::vertical();
                for line in code.lines() {
                    if line.trim().is_empty() {
                        lines.push(Break::new(1));
                    } else {
                        lines.push(Paragraph::new(StyledString::new(
                            clean(line),
                            Style::new().with_font_family(ctx.mono).with_font_size(BODY - 1),
                        )));
                    }
                }
                out.push(lines.padded(1.5).framed());
            }
            Block::Rule => out.push(FramedElement::new(Break::new(0.05))),
        }
    }
    out
}

/// Build the PDF and return its bytes.
pub fn build_pdf(report: &Report, cats: &[Category], images: &BTreeMap<String, String>) -> Result<Vec<u8>, String> {
    let text = load_family(SEGOE)
        .or_else(|| load_family(ARIAL))
        .ok_or("Couldn't find a font to use (Segoe UI or Arial).")?;
    // A monospace font is only added (it makes the PDF bigger) when some entry uses code.
    let wants_code = report.entries.iter().any(|e| e.text.contains('`'));
    let mono = if wants_code { load_family(CONSOLAS).or_else(|| load_family(COURIER_NEW)) } else { None };

    let mut doc = Document::new(text);
    let mono = match mono {
        Some(family) => doc.add_font_family(family),
        None => doc.font_cache().default_font_family(),
    };
    doc.set_title(report.title.clone());
    doc.set_minimal_conformance();
    doc.set_paper_size(PaperSize::A4);
    doc.set_font_size(BODY);
    doc.set_line_spacing(1.2);
    let mut decorator = SimplePageDecorator::new();
    decorator.set_margins(Margins::trbl(16, 18, 16, 18));
    decorator.set_header(|page| {
        Paragraph::new(StyledString::new(format!("{page}"), Style::new().with_font_size(9).with_color(GREY))).aligned(Alignment::Right)
    });
    doc.set_page_decorator(decorator);

    let ctx = Ctx { mono, images, include_images: report.include_images };
    doc.push(Paragraph::new(StyledString::new(report.title.clone(), Style::new().bold().with_font_size(24).with_color(BLUE))));
    doc.push(Paragraph::new(StyledString::new(report.subtitle.clone(), Style::new().with_font_size(10).with_color(GREY))));
    doc.push(Break::new(1.5));

    let mut by_day: BTreeMap<NaiveDate, Vec<&Entry>> = BTreeMap::new();
    for e in &report.entries {
        if let Ok(d) = NaiveDate::parse_from_str(&e.date, "%Y-%m-%d") {
            by_day.entry(d).or_default().push(e);
        }
    }
    if by_day.is_empty() {
        doc.push(Paragraph::new("There are no entries in this range."));
    }
    for (day, mut entries) in by_day {
        entries.sort_by(|a, b| a.added_at.cmp(&b.added_at));
        doc.push(Paragraph::new(StyledString::new(
            day.format("%A %e %B %Y").to_string(),
            Style::new().bold().with_font_size(15).with_color(BLUE),
        )));
        doc.push(FramedElement::new(Break::new(0.05)));
        doc.push(Break::new(0.6));
        for e in entries {
            let rgb = cats.iter().find(|c| c.name == e.category).map_or([110, 110, 110], |c| c.color);
            // Darkened so light category colours stay readable on white paper.
            let tint = Color::Rgb((rgb[0] as f32 * 0.6) as u8, (rgb[1] as f32 * 0.6) as u8, (rgb[2] as f32 * 0.6) as u8);
            let mut head = format!("{}   {}", e.added_at.get(11..16).unwrap_or(""), e.category);
            if let Some(m) = e.mood.filter(|m| (1..=5).contains(m)) {
                head.push_str(&format!("   mood {m}/5"));
            }
            doc.push(Paragraph::new(StyledString::new(head, Style::new().bold().with_font_size(10).with_color(tint))));
            doc.push(Break::new(0.3));
            doc.push(blocks(&parse(&e.text.replace('\n', "  \n")), &ctx));
            doc.push(Break::new(1.4));
        }
        doc.push(Break::new(0.6));
    }

    let mut bytes = Vec::new();
    doc.render(&mut bytes).map_err(|e| format!("Couldn't create the PDF: {e}"))?;
    Ok(compress(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u64, date: &str, text: &str, mood: Option<u8>) -> Entry {
        Entry { id, date: date.into(), added_at: format!("{date} 09:30:00"), category: "Work".into(), text: text.into(), mood }
    }

    fn png_b64() -> String {
        let img = image::RgbaImage::from_pixel(40, 30, image::Rgba([30, 120, 200, 255]));
        let mut bytes = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        B64.encode(bytes)
    }

    #[test]
    fn builds_a_pdf_with_every_kind_of_content() {
        let md = "# Big heading\n\nSome **bold**, *italic*, ~~gone~~, `code` and a [link](http://x).\nA second line.\n\n\
                  - one\n- two\n  - nested\n\n1. first\n2. second\n\n> a quote\n\n```\nlet x = 1;\n\nlet y = 2;\n```\n\n---\n\n![pic](img:i1)\n\nThe end é ü ✓";
        let entries = vec![
            entry(1, "2026-10-08", md, Some(4)),
            entry(2, "2026-10-08", "Second entry", None),
            entry(3, "2026-10-09", "Another day\n\nwith two paragraphs", Some(2)),
        ];
        let cats = vec![Category { name: "Work".into(), color: [251, 191, 36] }];
        let images = BTreeMap::from([("i1".to_string(), png_b64())]);
        for include_images in [true, false] {
            let report = Report {
                title: "My Diary".into(),
                subtitle: "Test".into(),
                entries: entries.iter().collect(),
                include_images,
            };
            let pdf = build_pdf(&report, &cats, &images).expect("pdf");
            assert!(pdf.starts_with(b"%PDF"), "a PDF header");
            if let (Some(dir), true) = (std::env::var_os("DIARY_PDF_OUT"), include_images) {
                std::fs::write(std::path::Path::new(&dir).join("sample.pdf"), &pdf).unwrap();
            }
            assert!(pdf.len() > 3000, "has content: {} bytes", pdf.len());
            let has_picture = pdf.windows(6).any(|w| w == b"/Image");
            assert_eq!(has_picture, include_images, "pictures are included only when asked for");
        }
    }

    #[test]
    fn empty_and_missing_pictures_do_not_fail() {
        let report = Report { title: "Empty".into(), subtitle: String::new(), entries: Vec::new(), include_images: true };
        assert!(build_pdf(&report, &[], &BTreeMap::new()).unwrap().starts_with(b"%PDF"));

        let e = entry(1, "2026-01-01", "![gone](img:nope)\n\n![bad](img:i1)", None);
        let report = Report { title: "Pics".into(), subtitle: String::new(), entries: vec![&e], include_images: true };
        let images = BTreeMap::from([("i1".to_string(), "not base64!!".to_string())]);
        assert!(build_pdf(&report, &[], &images).unwrap().starts_with(b"%PDF"));
    }
}
