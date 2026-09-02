use super::{extension_lowercase, Detection, Provider};
use crate::dateparse::parse_date;
use ::exif::{In, Tag};
use std::path::Path;

const EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jpe", "tif", "tiff", "png", "webp", "heic", "heif", "hif", "avif", "dng",
    "cr2", "cr3", "nef", "nrw", "arw", "sr2", "srf", "orf", "rw2", "raf", "pef", "3fr",
];

const TAGS: &[(Tag, &str, Tag)] = &[
    (
        Tag::DateTimeOriginal,
        "DateTimeOriginal",
        Tag::OffsetTimeOriginal,
    ),
    (
        Tag::DateTimeDigitized,
        "DateTimeDigitized",
        Tag::OffsetTimeDigitized,
    ),
    (Tag::DateTime, "DateTime", Tag::OffsetTime),
];

pub fn detect(path: &Path) -> Option<Detection> {
    let ext = extension_lowercase(path)?;
    if !EXTENSIONS.contains(&ext.as_str()) {
        return None;
    }

    let exif = crate::exifread::from_path(path)?;

    TAGS.iter().find_map(|(tag, name, offset_tag)| {
        let field = exif.get_field(*tag, In::PRIMARY)?;
        let taken = parse_date(&field.display_value().to_string())?;
        let zone = offset(&exif, *offset_tag);
        Some(Detection {
            provider: Provider::Exif,
            info: Some(match zone {
                Some(zone) => format!("{name} {zone}"),
                None => (*name).to_string(),
            }),
            taken,
        })
    })
}

fn offset(exif: &::exif::Exif, tag: Tag) -> Option<String> {
    let raw = exif
        .get_field(tag, In::PRIMARY)?
        .display_value()
        .to_string();
    let zone = raw.trim().trim_matches('"').to_string();
    zone.starts_with(['+', '-']).then_some(zone)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::fs;
    use tempfile::tempdir;

    const IFD0_OFFSET: u32 = 8;
    const EXIF_IFD_OFFSET: u32 = 26;
    const TEXT_OFFSET: u32 = 44;

    fn tiff_with_date_time_original(value: &str) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"II\x2a\x00");
        out.extend_from_slice(&IFD0_OFFSET.to_le_bytes());

        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&0x8769u16.to_le_bytes());
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&EXIF_IFD_OFFSET.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());

        let text = format!("{value}\0");
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&0x9003u16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&(text.len() as u32).to_le_bytes());
        out.extend_from_slice(&TEXT_OFFSET.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());

        assert_eq!(out.len() as u32, TEXT_OFFSET);
        out.extend_from_slice(text.as_bytes());
        out
    }

    #[test]
    fn reads_date_time_original() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("shot.tif");
        fs::write(&path, tiff_with_date_time_original("2011:05:06 07:08:09")).unwrap();

        let found = detect(&path).unwrap();
        assert_eq!(found.provider, Provider::Exif);
        assert_eq!(found.info.as_deref(), Some("DateTimeOriginal"));
        assert_eq!(
            found.taken,
            NaiveDate::from_ymd_opt(2011, 5, 6)
                .unwrap()
                .and_hms_opt(7, 8, 9)
                .unwrap()
        );
    }

    #[test]
    fn skips_files_without_an_image_extension() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("shot.txt");
        fs::write(&path, tiff_with_date_time_original("2011:05:06 07:08:09")).unwrap();

        assert!(detect(&path).is_none());
    }

    #[test]
    fn returns_none_for_an_image_without_exif() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("empty.jpg");
        fs::write(&path, b"not really a jpeg").unwrap();

        assert!(detect(&path).is_none());
    }
}
