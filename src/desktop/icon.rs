//! Airth's icons, drawn in code: the app icon (a teal tile with an orbit and
//! a planet) and the menu bar icon (the same mark, as a template image).

/// RGBA pixels of the mark on a `size`×`size` canvas. `tile` draws the app
/// icon's rounded background; without it the mark is black on transparent
/// (macOS tints template images for light and dark menu bars).
pub fn draw(size: u32, tile: bool) -> Vec<u8> {
    let s = size as f32;
    let mut px = vec![0u8; (size * size * 4) as usize];
    // Coverage of a shape at a pixel, by 4×4 supersampling.
    let cover = |x: u32, y: u32, inside: &dyn Fn(f32, f32) -> bool| -> f32 {
        let mut n = 0;
        for i in 0..4 {
            for j in 0..4 {
                let fx = (x as f32 + (i as f32 + 0.5) / 4.0) / s;
                let fy = (y as f32 + (j as f32 + 0.5) / 4.0) / s;
                if inside(fx, fy) {
                    n += 1;
                }
            }
        }
        n as f32 / 16.0
    };
    let (m0, m1, r) = if tile {
        (0.1, 0.9, 0.18)
    } else {
        (0.0, 1.0, 0.0)
    };
    let rounded = move |x: f32, y: f32| {
        let cx = x.clamp(m0 + r, m1 - r);
        let cy = y.clamp(m0 + r, m1 - r);
        x >= m0
            && x <= m1
            && y >= m0
            && y <= m1
            && ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() <= r
    };
    let (c, scale) = (0.5, if tile { 0.8 } else { 1.0 });
    let ring = move |x: f32, y: f32| {
        // A tilted orbit: an ellipse band.
        let (dx, dy) = ((x - c) / scale, (y - c) / scale);
        let (a, b) = (0.40, 0.17);
        let (rx, ry) = (dx * 0.866 + dy * 0.5, -dx * 0.5 + dy * 0.866);
        let d = ((rx / a).powi(2) + (ry / b).powi(2)).sqrt();
        (0.86..=1.0).contains(&d)
    };
    let planet = move |x: f32, y: f32| {
        let (dx, dy) = ((x - c) / scale, (y - c) / scale);
        (dx * dx + dy * dy).sqrt() <= 0.19
    };
    let moon = move |x: f32, y: f32| {
        let (dx, dy) = ((x - c) / scale - 0.30, (y - c) / scale + 0.25);
        (dx * dx + dy * dy).sqrt() <= 0.06
    };
    for y in 0..size {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            let mark = cover(x, y, &|fx, fy| {
                ring(fx, fy) || planet(fx, fy) || moon(fx, fy)
            });
            if tile {
                let bg = cover(x, y, &rounded);
                if bg == 0.0 {
                    continue;
                }
                // Teal, lighter at the top.
                let t = y as f32 / s;
                let (r0, g0, b0) = (20.0 + 10.0 * (1.0 - t), 140.0 - 40.0 * t, 128.0 - 30.0 * t);
                let mix = |a: f32, b: f32| (a * (1.0 - mark) + b * mark) as u8;
                px[i] = mix(r0, 250.0);
                px[i + 1] = mix(g0, 250.0);
                px[i + 2] = mix(b0, 245.0);
                px[i + 3] = (bg * 255.0) as u8;
            } else {
                px[i + 3] = (mark * 255.0) as u8;
            }
        }
    }
    px
}

/// Encode RGBA pixels as a PNG (uncompressed deflate: simple, and these are small).
pub fn png(rgba: &[u8], size: u32) -> Vec<u8> {
    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xffff_ffffu32;
        for b in data {
            crc ^= u32::from(*b);
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xedb8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend(data);
        out.extend(&body);
        out.extend(crc32(&body).to_be_bytes());
    }
    let mut raw = Vec::with_capacity(rgba.len() + size as usize);
    for row in rgba.chunks((size * 4) as usize) {
        raw.push(0);
        raw.extend(row);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (n, block) in blocks.iter().enumerate() {
        z.push(u8::from(n + 1 == blocks.len()));
        let len = block.len() as u16;
        z.extend(len.to_le_bytes());
        z.extend((!len).to_le_bytes());
        z.extend(*block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for byte in &raw {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    z.extend(((b << 16) | a).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
    let mut ihdr = Vec::new();
    ihdr.extend(size.to_be_bytes());
    ihdr.extend(size.to_be_bytes());
    ihdr.extend([8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_a_valid_png() {
        let px = draw(64, true);
        assert_eq!(px.len(), 64 * 64 * 4);
        assert!(
            px.chunks(4).any(|p| p[3] == 255),
            "the tile is opaque somewhere"
        );
        let png = png(&px, 64);
        assert_eq!(&png[1..4], b"PNG");
        if std::path::Path::new("/usr/bin/sips").exists() {
            let dir = tempfile::tempdir().unwrap();
            let f = dir.path().join("i.png");
            std::fs::write(&f, png).unwrap();
            let out = std::process::Command::new("/usr/bin/sips")
                .args(["-g", "pixelWidth"])
                .arg(&f)
                .output()
                .unwrap();
            assert!(String::from_utf8_lossy(&out.stdout).contains("64"));
        }
    }
}
