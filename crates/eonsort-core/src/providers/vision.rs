use super::{Detection, Provider};
use crate::ai::{Client, Reading};
use crate::error::Result;
use std::path::Path;

const READABLE: [&str; 9] = [
    "jpg", "jpeg", "jpe", "png", "webp", "tif", "tiff", "bmp", "gif",
];

pub fn readable(path: &Path) -> bool {
    super::extension_lowercase(path)
        .map(|ext| READABLE.contains(&ext.as_str()))
        .unwrap_or(false)
}

pub fn detect(path: &Path, ai: Option<&Client>) -> Option<Detection> {
    let reading = read(path, ai).ok()?;
    if !reading.date_confident {
        return None;
    }
    into_detection(&reading)
}

pub fn read(path: &Path, ai: Option<&Client>) -> Result<Reading> {
    let client = ai.ok_or(crate::error::Error::AiDisabled)?;
    if !readable(path) {
        return Err(crate::error::Error::Ai(format!(
            "{} is not an image the model can look at",
            path.display()
        )));
    }
    client.read_image(path)
}

pub fn into_detection(reading: &Reading) -> Option<Detection> {
    Some(Detection {
        provider: Provider::Vision,
        info: reading
            .date_source
            .clone()
            .or_else(|| Some("read from the picture".to_string())),
        taken: reading.taken?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn reading(taken: Option<&str>, confident: bool) -> Reading {
        Reading {
            taken: taken.map(|t| {
                NaiveDate::parse_from_str(t, "%Y-%m-%d")
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
            }),
            date_confident: confident,
            date_source: Some("receipt header".into()),
            subject: Some("receipt".into()),
            tags: vec!["paper".into()],
            caption: None,
        }
    }

    #[test]
    fn only_looks_at_formats_the_model_can_decode() {
        assert!(readable(Path::new("/a/scan.jpg")));
        assert!(readable(Path::new("/a/scan.TIFF")));
        assert!(!readable(Path::new("/a/clip.mp4")));
        assert!(!readable(Path::new("/a/notes.txt")));
        assert!(!readable(Path::new("/a/noextension")));
    }

    #[test]
    fn builds_a_detection_from_a_reading() {
        let found = into_detection(&reading(Some("2019-07-04"), true)).unwrap();
        assert_eq!(found.provider, Provider::Vision);
        assert_eq!(found.info.as_deref(), Some("receipt header"));
    }

    #[test]
    fn a_reading_with_no_date_yields_no_detection() {
        assert!(into_detection(&reading(None, true)).is_none());
    }

    #[test]
    fn without_a_client_there_is_no_detection_and_no_panic() {
        assert!(detect(Path::new("/a/scan.jpg"), None).is_none());
        assert!(matches!(
            read(Path::new("/a/scan.jpg"), None),
            Err(crate::error::Error::AiDisabled)
        ));
    }
}
