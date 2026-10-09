//! Emoji-style faces for the 1–5 mood scale, from really sad to really happy.
//!
//! They are drawn here, pixel by pixel, rather than typed as emoji characters, because the UI
//! toolkit can only show emoji as flat one-colour glyphs. Each face is a small RGBA picture used
//! as a texture.

use eframe::egui::{self, Color32, TextureHandle, TextureOptions};

pub const MOODS: usize = 5;
const INK: [f32; 3] = [58.0, 38.0, 24.0];
const TEAR: [f32; 3] = [96.0, 165.0, 250.0];

/// Face colours: red (really sad) through amber to green (really happy).
const FILL: [[f32; 3]; MOODS] = [[239.0, 84.0, 84.0], [250.0, 140.0, 52.0], [250.0, 204.0, 21.0], [163.0, 213.0, 45.0], [74.0, 214.0, 110.0]];

type P = (f32, f32);

fn dist_point(p: P, c: P) -> f32 {
    (p.0 - c.0).hypot(p.1 - c.1)
}

fn dist_segment(p: P, a: P, b: P) -> f32 {
    let (ab, ap) = ((b.0 - a.0, b.1 - a.1), (p.0 - a.0, p.1 - a.1));
    let len2 = ab.0 * ab.0 + ab.1 * ab.1;
    let t = if len2 == 0.0 { 0.0 } else { ((ap.0 * ab.0 + ap.1 * ab.1) / len2).clamp(0.0, 1.0) };
    dist_point(p, (a.0 + ab.0 * t, a.1 + ab.1 * t))
}

fn dist_line(p: P, pts: &[P]) -> f32 {
    pts.windows(2).map(|w| dist_segment(p, w[0], w[1])).fold(f32::MAX, f32::min)
}

/// Is `p` inside the closed polygon?
fn inside(p: P, poly: &[P]) -> bool {
    let mut odd = false;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            odd = !odd;
        }
    }
    odd
}

/// A parabola-shaped mouth: `bend` > 0 is a smile (middle lower than the ends), < 0 a frown.
fn mouth(y: f32, half: f32, bend: f32) -> Vec<P> {
    (0..=24)
        .map(|i| {
            let x = -half + 2.0 * half * i as f32 / 24.0;
            let t = x / half;
            (x, y + bend * (1.0 - t * t))
        })
        .collect()
}

/// A rounded arc for a happy closed eye (an upside-down U).
fn happy_eye(cx: f32, cy: f32, r: f32) -> Vec<P> {
    (0..=12)
        .map(|i| {
            let a = (20.0 + 140.0 * i as f32 / 12.0).to_radians();
            (cx + r * a.cos(), cy - r * a.sin())
        })
        .collect()
}

fn over(acc: &mut [f32; 4], color: [f32; 3], a: f32) {
    let k = 1.0 - a;
    for i in 0..3 {
        acc[i] = color[i] * a + acc[i] * k;
    }
    acc[3] = a + acc[3] * k;
}

/// Straight-alpha RGBA pixels for the face of `mood` (1..=5), `size` pixels square.
pub fn face_rgba(mood: u8, size: usize) -> Vec<u8> {
    let mood = mood.clamp(1, 5);
    let m = usize::from(mood) - 1;
    let fill = FILL[m];
    let rim = [fill[0] * 0.55, fill[1] * 0.55, fill[2] * 0.55];
    let s = size as f32 / 2.0; // pixels per unit; the face lives in -1..1
    let line = 0.075; // half-thickness of the drawn lines, in units

    // Features, in face units (y grows downwards).
    let (left, right) = ((-0.36_f32, -0.16_f32), (0.36_f32, -0.16_f32));
    let eye_dots = [left, right];
    let (mouth_line, open_mouth): (Vec<P>, Option<Vec<P>>) = match mood {
        1 => (mouth(0.52, 0.34, -0.26), None),
        2 => (mouth(0.50, 0.32, -0.12), None),
        3 => (mouth(0.46, 0.28, 0.0), None),
        4 => (mouth(0.28, 0.36, 0.20), None),
        _ => {
            // A wide open grin: flat top, curved bottom.
            let mut shape = vec![(-0.44, 0.26), (0.44, 0.26)];
            shape.extend((0..=16).map(|i| {
                let x = 0.44 - 0.88 * i as f32 / 16.0;
                (x, 0.26 + 0.46 * (1.0 - (x / 0.44) * (x / 0.44)))
            }));
            (Vec::new(), Some(shape))
        }
    };
    let crying = mood == 1;
    let squeezed: Vec<Vec<P>> = if crying {
        // "> <" eyes.
        vec![
            vec![(-0.48, -0.34), (-0.22, -0.2), (-0.48, -0.06)],
            vec![(0.48, -0.34), (0.22, -0.2), (0.48, -0.06)],
        ]
    } else {
        Vec::new()
    };
    let happy: Vec<Vec<P>> = if mood == 5 { vec![happy_eye(left.0, left.1 + 0.06, 0.15), happy_eye(right.0, right.1 + 0.06, 0.15)] } else { Vec::new() };

    let mut rgba = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let p = ((x as f32 + 0.5) / s - 1.0, (y as f32 + 0.5) / s - 1.0);
            let cov = |d: f32| (0.5 - d * s).clamp(0.0, 1.0); // d in units -> pixels
            let mut acc = [0.0f32; 4];

            // Face disc with a darker rim.
            let r = dist_point(p, (0.0, 0.0));
            over(&mut acc, rim, cov(r - 0.96));
            over(&mut acc, fill, cov(r - 0.96 + 0.07));
            // A soft highlight on the upper left.
            let hl = dist_point(p, (-0.38, -0.55));
            over(&mut acc, [255.0, 255.0, 255.0], cov(hl - 0.22) * 0.18);

            // Tears (really sad only).
            if crying {
                for side in [-1.0_f32, 1.0] {
                    let top = (side * 0.5, 0.0);
                    let tear = dist_segment(p, top, (side * 0.5, 0.2)) - 0.09;
                    over(&mut acc, TEAR, cov(tear));
                }
            }
            // Eyes.
            if !crying && mood != 5 {
                for e in eye_dots {
                    over(&mut acc, INK, cov(dist_point(p, e) - 0.095));
                }
            }
            for eye in squeezed.iter().chain(happy.iter()) {
                over(&mut acc, INK, cov(dist_line(p, eye) - line));
            }
            // Mouth.
            if !mouth_line.is_empty() {
                over(&mut acc, INK, cov(dist_line(p, &mouth_line) - line));
            }
            if let Some(shape) = &open_mouth {
                let d = dist_line(p, &[shape.as_slice(), &shape[..1]].concat()) - 0.03;
                if inside(p, shape) {
                    over(&mut acc, [96.0, 36.0, 36.0], 1.0);
                    // A little tongue.
                    let tongue = dist_point(p, (0.0, 0.68)) - 0.2;
                    over(&mut acc, [240.0, 110.0, 120.0], cov(tongue));
                } else {
                    over(&mut acc, INK, cov(d));
                }
            }

            let a = acc[3];
            for c in &acc[..3] {
                rgba.push(if a > 0.0 { (c / a).clamp(0.0, 255.0).round() as u8 } else { 0 });
            }
            rgba.push((a.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }
    rgba
}

const TEXTURE_SIZE: usize = 96;

/// The face texture for `mood`, created the first time it is needed and then kept.
pub fn texture(ctx: &egui::Context, mood: u8) -> TextureHandle {
    let mood = mood.clamp(1, 5);
    let id = egui::Id::new(("mood-face", mood));
    if let Some(tex) = ctx.data(|d| d.get_temp::<TextureHandle>(id)) {
        return tex;
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([TEXTURE_SIZE, TEXTURE_SIZE], &face_rgba(mood, TEXTURE_SIZE));
    let tex = ctx.load_texture(format!("mood-face-{mood}"), image, TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, tex.clone()));
    tex
}

/// Paint the face for `mood` into `rect`; `strength` below 1.0 fades it.
pub fn paint(painter: &egui::Painter, ctx: &egui::Context, rect: egui::Rect, mood: u8, strength: f32) {
    let tint = Color32::WHITE.gamma_multiply(strength.clamp(0.0, 1.0));
    painter.image(texture(ctx, mood).id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), tint);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(rgba: &[u8], size: usize, x: usize, y: usize) -> [u8; 4] {
        let i = (y * size + x) * 4;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    }

    #[test]
    fn faces_are_round_coloured_and_different() {
        let size = 64;
        let faces: Vec<Vec<u8>> = (1..=5).map(|m| face_rgba(m, size)).collect();
        for (i, face) in faces.iter().enumerate() {
            assert_eq!(face.len(), size * size * 4);
            assert_eq!(pixel(face, size, 0, 0)[3], 0, "corners are transparent");
            // The forehead is the face colour (highlight aside), fully opaque.
            let forehead = pixel(face, size, 32, 8);
            assert_eq!(forehead[3], 255);
            let fill = FILL[i];
            assert!((f32::from(forehead[0]) - fill[0]).abs() < 40.0, "face {} is the right colour: {forehead:?}", i + 1);
        }
        for i in 0..5 {
            for j in i + 1..5 {
                assert_ne!(faces[i], faces[j], "faces {} and {} differ", i + 1, j + 1);
            }
        }
        // Out-of-range moods are clamped rather than panicking.
        assert_eq!(face_rgba(0, 16), face_rgba(1, 16));
        assert_eq!(face_rgba(9, 16), face_rgba(5, 16));
    }

    #[test]
    fn writes_a_contact_sheet_when_asked() {
        // Set DIARY_PDF_OUT to a folder to get faces.png for a visual check.
        let Some(dir) = std::env::var_os("DIARY_PDF_OUT") else { return };
        let (size, gap) = (128usize, 16usize);
        let width = 5 * size + 6 * gap;
        let mut sheet = image::RgbaImage::from_pixel(width as u32, (size + 2 * gap) as u32, image::Rgba([245, 247, 252, 255]));
        for m in 1..=5u8 {
            let face = image::RgbaImage::from_raw(size as u32, size as u32, face_rgba(m, size)).unwrap();
            image::imageops::overlay(&mut sheet, &face, (gap + (m as usize - 1) * (size + gap)) as i64, gap as i64);
        }
        sheet.save(std::path::Path::new(&dir).join("faces.png")).unwrap();
    }
}
