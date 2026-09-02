use super::{extension_lowercase, Detection, Provider};
use ::exif::{In, Tag, Value};
use chrono::{Duration, NaiveDate};
use std::path::Path;

const EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jpe", "tif", "tiff", "png", "webp", "heic", "heif", "hif", "avif", "dng",
    "cr2", "cr3", "nef", "nrw", "arw", "sr2", "srf", "orf", "rw2", "raf", "pef", "3fr",
];

pub fn detect(path: &Path) -> Option<Detection> {
    let ext = extension_lowercase(path)?;
    if !EXTENSIONS.contains(&ext.as_str()) {
        return None;
    }

    let exif = crate::exifread::from_path(path)?;
    read(&exif)
}

fn read(exif: &::exif::Exif) -> Option<Detection> {
    let day = stamped_day(exif)?;
    let (hour, minute, second) = stamped_time(exif)?;
    let utc = day.and_hms_opt(hour, minute, second)?;

    match zone_minutes(exif) {
        Some((minutes, label)) => Some(Detection {
            provider: Provider::Gps,
            info: Some(format!("GPSDateStamp {label}")),
            taken: utc + Duration::minutes(minutes),
        }),
        None => Some(Detection {
            provider: Provider::Gps,
            info: Some("GPSDateStamp UTC".to_string()),
            taken: utc,
        }),
    }
}

fn stamped_day(exif: &::exif::Exif) -> Option<NaiveDate> {
    let field = exif.get_field(Tag::GPSDateStamp, In::PRIMARY)?;
    let raw = match field.value {
        Value::Ascii(ref lines) => String::from_utf8_lossy(lines.first()?).into_owned(),
        _ => field.display_value().to_string(),
    };
    let text = raw.trim().trim_matches('"');
    let parts: Vec<&str> = text.split([':', '-', '/']).collect();
    if parts.len() != 3 {
        return None;
    }
    NaiveDate::from_ymd_opt(
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    )
}

fn stamped_time(exif: &::exif::Exif) -> Option<(u32, u32, u32)> {
    let field = exif.get_field(Tag::GPSTimeStamp, In::PRIMARY)?;
    let Value::Rational(ref parts) = field.value else {
        return None;
    };
    if parts.len() < 3 {
        return None;
    }
    let whole = |index: usize| -> Option<u32> {
        let part = parts.get(index)?;
        (part.denom != 0).then(|| part.num / part.denom)
    };
    let (hour, minute, second) = (whole(0)?, whole(1)?, whole(2)?);
    (hour < 24 && minute < 60 && second < 60).then_some((hour, minute, second))
}

fn zone_minutes(exif: &::exif::Exif) -> Option<(i64, String)> {
    for tag in [Tag::OffsetTimeOriginal, Tag::OffsetTime] {
        let Some(field) = exif.get_field(tag, In::PRIMARY) else {
            continue;
        };
        let raw = field.display_value().to_string();
        let text = raw.trim().trim_matches('"');
        if let Some(minutes) = parse_offset(text) {
            return Some((minutes, text.to_string()));
        }
    }
    None
}

fn parse_offset(text: &str) -> Option<i64> {
    let sign = match text.chars().next()? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    let body = &text[1..];
    let (hours, minutes) = body.split_once(':')?;
    let hours: i64 = hours.parse().ok()?;
    let minutes: i64 = minutes.parse().ok()?;
    (hours <= 14 && minutes < 60).then_some(sign * (hours * 60 + minutes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gps_tiff(
        date: Option<&str>,
        time: Option<[(u32, u32); 3]>,
        offset: Option<&str>,
    ) -> Vec<u8> {
        let mut entries: Vec<(u16, u16, Vec<u8>)> = Vec::new();
        if let Some(date) = date {
            let mut text = date.as_bytes().to_vec();
            text.push(0);
            entries.push((0x001d, 2, text));
        }
        if let Some(time) = time {
            let mut body = Vec::new();
            for (num, denom) in time {
                body.extend_from_slice(&num.to_le_bytes());
                body.extend_from_slice(&denom.to_le_bytes());
            }
            entries.push((0x0007, 5, body));
        }

        let mut exif_entries: Vec<(u16, u16, Vec<u8>)> = Vec::new();
        if let Some(offset) = offset {
            let mut text = offset.as_bytes().to_vec();
            text.push(0);
            exif_entries.push((0x9011, 2, text));
        }

        build(&entries, &exif_entries)
    }

    fn build(gps: &[(u16, u16, Vec<u8>)], exif: &[(u16, u16, Vec<u8>)]) -> Vec<u8> {
        let mut head = Vec::new();
        head.extend_from_slice(b"II\x2a\x00");
        head.extend_from_slice(&8u32.to_le_bytes());

        let ifd0_count: u16 = 2;
        let ifd0_len = 2 + ifd0_count as usize * 12 + 4;
        let gps_start = 8 + ifd0_len;
        let gps_len = 2 + gps.len() * 12 + 4;
        let exif_start = gps_start + gps_len;
        let exif_len = 2 + exif.len() * 12 + 4;
        let mut heap_at = exif_start + exif_len;

        let mut heap = Vec::new();
        let mut gps_body = Vec::new();
        gps_body.extend_from_slice(&(gps.len() as u16).to_le_bytes());
        for (tag, kind, body) in gps {
            gps_body.extend_from_slice(&tag.to_le_bytes());
            gps_body.extend_from_slice(&kind.to_le_bytes());
            let count = if *kind == 5 {
                body.len() / 8
            } else {
                body.len()
            };
            gps_body.extend_from_slice(&(count as u32).to_le_bytes());
            gps_body.extend_from_slice(&(heap_at as u32).to_le_bytes());
            heap.extend_from_slice(body);
            heap_at += body.len();
        }
        gps_body.extend_from_slice(&0u32.to_le_bytes());

        let mut exif_body = Vec::new();
        exif_body.extend_from_slice(&(exif.len() as u16).to_le_bytes());
        for (tag, kind, body) in exif {
            exif_body.extend_from_slice(&tag.to_le_bytes());
            exif_body.extend_from_slice(&kind.to_le_bytes());
            exif_body.extend_from_slice(&(body.len() as u32).to_le_bytes());
            exif_body.extend_from_slice(&(heap_at as u32).to_le_bytes());
            heap.extend_from_slice(body);
            heap_at += body.len();
        }
        exif_body.extend_from_slice(&0u32.to_le_bytes());

        let mut ifd0 = Vec::new();
        ifd0.extend_from_slice(&ifd0_count.to_le_bytes());
        ifd0.extend_from_slice(&0x8825u16.to_le_bytes());
        ifd0.extend_from_slice(&4u16.to_le_bytes());
        ifd0.extend_from_slice(&1u32.to_le_bytes());
        ifd0.extend_from_slice(&(gps_start as u32).to_le_bytes());
        ifd0.extend_from_slice(&0x8769u16.to_le_bytes());
        ifd0.extend_from_slice(&4u16.to_le_bytes());
        ifd0.extend_from_slice(&1u32.to_le_bytes());
        ifd0.extend_from_slice(&(exif_start as u32).to_le_bytes());
        ifd0.extend_from_slice(&0u32.to_le_bytes());

        let mut out = head;
        out.extend_from_slice(&ifd0);
        out.extend_from_slice(&gps_body);
        out.extend_from_slice(&exif_body);
        out.extend_from_slice(&heap);
        out
    }

    fn parsed(bytes: &[u8]) -> Option<Detection> {
        let exif = ::exif::Reader::new()
            .read_raw(bytes.to_vec())
            .expect("the test tiff should parse");
        read(&exif)
    }

    #[test]
    fn reads_the_satellite_date_and_time_as_utc() {
        let bytes = gps_tiff(Some("2019:07:04"), Some([(10, 1), (11, 1), (12, 1)]), None);
        let found = parsed(&bytes).expect("a gps date should be found");

        assert_eq!(found.provider, Provider::Gps);
        assert_eq!(found.info.as_deref(), Some("GPSDateStamp UTC"));
        assert_eq!(
            found.taken,
            NaiveDate::from_ymd_opt(2019, 7, 4)
                .unwrap()
                .and_hms_opt(10, 11, 12)
                .unwrap()
        );
    }

    #[test]
    fn moves_the_satellite_time_into_the_zone_the_camera_recorded() {
        let bytes = gps_tiff(
            Some("2019:07:04"),
            Some([(10, 1), (11, 1), (12, 1)]),
            Some("+02:00"),
        );
        let found = parsed(&bytes).expect("a gps date should be found");

        assert_eq!(found.info.as_deref(), Some("GPSDateStamp +02:00"));
        assert_eq!(
            found.taken,
            NaiveDate::from_ymd_opt(2019, 7, 4)
                .unwrap()
                .and_hms_opt(12, 11, 12)
                .unwrap()
        );
    }

    #[test]
    fn a_zone_behind_utc_moves_the_day_back() {
        let bytes = gps_tiff(
            Some("2019:07:04"),
            Some([(2, 1), (0, 1), (0, 1)]),
            Some("-05:00"),
        );
        let found = parsed(&bytes).expect("a gps date should be found");

        assert_eq!(
            found.taken,
            NaiveDate::from_ymd_opt(2019, 7, 3)
                .unwrap()
                .and_hms_opt(21, 0, 0)
                .unwrap()
        );
    }

    #[test]
    fn a_file_carrying_only_half_the_stamp_reports_nothing() {
        assert!(parsed(&gps_tiff(Some("2019:07:04"), None, None)).is_none());
        assert!(parsed(&gps_tiff(None, Some([(10, 1), (11, 1), (12, 1)]), None)).is_none());
        assert!(parsed(&gps_tiff(None, None, None)).is_none());
    }

    #[test]
    fn a_time_outside_the_clock_is_refused() {
        assert!(parsed(&gps_tiff(
            Some("2019:07:04"),
            Some([(25, 1), (0, 1), (0, 1)]),
            None
        ))
        .is_none());
        assert!(parsed(&gps_tiff(
            Some("2019:07:04"),
            Some([(10, 0), (0, 1), (0, 1)]),
            None
        ))
        .is_none());
    }

    #[test]
    fn a_date_that_is_not_a_day_is_refused() {
        assert!(parsed(&gps_tiff(
            Some("2019:13:40"),
            Some([(10, 1), (11, 1), (12, 1)]),
            None
        ))
        .is_none());
    }

    #[test]
    fn reads_the_offsets_a_camera_writes() {
        assert_eq!(parse_offset("+02:00"), Some(120));
        assert_eq!(parse_offset("-05:30"), Some(-330));
        assert_eq!(parse_offset("+00:00"), Some(0));
        assert_eq!(parse_offset("02:00"), None);
        assert_eq!(parse_offset("+99:00"), None);
        assert_eq!(parse_offset(""), None);
    }
}
