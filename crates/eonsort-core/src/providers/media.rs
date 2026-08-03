use super::{extension_lowercase, Detection, Provider};
use chrono::{DateTime, Local, NaiveDateTime};
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

pub fn detect(path: &Path) -> Option<Detection> {
    let ext = extension_lowercase(path)?;
    if !EXTENSIONS.contains(&ext.as_str()) {
        return None;
    }

    let mut file = File::open(path).ok()?;
    let end = file.metadata().ok()?.len();
    let (moov_start, moov_end) = find_box(&mut file, 0, end, b"moov")?;
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
