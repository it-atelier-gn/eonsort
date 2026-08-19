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
    image::open(path).ok()
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
    fn refuses_heif_that_does_not_parse() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.heic");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(b"not a container").unwrap();

        assert!(open(&path).is_none());
    }
}
