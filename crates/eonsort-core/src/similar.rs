use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const BURST_GAP_SECONDS: i64 = 10;
pub const NEAR_DUPLICATE_BITS: u32 = 10;
pub const HASH_WIDTH: u32 = 9;
pub const HASH_HEIGHT: u32 = 8;

const HASHABLE: [&str; 13] = [
    "jpg", "jpeg", "jpe", "png", "webp", "tif", "tiff", "bmp", "gif", "avif", "heic", "heif", "hif",
];

pub fn hashable(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| HASHABLE.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

pub fn fingerprint(path: &Path) -> Option<u64> {
    if !hashable(path) {
        return None;
    }
    let image = crate::imageio::open(path)?;
    let small = image
        .resize_exact(
            HASH_WIDTH,
            HASH_HEIGHT,
            image::imageops::FilterType::Triangle,
        )
        .to_luma8();

    let mut bits = 0u64;
    let mut index = 0;
    for y in 0..HASH_HEIGHT {
        for x in 0..HASH_WIDTH - 1 {
            let left = small.get_pixel(x, y).0[0];
            let right = small.get_pixel(x + 1, y).0[0];
            if left > right {
                bits |= 1 << index;
            }
            index += 1;
        }
    }
    Some(bits)
}

pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

pub fn fingerprint_all(items: &[(PathBuf, NaiveDateTime, u64)]) -> Vec<BurstFacts> {
    use rayon::prelude::*;

    items
        .par_iter()
        .map(|(source, taken, size)| BurstFacts {
            hash: fingerprint(source),
            source: source.clone(),
            taken: *taken,
            size: *size,
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BurstFacts {
    pub source: PathBuf,
    pub taken: NaiveDateTime,
    pub size: u64,
    pub hash: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Burst {
    pub keeper: PathBuf,
    pub members: Vec<PathBuf>,
}

impl Burst {
    pub fn others(&self) -> impl Iterator<Item = &PathBuf> {
        self.members.iter().filter(move |m| **m != self.keeper)
    }
}

pub fn group_bursts(facts: &[BurstFacts]) -> Vec<Burst> {
    let mut by_folder: std::collections::HashMap<&Path, Vec<usize>> =
        std::collections::HashMap::new();
    for (index, entry) in facts.iter().enumerate() {
        if entry.hash.is_none() {
            continue;
        }
        let parent = entry.source.parent().unwrap_or_else(|| Path::new(""));
        by_folder.entry(parent).or_default().push(index);
    }

    let mut bursts = Vec::new();
    for mut indices in by_folder.into_values() {
        indices.sort_by_key(|&i| (facts[i].taken, facts[i].source.clone()));

        let mut run: Vec<usize> = Vec::new();
        for index in indices {
            match run.last() {
                Some(&previous) if continues(&facts[previous], &facts[index]) => run.push(index),
                _ => {
                    close(&mut bursts, facts, &run);
                    run = vec![index];
                }
            }
        }
        close(&mut bursts, facts, &run);
    }

    bursts.sort_by(|a, b| {
        b.members
            .len()
            .cmp(&a.members.len())
            .then(a.keeper.cmp(&b.keeper))
    });
    bursts
}

fn continues(previous: &BurstFacts, next: &BurstFacts) -> bool {
    let (Some(left), Some(right)) = (previous.hash, next.hash) else {
        return false;
    };
    let gap = (next.taken - previous.taken).num_seconds().abs();
    gap <= BURST_GAP_SECONDS && hamming(left, right) <= NEAR_DUPLICATE_BITS
}

fn close(bursts: &mut Vec<Burst>, facts: &[BurstFacts], run: &[usize]) {
    if run.len() < 2 {
        return;
    }
    let keeper = run
        .iter()
        .copied()
        .max_by_key(|&i| (facts[i].size, std::cmp::Reverse(facts[i].source.clone())))
        .unwrap();

    bursts.push(Burst {
        keeper: facts[keeper].source.clone(),
        members: run.iter().map(|&i| facts[i].source.clone()).collect(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(second: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2019, 7, 4)
            .unwrap()
            .and_hms_opt(10, 0, second)
            .unwrap()
    }

    fn shot(name: &str, second: u32, size: u64, hash: u64) -> BurstFacts {
        BurstFacts {
            source: PathBuf::from(format!("/cam/{name}")),
            taken: at(second),
            size,
            hash: Some(hash),
        }
    }

    #[test]
    fn counts_differing_bits() {
        assert_eq!(hamming(0, 0), 0);
        assert_eq!(hamming(0b1011, 0b1001), 1);
        assert_eq!(hamming(u64::MAX, 0), 64);
    }

    #[test]
    fn groups_near_identical_shots_taken_seconds_apart() {
        let facts = vec![
            shot("a.jpg", 0, 100, 0b0000),
            shot("b.jpg", 2, 500, 0b0001),
            shot("c.jpg", 4, 200, 0b0011),
        ];

        let bursts = group_bursts(&facts);
        assert_eq!(bursts.len(), 1);
        assert_eq!(bursts[0].members.len(), 3);
        assert!(bursts[0].keeper.ends_with("b.jpg"));
        assert_eq!(bursts[0].others().count(), 2);
    }

    #[test]
    fn a_long_gap_breaks_the_burst_even_when_the_pictures_match() {
        let facts = vec![shot("a.jpg", 0, 100, 0), shot("b.jpg", 40, 100, 0)];
        assert!(group_bursts(&facts).is_empty());
    }

    #[test]
    fn different_looking_pictures_are_not_a_burst() {
        let facts = vec![
            shot("a.jpg", 0, 100, 0x0000_0000_0000_0000),
            shot("b.jpg", 1, 100, 0xFFFF_FFFF_FFFF_FFFF),
        ];
        assert!(group_bursts(&facts).is_empty());
    }

    #[test]
    fn shots_in_different_folders_never_group() {
        let facts = vec![
            BurstFacts {
                source: PathBuf::from("/one/a.jpg"),
                taken: at(0),
                size: 100,
                hash: Some(0),
            },
            BurstFacts {
                source: PathBuf::from("/two/b.jpg"),
                taken: at(1),
                size: 100,
                hash: Some(0),
            },
        ];
        assert!(group_bursts(&facts).is_empty());
    }

    #[test]
    fn a_lone_shot_is_not_a_burst() {
        assert!(group_bursts(&[shot("only.jpg", 0, 100, 0)]).is_empty());
    }

    #[test]
    fn files_without_a_fingerprint_are_left_out() {
        let facts = vec![
            BurstFacts {
                source: PathBuf::from("/cam/clip.mp4"),
                taken: at(0),
                size: 100,
                hash: None,
            },
            BurstFacts {
                source: PathBuf::from("/cam/other.mp4"),
                taken: at(1),
                size: 100,
                hash: None,
            },
        ];
        assert!(group_bursts(&facts).is_empty());
    }

    #[test]
    fn two_separate_bursts_in_one_folder_stay_separate() {
        let facts = vec![
            shot("a.jpg", 0, 100, 0b0000),
            shot("b.jpg", 1, 100, 0b0001),
            shot("c.jpg", 30, 100, 0b0000),
            shot("d.jpg", 31, 100, 0b0001),
        ];
        assert_eq!(group_bursts(&facts).len(), 2);
    }

    #[test]
    fn only_looks_at_pictures_it_can_decode() {
        assert!(hashable(Path::new("/a/x.JPG")));
        assert!(hashable(Path::new("/a/x.png")));
        assert!(!hashable(Path::new("/a/x.mp4")));
        assert!(!hashable(Path::new("/a/x")));
        assert_eq!(fingerprint(Path::new("/a/x.mp4")), None);
    }

    #[test]
    fn fingerprints_a_real_image_and_matches_a_rescaled_copy() {
        let dir = tempfile::tempdir().unwrap();
        let big = dir.path().join("big.png");
        let small = dir.path().join("small.png");

        let mut canvas = image::RgbImage::new(64, 64);
        for (x, y, pixel) in canvas.enumerate_pixels_mut() {
            let shade = ((x * 4) % 256) as u8;
            *pixel = image::Rgb([shade, shade.wrapping_add(y as u8), shade]);
        }
        canvas.save(&big).unwrap();
        image::DynamicImage::ImageRgb8(canvas)
            .resize_exact(32, 32, image::imageops::FilterType::Triangle)
            .save(&small)
            .unwrap();

        let left = fingerprint(&big).unwrap();
        let right = fingerprint(&small).unwrap();
        assert!(
            hamming(left, right) <= NEAR_DUPLICATE_BITS,
            "a rescaled copy should look the same: {}",
            hamming(left, right)
        );
    }
}
