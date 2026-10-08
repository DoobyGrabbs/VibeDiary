//! Embeds the app icon in the Windows executable so it shows up in File Explorer and the taskbar.

#[path = "src/icon.rs"]
mod icon;

/// Build a multi-size .ico (32-bit BMP frames) from the procedural icon.
fn make_ico() -> Vec<u8> {
    let sizes = [16usize, 24, 32, 48, 64, 128, 256];
    let frames: Vec<Vec<u8>> = sizes
        .iter()
        .map(|&n| {
            let rgba = icon::rgba(n);
            let mut data = Vec::new();
            // BITMAPINFOHEADER (height is doubled: XOR bitmap + AND mask).
            data.extend(40u32.to_le_bytes());
            data.extend((n as i32).to_le_bytes());
            data.extend((n as i32 * 2).to_le_bytes());
            data.extend(1u16.to_le_bytes());
            data.extend(32u16.to_le_bytes());
            data.extend([0u8; 24]);
            // Pixels are BGRA, bottom row first.
            for y in (0..n).rev() {
                for x in 0..n {
                    let i = (y * n + x) * 4;
                    data.extend([rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
                }
            }
            // AND mask: all zero (alpha channel does the work); rows are padded to 4 bytes.
            let mask_row = n.div_ceil(32) * 4;
            data.extend(std::iter::repeat_n(0u8, mask_row * n));
            data
        })
        .collect();

    let mut ico = Vec::new();
    ico.extend([0u8, 0, 1, 0]);
    ico.extend((sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len();
    for (n, frame) in sizes.iter().zip(&frames) {
        let dim = if *n >= 256 { 0 } else { *n as u8 };
        ico.extend([dim, dim, 0, 0]);
        ico.extend(1u16.to_le_bytes());
        ico.extend(32u16.to_le_bytes());
        ico.extend((frame.len() as u32).to_le_bytes());
        ico.extend((offset as u32).to_le_bytes());
        offset += frame.len();
    }
    for frame in frames {
        ico.extend(frame);
    }
    ico
}

fn main() {
    println!("cargo:rerun-if-changed=src/icon.rs");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let path = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("diary.ico");
    std::fs::write(&path, make_ico()).unwrap();

    let mut res = winresource::WindowsResource::new();
    res.set_icon(path.to_str().unwrap());
    res.set("ProductName", "My Diary");
    res.set("FileDescription", "My Diary");
    // Not fatal: the app still runs (with the default exe icon) if resource compilation fails.
    if let Err(e) = res.compile() {
        println!("cargo:warning=could not embed the exe icon: {e}");
    }
}
