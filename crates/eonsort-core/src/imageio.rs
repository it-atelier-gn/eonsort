use image::DynamicImage;
use std::path::Path;

const HEIF_EXTENSIONS: [&str; 3] = ["heic", "heif", "hif"];

pub fn is_heif_extension(extension: &str) -> bool {
    HEIF_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
}

pub fn is_heif(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(is_heif_extension)
        .unwrap_or(false)
}

pub fn open(path: &Path) -> Option<DynamicImage> {
    if is_heif(path) {
        return heif(path);
    }
    if crate::raw::is_raw(path) {
        return crate::raw::preview(path);
    }
    image::open(path).ok()
}

pub fn open_upright(path: &Path) -> Option<DynamicImage> {
    let opened = open(path)?;
    let turn = crate::rotate::Transform::for_orientation(crate::rotate::read_orientation(path));
    Some(crate::rotate::applied(opened, turn))
}

fn heif(path: &Path) -> Option<DynamicImage> {
    let decoded = heif_oxide::decode_file(path).ok()?;
    let pixels = image::RgbaImage::from_raw(decoded.width, decoded.height, decoded.to_rgba8())?;
    Some(DynamicImage::ImageRgba8(pixels))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn recognises_heif_by_extension() {
        assert!(is_heif(Path::new("/photos/IMG_0001.HEIC")));
        assert!(is_heif(Path::new("/photos/IMG_0001.heif")));
        assert!(is_heif(Path::new("/photos/IMG_0001.hif")));
        assert!(!is_heif(Path::new("/photos/IMG_0001.jpg")));
        assert!(!is_heif(Path::new("/photos/IMG_0001")));
    }

    #[test]
    fn opens_formats_the_image_crate_handles() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("red.png");
        image::RgbImage::from_pixel(4, 3, image::Rgb([255, 0, 0]))
            .save(&path)
            .unwrap();

        let opened = open(&path).unwrap();
        assert_eq!((opened.width(), opened.height()), (4, 3));
    }

    #[test]
    fn a_sideways_photo_opens_the_way_it_is_meant_to_be_seen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sideways.jpg");
        let mut body = Vec::new();
        DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            64,
            32,
            image::Rgb([9, 40, 200]),
        ))
        .write_to(
            &mut std::io::Cursor::new(&mut body),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
        std::fs::write(&path, exif_app1(&body, 6)).unwrap();

        let stored = open(&path).unwrap();
        assert_eq!((stored.width(), stored.height()), (64, 32));

        let seen = open_upright(&path).unwrap();
        assert_eq!((seen.width(), seen.height()), (32, 64));
    }

    #[test]
    fn a_photo_with_nothing_to_turn_opens_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("flat.png");
        image::RgbImage::from_pixel(6, 4, image::Rgb([1, 2, 3]))
            .save(&path)
            .unwrap();

        let seen = open_upright(&path).unwrap();
        assert_eq!((seen.width(), seen.height()), (6, 4));
    }

    fn exif_app1(jpeg: &[u8], orientation: u16) -> Vec<u8> {
        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"MM");
        tiff.extend_from_slice(&42u16.to_be_bytes());
        tiff.extend_from_slice(&8u32.to_be_bytes());
        tiff.extend_from_slice(&1u16.to_be_bytes());
        tiff.extend_from_slice(&0x0112u16.to_be_bytes());
        tiff.extend_from_slice(&3u16.to_be_bytes());
        tiff.extend_from_slice(&1u32.to_be_bytes());
        tiff.extend_from_slice(&orientation.to_be_bytes());
        tiff.extend_from_slice(&[0, 0]);
        tiff.extend_from_slice(&0u32.to_be_bytes());

        let mut app1 = Vec::new();
        app1.extend_from_slice(b"Exif\0\0");
        app1.extend_from_slice(&tiff);

        let mut out = Vec::new();
        out.extend_from_slice(&jpeg[0..2]);
        out.extend_from_slice(&[0xFF, 0xE1]);
        out.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
        out.extend_from_slice(&app1);
        out.extend_from_slice(&jpeg[2..]);
        out
    }

    #[test]
    fn refuses_heif_that_does_not_parse() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.heic");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(b"not a container").unwrap();

        assert!(open(&path).is_none());
    }
}
