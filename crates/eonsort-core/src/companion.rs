use crate::error::Result;
use crate::model::{destination_with_subject, PlanEntry};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const PICTURE: [&str; 22] = [
    "jpg", "jpeg", "jpe", "png", "webp", "tif", "tiff", "bmp", "gif", "avif", "heic", "heif",
    "hif", "dng", "cr2", "cr3", "nef", "arw", "orf", "rw2", "raf", "pef",
];
const VIDEO: [&str; 13] = [
    "mp4", "mov", "m4v", "avi", "mkv", "webm", "3gp", "3g2", "mts", "m2ts", "wmv", "f4v", "mj2",
];
const AUDIO: [&str; 6] = ["m4a", "m4b", "wav", "mp3", "aac", "flac"];
const SIDECAR: [&str; 5] = ["xmp", "aae", "json", "thm", "lrv"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Other,
    Sidecar,
    Audio,
    Video,
    Picture,
}

fn kind(extension: &str) -> Kind {
    if PICTURE.contains(&extension) {
        Kind::Picture
    } else if VIDEO.contains(&extension) {
        Kind::Video
    } else if AUDIO.contains(&extension) {
        Kind::Audio
    } else if SIDECAR.contains(&extension) {
        Kind::Sidecar
    } else {
        Kind::Other
    }
}

fn extension_of(name: &str) -> &str {
    name.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("")
}

pub fn group_key(path: &Path) -> Option<(PathBuf, String)> {
    let parent = path.parent()?.to_path_buf();
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();

    let (stem, extension) = name.rsplit_once('.')?;
    let stem = if SIDECAR.contains(&extension) {
        stem.rsplit_once('.').map(|(head, _)| head).unwrap_or(stem)
    } else {
        stem
    };

    Some((parent, stem.to_string()))
}

pub fn pair(entries: &mut [PlanEntry], root: &Path, folder_pattern: &str) -> Result<usize> {
    let mut groups: HashMap<(PathBuf, String), Vec<usize>> = HashMap::new();
    for (index, entry) in entries.iter().enumerate() {
        if let Some(key) = group_key(&entry.source) {
            groups.entry(key).or_default().push(index);
        }
    }

    let mut changed = 0;
    for members in groups.into_values() {
        if members.len() < 2 {
            continue;
        }
        let Some(master) = master_of(entries, &members) else {
            continue;
        };

        let taken = entries[master].taken;
        let provider = entries[master].provider;
        let beside = entries[master]
            .source
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();

        for index in members {
            if index == master || entries[index].taken == taken {
                continue;
            }
            let entry = &mut entries[index];
            entry.destination = destination_with_subject(
                &entry.source,
                taken,
                entry.subject.as_deref(),
                root,
                folder_pattern,
            )?;
            entry.taken = taken;
            entry.provider = provider;
            entry.provider_info = Some(format!("beside {beside}"));
            entry.flags.clear();
            changed += 1;
        }
    }

    Ok(changed)
}

fn rank(entry: &PlanEntry) -> (Kind, i64, std::cmp::Reverse<String>) {
    let name = entry
        .source
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    (
        kind(extension_of(&name)),
        entry.provider.trust_rank(),
        std::cmp::Reverse(name),
    )
}

fn master_of(entries: &[PlanEntry], members: &[usize]) -> Option<usize> {
    let best = *members.iter().max_by_key(|index| rank(&entries[**index]))?;
    (rank(&entries[best]).0 > Kind::Sidecar).then_some(best)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::Provider;
    use chrono::{NaiveDate, NaiveDateTime};

    fn at(y: i32, m: u32, d: u32, hh: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(hh, 0, 0)
            .unwrap()
    }

    fn entry(source: &str, taken: NaiveDateTime, provider: Provider) -> PlanEntry {
        PlanEntry {
            source: PathBuf::from(source),
            destination: PathBuf::from("/out/old/x"),
            taken,
            provider,
            provider_info: None,
            size: 1,
            candidates: Vec::new(),
            flags: Vec::new(),
            subject: None,
            tags: Vec::new(),
            caption: None,
            orientation: 0,
            rotate: Default::default(),
            rotate_reason: None,
            reencode: false,
        }
    }

    fn run(entries: &mut [PlanEntry]) -> usize {
        pair(entries, Path::new("/out"), "%Y/%m").unwrap()
    }

    #[test]
    fn a_live_photo_video_follows_its_picture() {
        let mut entries = [
            entry("/src/IMG_1234.HEIC", at(2019, 5, 14, 9), Provider::Exif),
            entry("/src/IMG_1234.MOV", at(2023, 1, 1, 3), Provider::Filesystem),
        ];

        assert_eq!(run(&mut entries), 1);
        assert_eq!(entries[1].taken, at(2019, 5, 14, 9));
        assert_eq!(entries[1].provider, Provider::Exif);
        assert_eq!(
            entries[1].provider_info.as_deref(),
            Some("beside IMG_1234.HEIC")
        );
        assert_eq!(
            entries[1].destination,
            PathBuf::from("/out/2019/05/IMG_1234.MOV")
        );
    }

    #[test]
    fn a_sidecar_never_sets_the_date_for_the_picture() {
        let mut entries = [
            entry("/src/IMG_1234.JPG", at(2019, 5, 14, 9), Provider::Exif),
            entry(
                "/src/IMG_1234.JPG.xmp",
                at(2024, 2, 2, 2),
                Provider::Filesystem,
            ),
            entry("/src/IMG_1234.AAE", at(2024, 2, 2, 2), Provider::Filesystem),
        ];

        assert_eq!(run(&mut entries), 2);
        assert_eq!(entries[0].taken, at(2019, 5, 14, 9));
        assert_eq!(entries[1].taken, at(2019, 5, 14, 9));
        assert_eq!(entries[2].taken, at(2019, 5, 14, 9));
    }

    #[test]
    fn a_takeout_sidecar_lands_with_the_picture_it_describes() {
        let mut entries = [
            entry("/src/IMG_1234.JPG", at(2019, 5, 14, 9), Provider::Takeout),
            entry(
                "/src/IMG_1234.JPG.json",
                at(2024, 2, 2, 2),
                Provider::Filesystem,
            ),
        ];

        assert_eq!(run(&mut entries), 1);
        assert_eq!(
            entries[1].destination,
            PathBuf::from("/out/2019/05/IMG_1234.JPG.json")
        );
    }

    #[test]
    fn the_flags_of_a_re_dated_companion_are_dropped() {
        let mut entries = [
            entry("/src/IMG_1234.HEIC", at(2019, 5, 14, 9), Provider::Exif),
            entry("/src/IMG_1234.MOV", at(2003, 1, 1, 0), Provider::Media),
        ];
        entries[1].flags.push(crate::suspect::Flag::CameraEpoch);

        run(&mut entries);
        assert!(entries[1].flags.is_empty());
    }

    #[test]
    fn files_that_only_share_a_folder_are_left_alone() {
        let mut entries = [
            entry("/src/IMG_1234.JPG", at(2019, 5, 14, 9), Provider::Exif),
            entry("/src/IMG_9999.JPG", at(2021, 7, 4, 9), Provider::Exif),
        ];

        assert_eq!(run(&mut entries), 0);
        assert_eq!(entries[1].taken, at(2021, 7, 4, 9));
    }

    #[test]
    fn a_numbered_copy_is_its_own_file() {
        let mut entries = [
            entry("/src/IMG_1234.JPG", at(2019, 5, 14, 9), Provider::Exif),
            entry("/src/IMG_1234(1).JPG", at(2021, 7, 4, 9), Provider::Exif),
        ];

        assert_eq!(run(&mut entries), 0);
    }

    #[test]
    fn a_pair_that_already_agrees_is_not_touched() {
        let shared = at(2019, 5, 14, 9);
        let mut entries = [
            entry("/src/IMG_1234.HEIC", shared, Provider::Exif),
            entry("/src/IMG_1234.MOV", shared, Provider::Media),
        ];

        assert_eq!(run(&mut entries), 0);
        assert_eq!(entries[1].destination, PathBuf::from("/out/old/x"));
    }

    #[test]
    fn a_raw_and_its_jpeg_end_up_on_one_date() {
        let mut entries = [
            entry("/src/DSC0001.JPG", at(2020, 3, 14, 9), Provider::Filename),
            entry("/src/DSC0001.CR2", at(2020, 3, 14, 8), Provider::Exif),
        ];

        assert_eq!(run(&mut entries), 1);
        assert_eq!(entries[0].taken, at(2020, 3, 14, 8));
        assert_eq!(entries[1].taken, at(2020, 3, 14, 8));
    }
}
