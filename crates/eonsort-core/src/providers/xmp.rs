use super::{Detection, Provider};
use chrono::{DateTime, NaiveDate, NaiveDateTime};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

const FIELDS: [&str; 4] = [
    "exif:DateTimeOriginal",
    "photoshop:DateCreated",
    "xmp:CreateDate",
    "xmp:ModifyDate",
];

static PATTERNS: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    FIELDS
        .iter()
        .map(|field| {
            let escaped = regex::escape(field);
            let pattern =
                format!(r#"(?is){escaped}\s*=\s*"([^"]+)"|<{escaped}>([^<]+)</{escaped}>"#);
            (
                *field,
                Regex::new(&pattern).expect("xmp pattern must compile"),
            )
        })
        .collect()
});

pub fn detect(path: &Path) -> Option<Detection> {
    let sidecar = sidecar(path)?;
    let raw = std::fs::read_to_string(&sidecar).ok()?;
    let (field, taken) = taken_from(&raw)?;

    Some(Detection {
        provider: Provider::Xmp,
        info: Some(format!(
            "{field} in {}",
            sidecar.file_name()?.to_string_lossy()
        )),
        taken,
    })
}

pub fn sidecar(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    let name = path.file_name()?.to_str()?;
    let stem = path.file_stem()?.to_str()?;

    [
        format!("{name}.xmp"),
        format!("{name}.XMP"),
        format!("{stem}.xmp"),
        format!("{stem}.XMP"),
    ]
    .into_iter()
    .map(|candidate| parent.join(candidate))
    .find(|candidate| candidate.is_file())
}

fn taken_from(raw: &str) -> Option<(&'static str, NaiveDateTime)> {
    PATTERNS.iter().find_map(|(field, pattern)| {
        let caught = pattern.captures(raw)?;
        let text = caught
            .get(1)
            .or_else(|| caught.get(2))?
            .as_str()
            .trim()
            .to_string();
        stamped(&text).map(|taken| (*field, taken))
    })
}

fn stamped(text: &str) -> Option<NaiveDateTime> {
    if let Ok(zoned) = DateTime::parse_from_rfc3339(text) {
        return Some(zoned.naive_local());
    }
    for shape in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M:%S",
    ] {
        if let Ok(plain) = NaiveDateTime::parse_from_str(text, shape) {
            return Some(plain);
        }
    }
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .ok()
        .and_then(|day| day.and_hms_opt(0, 0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn at(y: i32, m: u32, d: u32, hh: u32, mm: u32, ss: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(hh, mm, ss)
            .unwrap()
    }

    fn attribute_form(field: &str, value: &str) -> String {
        format!(
            r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:exif="http://ns.adobe.com/exif/1.0/"
    {field}="{value}"/>
 </rdf:RDF>
</x:xmpmeta>"#
        )
    }

    #[test]
    fn reads_the_sidecar_lightroom_writes_next_to_a_raw() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("IMG_1234.CR2"), b"raw").unwrap();
        fs::write(
            dir.path().join("IMG_1234.xmp"),
            attribute_form("exif:DateTimeOriginal", "2019-05-14T09:22:03+02:00"),
        )
        .unwrap();

        let found = detect(&dir.path().join("IMG_1234.CR2")).unwrap();
        assert_eq!(found.provider, Provider::Xmp);
        assert_eq!(found.taken, at(2019, 5, 14, 9, 22, 3));
        assert_eq!(
            found.info.as_deref(),
            Some("exif:DateTimeOriginal in IMG_1234.xmp")
        );
    }

    #[test]
    fn reads_the_sidecar_darktable_writes_next_to_a_raw() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("IMG_1234.CR2"), b"raw").unwrap();
        fs::write(
            dir.path().join("IMG_1234.CR2.xmp"),
            attribute_form("xmp:CreateDate", "2019-05-14T09:22:03"),
        )
        .unwrap();

        assert_eq!(
            detect(&dir.path().join("IMG_1234.CR2")).unwrap().taken,
            at(2019, 5, 14, 9, 22, 3)
        );
    }

    #[test]
    fn reads_a_date_written_as_its_own_element() {
        let raw = r#"<rdf:Description>
            <xmp:CreateDate>2021-07-04T11:30:00Z</xmp:CreateDate>
        </rdf:Description>"#;

        assert_eq!(taken_from(raw).unwrap().1, at(2021, 7, 4, 11, 30, 0));
    }

    #[test]
    fn the_capture_date_wins_over_the_date_the_file_was_last_edited() {
        let raw = r#"<rdf:Description
            xmp:ModifyDate="2024-02-02T08:00:00"
            exif:DateTimeOriginal="2019-05-14T09:22:03"/>"#;

        let (field, taken) = taken_from(raw).unwrap();
        assert_eq!(field, "exif:DateTimeOriginal");
        assert_eq!(taken, at(2019, 5, 14, 9, 22, 3));
    }

    #[test]
    fn takes_a_date_with_no_time_at_midnight() {
        let raw = r#"<rdf:Description photoshop:DateCreated="2019-05-14"/>"#;
        assert_eq!(taken_from(raw).unwrap().1, at(2019, 5, 14, 0, 0, 0));
    }

    #[test]
    fn keeps_the_wall_clock_time_an_offset_was_written_against() {
        let raw = r#"<rdf:Description xmp:CreateDate="2019-05-14T09:22:03-07:00"/>"#;
        assert_eq!(taken_from(raw).unwrap().1, at(2019, 5, 14, 9, 22, 3));
    }

    #[test]
    fn says_nothing_without_a_sidecar() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("IMG_1234.CR2"), b"raw").unwrap();

        assert!(detect(&dir.path().join("IMG_1234.CR2")).is_none());
    }

    #[test]
    fn says_nothing_for_a_sidecar_that_carries_no_date() {
        assert!(taken_from(r#"<rdf:Description xmp:Rating="5"/>"#).is_none());
        assert!(taken_from("not xmp at all").is_none());
        assert!(taken_from(r#"<rdf:Description xmp:CreateDate="whenever"/>"#).is_none());
    }
}
