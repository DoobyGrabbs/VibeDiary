//! Draws the application icon procedurally (a calendar card with colourful entry pills).

/// The icon is designed on a 256x256 grid and scaled to whatever size is asked for.
const DESIGN: f32 = 256.0;

/// Signed distance to a rounded rectangle (negative inside).
fn rrect(px: f32, py: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = (px - cx).abs() - hw + r;
    let qy = (py - cy).abs() - hh + r;
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
}

fn coverage(d: f32) -> f32 {
    (0.5 - d).clamp(0.0, 1.0)
}

/// Composite `color` with alpha `a` over the premultiplied accumulator.
fn over(acc: &mut [f32; 4], color: [f32; 3], a: f32) {
    let k = 1.0 - a;
    for i in 0..3 {
        acc[i] = color[i] * a + acc[i] * k;
    }
    acc[3] = a + acc[3] * k;
}

fn lerp(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Straight-alpha RGBA pixels for a square icon of `size` pixels.
pub fn rgba(size: usize) -> Vec<u8> {
    let s = size as f32 / DESIGN;
    let cov = |d: f32| coverage(d * s);
    let rgb = |r: f32, g: f32, b: f32| [r / 255.0, g / 255.0, b / 255.0];
    let (top, bottom) = (rgb(96.0, 165.0, 250.0), rgb(30.0, 64.0, 175.0));
    let white = rgb(255.0, 255.0, 255.0);
    let band = rgb(244.0, 63.0, 94.0);
    let ring = rgb(30.0, 41.0, 59.0);
    // (left edge, half width, centre y, colour)
    let pills = [
        (74.0, 54.0, 114.0, rgb(251.0, 191.0, 36.0)),
        (74.0, 38.0, 142.0, rgb(74.0, 222.0, 128.0)),
        (74.0, 46.0, 170.0, rgb(192.0, 132.0, 252.0)),
    ];

    let mut rgba = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let (px, py) = ((x as f32 + 0.5) / s, (y as f32 + 0.5) / s);
            let mut acc = [0.0f32; 4];

            // Rounded-square background with a diagonal gradient.
            let bg = cov(rrect(px, py, 128.0, 128.0, 126.0, 126.0, 58.0));
            over(&mut acc, lerp(top, bottom, (px + py) / (2.0 * DESIGN)), bg);

            // Soft shadow under the card.
            let shadow = cov(rrect(px, py, 128.0, 142.0, 78.0, 76.0, 18.0)) * 0.28;
            over(&mut acc, ring, shadow * bg);

            // White card with a coloured header band.
            let card = cov(rrect(px, py, 128.0, 134.0, 78.0, 76.0, 18.0));
            over(&mut acc, white, card);
            let band_cov = card * ((88.0 - py) * s).clamp(0.0, 1.0);
            over(&mut acc, band, band_cov);

            // Binder rings.
            for rx in [92.0, 164.0] {
                over(&mut acc, ring, cov(rrect(px, py, rx, 62.0, 9.0, 17.0, 9.0)));
                over(&mut acc, white, cov(rrect(px, py, rx, 62.0, 6.0, 14.0, 6.0)));
            }

            // Entry pills.
            for (left, hw, cy, color) in pills {
                over(&mut acc, color, cov(rrect(px, py, left + hw, cy, hw, 9.0, 9.0)));
            }

            // Back to straight alpha for the icon data.
            let a = acc[3];
            for c in &acc[..3] {
                let v = if a > 0.0 { c / a } else { 0.0 };
                rgba.push((v.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
            rgba.push((a.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }
    rgba
}
