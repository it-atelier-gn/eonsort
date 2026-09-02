use image::DynamicImage;
use std::io::Read;
use std::path::Path;

pub const RAW_EXTENSIONS: [&str; 15] = [
    "dng", "cr2", "cr3", "nef", "nrw", "arw", "sr2", "srf", "orf", "rw2", "raf", "pef", "3fr",
    "iiq", "erf",
];

const SCAN_LIMIT: usize = 64 * 1024 * 1024;
const MIN_PREVIEW_BYTES: usize = 1024;
const MAX_CANDIDATES: usize = 8;

pub fn is_raw_extension(extension: &str) -> bool {
    RAW_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
}

pub fn is_raw(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(is_raw_extension)
        .unwrap_or(false)
}

pub fn preview(path: &Path) -> Option<DynamicImage> {
    let body = read_capped(path)?;
    for candidate in candidates(&body) {
        if let Ok(decoded) =
            image::load_from_memory_with_format(candidate, image::ImageFormat::Jpeg)
        {
            return Some(decoded);
        }
    }
    None
}

pub fn preview_bytes(path: &Path) -> Option<Vec<u8>> {
    let body = read_capped(path)?;
    candidates(&body).first().map(|slice| slice.to_vec())
}

fn read_capped(path: &Path) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    let mut body = Vec::new();
    file.take(SCAN_LIMIT as u64).read_to_end(&mut body).ok()?;
    Some(body)
}

fn candidates(body: &[u8]) -> Vec<&[u8]> {
    let mut found: Vec<&[u8]> = Vec::new();
    let mut at = 0;

    while at + 3 < body.len() {
        if body[at] == 0xFF && body[at + 1] == 0xD8 && body[at + 2] == 0xFF {
            if let Some(end) = end_of_image(body, at + 2) {
                if end - at >= MIN_PREVIEW_BYTES {
                    found.push(&body[at..end]);
                }
                at = end;
                continue;
            }
        }
        at += 1;
    }

    found.sort_by_key(|slice| std::cmp::Reverse(slice.len()));
    found.truncate(MAX_CANDIDATES);
    found
}

fn end_of_image(body: &[u8], from: usize) -> Option<usize> {
    let mut at = from;
    while at + 1 < body.len() {
        if body[at] == 0xFF && body[at + 1] == 0xD9 {
            return Some(at + 2);
        }
        at += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn jpeg(width: u32, height: u32) -> Vec<u8> {
        let mut pixels = image::RgbImage::new(width, height);
        let mut noise: u32 = 0x1234_5678;
        for pixel in pixels.pixels_mut() {
            noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *pixel = image::Rgb([(noise >> 16) as u8, (noise >> 8) as u8, noise as u8]);
        }
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(pixels)
            .write_to(
                &mut std::io::Cursor::new(&mut out),
                image::ImageFormat::Jpeg,
            )
            .unwrap();
        out
    }

    fn pretend_raw(previews: &[Vec<u8>]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(b"II\x2a\x00");
        body.extend_from_slice(&[0u8; 64]);
        for preview in previews {
            body.extend_from_slice(preview);
            body.extend_from_slice(&[0u8; 32]);
        }
        body
    }

    #[test]
    fn knows_the_raw_formats_by_their_extension() {
        assert!(is_raw(Path::new("/photos/IMG_1.CR2")));
        assert!(is_raw(Path::new("/photos/IMG_1.nef")));
        assert!(is_raw(Path::new("/photos/IMG_1.dng")));
        assert!(!is_raw(Path::new("/photos/IMG_1.jpg")));
        assert!(!is_raw(Path::new("/photos/IMG_1")));
    }

    #[test]
    fn pulls_the_preview_out_of_a_raw_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("shot.cr2");
        std::fs::write(&path, pretend_raw(&[jpeg(160, 120)])).unwrap();

        let found = preview(&path).unwrap();
        assert_eq!((found.width(), found.height()), (160, 120));
    }

    #[test]
    fn prefers_the_biggest_preview_over_the_thumbnail() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("shot.nef");
        std::fs::write(&path, pretend_raw(&[jpeg(64, 48), jpeg(320, 240)])).unwrap();

        let found = preview(&path).unwrap();
        assert_eq!((found.width(), found.height()), (320, 240));
    }

    #[test]
    fn the_order_the_previews_sit_in_does_not_matter() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("shot.arw");
        std::fs::write(&path, pretend_raw(&[jpeg(320, 240), jpeg(64, 48)])).unwrap();

        let found = preview(&path).unwrap();
        assert_eq!((found.width(), found.height()), (320, 240));
    }

    #[test]
    fn a_raw_with_nothing_embedded_yields_nothing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("empty.cr2");
        std::fs::write(&path, pretend_raw(&[])).unwrap();

        assert!(preview(&path).is_none());
    }

    #[test]
    fn a_scrap_too_small_to_be_a_preview_is_ignored() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("tiny.cr2");
        let mut body = pretend_raw(&[]);
        body.extend_from_slice(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0xFF, 0xD9]);
        std::fs::write(&path, body).unwrap();

        assert!(preview(&path).is_none());
    }

    #[test]
    fn a_preview_that_never_ends_is_not_mistaken_for_one() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("truncated.cr2");
        let mut body = pretend_raw(&[]);
        body.extend_from_slice(&[0xFF, 0xD8, 0xFF]);
        body.extend_from_slice(&vec![0x11; MIN_PREVIEW_BYTES * 2]);
        std::fs::write(&path, body).unwrap();

        assert!(preview(&path).is_none());
    }

    #[test]
    fn rubbish_that_only_looks_like_a_preview_is_stepped_over() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("liar.cr2");
        let mut body = Vec::new();
        body.extend_from_slice(&[0xFF, 0xD8, 0xFF]);
        body.extend_from_slice(&vec![0x42; MIN_PREVIEW_BYTES]);
        body.extend_from_slice(&[0xFF, 0xD9]);
        body.extend_from_slice(&jpeg(200, 150));
        std::fs::write(&path, body).unwrap();

        let found = preview(&path).unwrap();
        assert_eq!((found.width(), found.height()), (200, 150));
    }

    #[test]
    fn the_raw_bytes_of_the_preview_come_back_as_a_jpeg() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("shot.cr2");
        std::fs::write(&path, pretend_raw(&[jpeg(160, 120)])).unwrap();

        let bytes = preview_bytes(&path).unwrap();
        assert_eq!(&bytes[0..3], &[0xFF, 0xD8, 0xFF]);
        assert_eq!(&bytes[bytes.len() - 2..], &[0xFF, 0xD9]);
    }

    #[test]
    fn a_file_that_is_not_there_is_not_a_crash() {
        assert!(preview(Path::new("/nowhere/at/all.cr2")).is_none());
        assert!(preview_bytes(Path::new("/nowhere/at/all.cr2")).is_none());
    }
}
