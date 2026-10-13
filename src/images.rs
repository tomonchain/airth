//! Pictures the owner gives their profile and realms, for the app only (the
//! model never sees them). One file per owner or realm, in Airth's home.
//!
//! Only raster formats recognized by their signature are kept: an SVG could
//! carry script, and the file's claimed type is not trusted.

use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

/// The app sends pictures already reduced to 256×256; this leaves room.
pub const MAX_IMAGE: usize = 1 << 20;

const KINDS: [(&str, &str); 4] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("webp", "image/webp"),
    ("gif", "image/gif"),
];

/// The file extension and media type of a supported picture.
fn kind(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    let i = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        0
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        1
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        2
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        3
    } else {
        return None;
    };
    Some(KINDS[i])
}

/// The picture named `stem` in `dir`, if there is one.
fn find(dir: &Path, stem: &str) -> Option<(PathBuf, &'static str)> {
    KINDS.iter().find_map(|(ext, mime)| {
        let path = dir.join(format!("{stem}.{ext}"));
        path.is_file().then_some((path, *mime))
    })
}

/// Replace the picture `stem` in `dir`.
pub fn save(dir: &Path, stem: &str, bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_IMAGE {
        bail!("the picture is too large (at most {} KB)", MAX_IMAGE / 1024);
    }
    let Some((ext, _)) = kind(bytes) else {
        bail!("the picture must be a PNG, JPEG, WebP or GIF image");
    };
    remove(dir, stem)?;
    let path = dir.join(format!("{stem}.{ext}"));
    let tmp = dir.join(format!(".{stem}.{ext}.tmp"));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// The picture's bytes and media type.
pub fn load(dir: &Path, stem: &str) -> Option<(Vec<u8>, &'static str)> {
    let (path, mime) = find(dir, stem)?;
    Some((std::fs::read(path).ok()?, mime))
}

/// Remove the picture; false if there was none.
pub fn remove(dir: &Path, stem: &str) -> Result<bool> {
    let mut removed = false;
    while let Some((path, _)) = find(dir, stem) {
        std::fs::remove_file(path)?;
        removed = true;
    }
    Ok(removed)
}

/// Changes when the picture does (for the app's cache), None without one.
pub fn version(dir: &Path, stem: &str) -> Option<u64> {
    let (path, _) = find(dir, stem)?;
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let ms = modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis();
    Some(ms as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_one_raster_picture() {
        let dir = tempfile::tempdir().unwrap();
        let png = b"\x89PNG\r\n\x1a\nrest".to_vec();
        let jpg = vec![0xFF, 0xD8, 0xFF, 0xE0, 1, 2];
        assert!(version(dir.path(), "avatar").is_none());
        save(dir.path(), "avatar", &png).unwrap();
        assert_eq!(load(dir.path(), "avatar").unwrap(), (png, "image/png"));
        assert!(version(dir.path(), "avatar").is_some());
        // Another format replaces it.
        save(dir.path(), "avatar", &jpg).unwrap();
        assert_eq!(load(dir.path(), "avatar").unwrap().1, "image/jpeg");
        assert!(!dir.path().join("avatar.png").exists());

        let svg = b"<svg onload=\"alert(1)\"/>";
        assert!(save(dir.path(), "avatar", svg).is_err());
        assert!(save(dir.path(), "avatar", &vec![0xFF; MAX_IMAGE + 1]).is_err());
        assert_eq!(load(dir.path(), "avatar").unwrap().1, "image/jpeg");

        assert!(remove(dir.path(), "avatar").unwrap());
        assert!(!remove(dir.path(), "avatar").unwrap());
        assert!(load(dir.path(), "avatar").is_none());
    }
}
