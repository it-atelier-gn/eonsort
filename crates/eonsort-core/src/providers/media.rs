use super::{extension_lowercase, Detection, Provider};
use chrono::{DateTime, FixedOffset, Local, NaiveDateTime};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "m4a", "m4b", "m4p", "mov", "qt", "3gp", "3g2", "mj2", "f4v",
];

/// ISO base media files count seconds from 1904-01-01, not the Unix epoch.
const EPOCH_OFFSET_SECONDS: i64 = 2_082_844_800;
const MIN_UNIX: i64 = 0;
const MAX_UNIX: i64 = 4_102_444_800;
const MAX_SIBLINGS: usize = 512;
const CREATION_KEY: &str = "com.apple.quicktime.creationdate";

pub fn detect(path: &Path) -> Option<Detection> {
    let ext = extension_lowercase(path)?;
    if !EXTENSIONS.contains(&ext.as_str()) {
        return None;
    }

    let mut file = File::open(path).ok()?;
    let end = file.metadata().ok()?.len();
    let (moov_start, moov_end) = find_box(&mut file, 0, end, b"moov")?;

    if let Some((taken, info)) = capture_local(&mut file, moov_start, moov_end) {
        return Some(Detection {
            provider: Provider::Media,
            info: Some(info),
            taken,
        });
    }

    let (mvhd_start, _) = find_box(&mut file, moov_start, moov_end, b"mvhd")?;
    let (created, modified) = read_mvhd_times(&mut file, mvhd_start)?;
    let (taken, info) = to_local(created)
        .map(|t| (t, "mvhd.creation_time"))
        .or_else(|| to_local(modified).map(|t| (t, "mvhd.modification_time")))?;

    Some(Detection {
        provider: Provider::Media,
        info: Some(info.to_string()),
        taken,
    })
}

fn capture_local(
    file: &mut File,
    moov_start: u64,
    moov_end: u64,
) -> Option<(NaiveDateTime, String)> {
    let (meta_start, meta_end) = find_box(file, moov_start, moov_end, b"meta")?;
    let start = meta_content(file, meta_start)?;

    let (keys_start, keys_end) = find_box(file, start, meta_end, b"keys")?;
    let index = key_index(file, keys_start, keys_end, CREATION_KEY)?;

    let (ilst_start, ilst_end) = find_box(file, start, meta_end, b"ilst")?;
    let text = ilst_text(file, ilst_start, ilst_end, index)?;

    let stamped = with_offset(text.trim())?;
    Some((
        stamped.naive_local(),
        format!("{CREATION_KEY} {}", stamped.offset()),
    ))
}

fn meta_content(file: &mut File, meta_start: u64) -> Option<u64> {
    file.seek(SeekFrom::Start(meta_start)).ok()?;
    let mut header = [0u8; 8];
    file.read_exact(&mut header).ok()?;

    let named = matches!(&header[4..8], b"hdlr" | b"keys" | b"ilst" | b"free");
    Some(if named { meta_start } else { meta_start + 4 })
}

fn key_index(file: &mut File, start: u64, end: u64, want: &str) -> Option<u32> {
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut head = [0u8; 8];
    file.read_exact(&mut head).ok()?;
    let count = u32::from_be_bytes(head[4..8].try_into().ok()?);

    let mut pos = start + 8;
    for index in 1..=count.min(MAX_SIBLINGS as u32) {
        if pos + 8 > end {
            return None;
        }
        file.seek(SeekFrom::Start(pos)).ok()?;
        let mut entry = [0u8; 8];
        file.read_exact(&mut entry).ok()?;
        let size = u32::from_be_bytes(entry[0..4].try_into().ok()?) as u64;
        if size < 8 || pos + size > end {
            return None;
        }

        let mut name = vec![0u8; (size - 8) as usize];
        file.read_exact(&mut name).ok()?;
        if name == want.as_bytes() {
            return Some(index);
        }
        pos += size;
    }
    None
}

fn ilst_text(file: &mut File, start: u64, end: u64, index: u32) -> Option<String> {
    let mut pos = start;
    for _ in 0..MAX_SIBLINGS {
        if pos + 8 > end {
            return None;
        }
        file.seek(SeekFrom::Start(pos)).ok()?;
        let mut header = [0u8; 8];
        file.read_exact(&mut header).ok()?;
        let size = u32::from_be_bytes(header[0..4].try_into().ok()?) as u64;
        if size < 8 || pos + size > end {
            return None;
        }

        if u32::from_be_bytes(header[4..8].try_into().ok()?) == index {
            let mut data = [0u8; 16];
            file.read_exact(&mut data).ok()?;
            if &data[4..8] != b"data" {
                return None;
            }
            let payload = u32::from_be_bytes(data[0..4].try_into().ok()?) as u64;
            if payload < 16 || payload > size - 8 {
                return None;
            }
            let mut text = vec![0u8; (payload - 16) as usize];
            file.read_exact(&mut text).ok()?;
            return String::from_utf8(text).ok();
        }
        pos += size;
    }
    None
}

fn with_offset(text: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(text)
        .or_else(|_| DateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%z"))
        .or_else(|_| DateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f%z"))
        .ok()
}

fn find_box(file: &mut File, start: u64, end: u64, want: &[u8; 4]) -> Option<(u64, u64)> {
    let mut pos = start;
    for _ in 0..MAX_SIBLINGS {
        if pos.checked_add(8)? > end {
            return None;
        }
        file.seek(SeekFrom::Start(pos)).ok()?;
        let mut header = [0u8; 8];
        file.read_exact(&mut header).ok()?;

        let declared = u32::from_be_bytes(header[0..4].try_into().ok()?) as u64;
        let kind: [u8; 4] = header[4..8].try_into().ok()?;

        let (total, header_len) = match declared {
            0 => (end - pos, 8),
            1 => {
                let mut ext = [0u8; 8];
                file.read_exact(&mut ext).ok()?;
                (u64::from_be_bytes(ext), 16)
            }
            n => (n, 8),
        };

        if total < header_len || pos.checked_add(total)? > end {
            return None;
        }
        if kind == *want {
            return Some((pos + header_len, pos + total));
        }
        pos += total;
    }
    None
}

fn read_mvhd_times(file: &mut File, content_start: u64) -> Option<(i64, i64)> {
    file.seek(SeekFrom::Start(content_start)).ok()?;
    let mut version = [0u8; 4];
    file.read_exact(&mut version).ok()?;

    if version[0] == 1 {
        let mut buf = [0u8; 16];
        file.read_exact(&mut buf).ok()?;
        Some((
            u64::from_be_bytes(buf[0..8].try_into().ok()?) as i64,
            u64::from_be_bytes(buf[8..16].try_into().ok()?) as i64,
        ))
    } else {
        let mut buf = [0u8; 8];
        file.read_exact(&mut buf).ok()?;
        Some((
            u32::from_be_bytes(buf[0..4].try_into().ok()?) as i64,
            u32::from_be_bytes(buf[4..8].try_into().ok()?) as i64,
        ))
    }
}

fn to_local(media_time: i64) -> Option<NaiveDateTime> {
    let unix = media_time.checked_sub(EPOCH_OFFSET_SECONDS)?;
    if !(MIN_UNIX..=MAX_UNIX).contains(&unix) {
        return None;
    }
    DateTime::from_timestamp(unix, 0).map(|dt| dt.with_timezone(&Local).naive_local())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn boxed(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(payload);
        out
    }

    fn raw(unix: i64) -> u32 {
        (unix + EPOCH_OFFSET_SECONDS) as u32
    }

    fn mp4(created_raw: u32, modified_raw: u32) -> Vec<u8> {
        let mut mvhd = vec![0u8; 4];
        mvhd.extend_from_slice(&created_raw.to_be_bytes());
        mvhd.extend_from_slice(&modified_raw.to_be_bytes());
        mvhd.extend_from_slice(&1000u32.to_be_bytes());
        mvhd.extend_from_slice(&5000u32.to_be_bytes());

        let mut out = boxed(b"ftyp", b"isom\0\0\0\0");
        out.extend_from_slice(&boxed(b"free", &[0u8; 4]));
        out.extend_from_slice(&boxed(b"moov", &boxed(b"mvhd", &mvhd)));
        out
    }

    fn expected(unix: i64) -> NaiveDateTime {
        DateTime::from_timestamp(unix, 0)
            .unwrap()
            .with_timezone(&Local)
            .naive_local()
    }

    fn keys(names: &[&str]) -> Vec<u8> {
        let mut payload = vec![0u8; 4];
        payload.extend_from_slice(&(names.len() as u32).to_be_bytes());
        for name in names {
            payload.extend_from_slice(&((name.len() + 8) as u32).to_be_bytes());
            payload.extend_from_slice(b"mdta");
            payload.extend_from_slice(name.as_bytes());
        }
        boxed(b"keys", &payload)
    }

    fn ilst(index: u32, value: &str) -> Vec<u8> {
        let mut data = 1u32.to_be_bytes().to_vec();
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(value.as_bytes());
        let data = boxed(b"data", &data);

        let mut entry = ((data.len() + 8) as u32).to_be_bytes().to_vec();
        entry.extend_from_slice(&index.to_be_bytes());
        entry.extend_from_slice(&data);
        boxed(b"ilst", &entry)
    }

    fn quicktime(value: &str, mvhd_unix: i64) -> Vec<u8> {
        let mut mvhd = vec![0u8; 4];
        mvhd.extend_from_slice(&raw(mvhd_unix).to_be_bytes());
        mvhd.extend_from_slice(&raw(mvhd_unix).to_be_bytes());
        mvhd.extend_from_slice(&1000u32.to_be_bytes());
        mvhd.extend_from_slice(&5000u32.to_be_bytes());

        let mut meta = boxed(b"hdlr", &[0u8; 24]);
        meta.extend_from_slice(&keys(&["com.apple.quicktime.make", CREATION_KEY]));
        meta.extend_from_slice(&ilst(2, value));

        let mut moov = boxed(b"mvhd", &mvhd);
        moov.extend_from_slice(&boxed(b"meta", &meta));

        let mut out = boxed(b"ftyp", b"qt      ");
        out.extend_from_slice(&boxed(b"moov", &moov));
        out
    }

    #[test]
    fn prefers_the_wall_clock_time_the_camera_recorded() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("clip.mov");
        fs::write(&path, quicktime("2019-05-14T09:22:03+0200", 1_557_818_523)).unwrap();

        let found = detect(&path).unwrap();
        assert_eq!(found.taken, at(2019, 5, 14, 9, 22, 3));
        assert_eq!(
            found.info.as_deref(),
            Some("com.apple.quicktime.creationdate +02:00")
        );
    }

    #[test]
    fn reads_a_wall_clock_time_written_with_a_colon_in_the_offset() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("clip.mov");
        fs::write(&path, quicktime("2019-05-14T09:22:03-07:00", 1_557_818_523)).unwrap();

        assert_eq!(detect(&path).unwrap().taken, at(2019, 5, 14, 9, 22, 3));
    }

    #[test]
    fn falls_back_to_mvhd_when_the_recorded_time_makes_no_sense() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("clip.mov");
        fs::write(&path, quicktime("not a date", 1_304_665_689)).unwrap();

        let found = detect(&path).unwrap();
        assert_eq!(found.info.as_deref(), Some("mvhd.creation_time"));
        assert_eq!(found.taken, expected(1_304_665_689));
    }

    fn at(y: i32, m: u32, d: u32, hh: u32, mm: u32, ss: u32) -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(hh, mm, ss)
            .unwrap()
    }

    #[test]
    fn reads_creation_time_from_mvhd() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("clip.mp4");
        fs::write(&path, mp4(raw(1_304_665_689), raw(1_400_000_000))).unwrap();

        let found = detect(&path).unwrap();
        assert_eq!(found.provider, Provider::Media);
        assert_eq!(found.info.as_deref(), Some("mvhd.creation_time"));
        assert_eq!(found.taken, expected(1_304_665_689));
    }

    #[test]
    fn falls_back_to_modification_time_when_creation_is_unset() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("clip.mov");
        fs::write(&path, mp4(0, raw(1_400_000_000))).unwrap();

        let found = detect(&path).unwrap();
        assert_eq!(found.info.as_deref(), Some("mvhd.modification_time"));
        assert_eq!(found.taken, expected(1_400_000_000));
    }

    #[test]
    fn returns_none_when_there_is_no_moov_box() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("clip.mp4");
        fs::write(&path, boxed(b"ftyp", b"isom\0\0\0\0")).unwrap();

        assert!(detect(&path).is_none());
    }

    #[test]
    fn skips_files_without_a_media_extension() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("clip.bin");
        fs::write(&path, mp4(raw(1_304_665_689), raw(1_400_000_000))).unwrap();

        assert!(detect(&path).is_none());
    }
}
