use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const BURST_GAP_SECONDS: i64 = 10;
pub const NEAR_DUPLICATE_BITS: u32 = 10;
pub const LOOKALIKE_BITS: u32 = 6;
pub const LOOKALIKE_BANDS: u32 = 8;
pub const HASH_SIDE: u32 = 32;
pub const HASH_BITS: usize = 64;
const LOW_FREQUENCY_SIDE: usize = 8;

const HASHABLE: [&str; 13] = [
    "jpg", "jpeg", "jpe", "png", "webp", "tif", "tiff", "bmp", "gif", "avif", "heic", "heif", "hif",
];

pub fn hashable(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let lowered = e.to_ascii_lowercase();
            HASHABLE.contains(&lowered.as_str()) || crate::raw::is_raw_extension(&lowered)
        })
        .unwrap_or(false)
}

pub fn fingerprint(path: &Path) -> Option<u64> {
    if !hashable(path) {
        return None;
    }
    let image = crate::imageio::open(path)?;
    Some(perceptual(&image))
}

pub fn perceptual(image: &image::DynamicImage) -> u64 {
    let small = image
        .resize_exact(HASH_SIDE, HASH_SIDE, image::imageops::FilterType::Triangle)
        .to_luma8();

    let side = HASH_SIDE as usize;
    let mut values = vec![0.0f64; side * side];
    for (index, value) in values.iter_mut().enumerate() {
        *value = f64::from(
            small
                .get_pixel((index % side) as u32, (index / side) as u32)
                .0[0],
        );
    }

    let coefficients = dct_2d(&values, side);
    let mut low = Vec::with_capacity(HASH_BITS);
    for row in 0..LOW_FREQUENCY_SIDE {
        for column in 0..LOW_FREQUENCY_SIDE {
            low.push(coefficients[row * side + column]);
        }
    }

    let median = median_of(&low[1..]);
    let mut bits = 0u64;
    for (index, value) in low.iter().enumerate() {
        if *value > median {
            bits |= 1 << index;
        }
    }
    bits
}

fn dct_2d(values: &[f64], side: usize) -> Vec<f64> {
    let basis = basis_table(side);
    let mut rows = vec![0.0f64; side * side];
    for row in 0..side {
        for frequency in 0..side {
            let mut sum = 0.0;
            for column in 0..side {
                sum += values[row * side + column] * basis[frequency * side + column];
            }
            rows[row * side + frequency] = sum;
        }
    }

    let mut out = vec![0.0f64; side * side];
    for column in 0..side {
        for frequency in 0..side {
            let mut sum = 0.0;
            for row in 0..side {
                sum += rows[row * side + column] * basis[frequency * side + row];
            }
            out[frequency * side + column] = sum;
        }
    }
    out
}

fn basis_table(side: usize) -> Vec<f64> {
    let mut table = vec![0.0f64; side * side];
    for frequency in 0..side {
        for position in 0..side {
            table[frequency * side + position] =
                (std::f64::consts::PI * frequency as f64 * (2.0 * position as f64 + 1.0)
                    / (2.0 * side as f64))
                    .cos();
        }
    }
    table
}

fn median_of(values: &[f64]) -> f64 {
    let mut sorted: Vec<f64> = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if sorted.is_empty() {
        return 0.0;
    }
    let middle = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    } else {
        sorted[middle]
    }
}

pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

pub fn fingerprint_all(items: &[(PathBuf, NaiveDateTime, u64)]) -> Vec<BurstFacts> {
    fingerprint_all_rated(items, &|_| None)
}

pub fn fingerprint_all_rated(
    items: &[(PathBuf, NaiveDateTime, u64)],
    rated: &(dyn Fn(&Path) -> Option<f32> + Sync),
) -> Vec<BurstFacts> {
    use rayon::prelude::*;

    items
        .par_iter()
        .map(|(source, taken, size)| BurstFacts {
            hash: fingerprint(source),
            rating: rated(source),
            source: source.clone(),
            taken: *taken,
            size: *size,
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct BurstFacts {
    pub source: PathBuf,
    pub taken: NaiveDateTime,
    pub size: u64,
    pub hash: Option<u64>,
    pub rating: Option<f32>,
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

fn rated(value: Option<f32>) -> f32 {
    value.filter(|v| v.is_finite()).unwrap_or(f32::NEG_INFINITY)
}

fn close(bursts: &mut Vec<Burst>, facts: &[BurstFacts], run: &[usize]) {
    if run.len() < 2 {
        return;
    }
    let keeper = run
        .iter()
        .copied()
        .max_by(|&a, &b| {
            rated(facts[a].rating)
                .total_cmp(&rated(facts[b].rating))
                .then(facts[a].size.cmp(&facts[b].size))
                .then(facts[b].source.cmp(&facts[a].source))
        })
        .unwrap();

    bursts.push(Burst {
        keeper: facts[keeper].source.clone(),
        members: run.iter().map(|&i| facts[i].source.clone()).collect(),
    });
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lookalike {
    pub keeper: PathBuf,
    pub members: Vec<PathBuf>,
    pub folders: usize,
}

impl Lookalike {
    pub fn others(&self) -> impl Iterator<Item = &PathBuf> {
        self.members.iter().filter(move |m| **m != self.keeper)
    }
}

struct Rings {
    parent: Vec<usize>,
}

impl Rings {
    fn new(size: usize) -> Self {
        Rings {
            parent: (0..size).collect(),
        }
    }

    fn find(&mut self, mut node: usize) -> usize {
        while self.parent[node] != node {
            self.parent[node] = self.parent[self.parent[node]];
            node = self.parent[node];
        }
        node
    }

    fn join(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent[rb] = ra;
        }
    }
}

fn band_of(hash: u64, band: u32) -> u64 {
    let width = 64 / LOOKALIKE_BANDS;
    (hash >> (band * width)) & ((1u64 << width) - 1)
}

pub fn group_lookalikes(
    facts: &[BurstFacts],
    digest: &dyn Fn(&Path) -> Option<String>,
) -> Vec<Lookalike> {
    use std::collections::HashMap;

    let seen: Vec<usize> = (0..facts.len())
        .filter(|&i| facts[i].hash.is_some())
        .collect();
    if seen.len() < 2 {
        return Vec::new();
    }

    let mut rings = Rings::new(facts.len());
    for band in 0..LOOKALIKE_BANDS {
        let mut buckets: HashMap<u64, Vec<usize>> = HashMap::new();
        for &index in &seen {
            let key = band_of(facts[index].hash.unwrap(), band);
            buckets.entry(key).or_default().push(index);
        }
        for members in buckets.values() {
            for (offset, &left) in members.iter().enumerate() {
                for &right in &members[offset + 1..] {
                    if hamming(facts[left].hash.unwrap(), facts[right].hash.unwrap())
                        <= LOOKALIKE_BITS
                    {
                        rings.join(left, right);
                    }
                }
            }
        }
    }

    let mut grouped: HashMap<usize, Vec<usize>> = HashMap::new();
    for &index in &seen {
        let root = rings.find(index);
        grouped.entry(root).or_default().push(index);
    }

    let mut out = Vec::new();
    for mut members in grouped.into_values() {
        if members.len() < 2 {
            continue;
        }
        members.sort_by_key(|&i| facts[i].source.clone());
        let kept = one_per_content(facts, &members, digest);
        if kept.len() < 2 {
            continue;
        }
        let folders = distinct_folders(facts, &kept);
        if folders < 2 {
            continue;
        }
        let keeper = kept
            .iter()
            .copied()
            .max_by_key(|&i| (facts[i].size, std::cmp::Reverse(facts[i].source.clone())))
            .unwrap();
        out.push(Lookalike {
            keeper: facts[keeper].source.clone(),
            members: kept.iter().map(|&i| facts[i].source.clone()).collect(),
            folders,
        });
    }

    out.sort_by(|a, b| {
        b.members
            .len()
            .cmp(&a.members.len())
            .then(a.keeper.cmp(&b.keeper))
    });
    out
}

fn one_per_content(
    facts: &[BurstFacts],
    members: &[usize],
    digest: &dyn Fn(&Path) -> Option<String>,
) -> Vec<usize> {
    use std::collections::HashSet;

    let mut held: HashSet<String> = HashSet::new();
    let mut kept = Vec::new();
    for &index in members {
        match digest(&facts[index].source) {
            Some(hash) => {
                if held.insert(hash) {
                    kept.push(index);
                }
            }
            None => kept.push(index),
        }
    }
    kept
}

fn distinct_folders(facts: &[BurstFacts], members: &[usize]) -> usize {
    use std::collections::HashSet;

    members
        .iter()
        .map(|&i| facts[i].source.parent().unwrap_or_else(|| Path::new("")))
        .collect::<HashSet<&Path>>()
        .len()
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
            rating: None,
        }
    }

    fn shot_rated(name: &str, second: u32, size: u64, hash: u64, rating: f32) -> BurstFacts {
        BurstFacts {
            rating: Some(rating),
            ..shot(name, second, size, hash)
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
                rating: None,
            },
            BurstFacts {
                source: PathBuf::from("/two/b.jpg"),
                taken: at(1),
                size: 100,
                hash: Some(0),
                rating: None,
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
                rating: None,
            },
            BurstFacts {
                source: PathBuf::from("/cam/other.mp4"),
                taken: at(1),
                size: 100,
                hash: None,
                rating: None,
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

    fn anywhere(name: &str, folder: &str, size: u64, hash: u64) -> BurstFacts {
        BurstFacts {
            source: PathBuf::from(format!("/{folder}/{name}")),
            taken: at(0),
            size,
            hash: Some(hash),
            rating: None,
        }
    }

    fn unhashed(path: &str) -> Option<String> {
        Some(path.to_string())
    }

    #[test]
    fn finds_the_same_picture_saved_into_three_different_folders() {
        let facts = vec![
            anywhere("IMG_1.jpg", "phone", 3_000_000, 0x0F0F_0F0F_0F0F_0F0F),
            anywhere("IMG_1.jpg", "whatsapp", 200_000, 0x0F0F_0F0F_0F0F_0F0E),
            anywhere("copy.jpg", "backup", 900_000, 0x0F0F_0F0F_0F0F_0F0B),
        ];

        let found = group_lookalikes(&facts, &|p| unhashed(&p.to_string_lossy()));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].members.len(), 3);
        assert_eq!(found[0].folders, 3);
        assert!(found[0].keeper.ends_with("phone/IMG_1.jpg"));
    }

    #[test]
    fn a_run_inside_one_folder_is_left_to_the_burst_list() {
        let facts = vec![
            anywhere("a.jpg", "cam", 100, 0x0F0F_0F0F_0F0F_0F0F),
            anywhere("b.jpg", "cam", 100, 0x0F0F_0F0F_0F0F_0F0E),
        ];

        assert!(group_lookalikes(&facts, &|p| unhashed(&p.to_string_lossy())).is_empty());
    }

    #[test]
    fn files_holding_the_same_bytes_are_left_to_the_identical_list() {
        let facts = vec![
            anywhere("a.jpg", "one", 100, 0x0F0F_0F0F_0F0F_0F0F),
            anywhere("a.jpg", "two", 100, 0x0F0F_0F0F_0F0F_0F0F),
        ];

        assert!(group_lookalikes(&facts, &|_| Some("same".to_string())).is_empty());
    }

    #[test]
    fn one_re_encoded_copy_beside_two_identical_ones_is_still_reported() {
        let facts = vec![
            anywhere("a.jpg", "one", 100, 0x0F0F_0F0F_0F0F_0F0F),
            anywhere("a.jpg", "two", 100, 0x0F0F_0F0F_0F0F_0F0F),
            anywhere("a.jpg", "three", 900, 0x0F0F_0F0F_0F0F_0F0B),
        ];

        let digest = |p: &Path| {
            if p.to_string_lossy().contains("three") {
                Some("other".to_string())
            } else {
                Some("same".to_string())
            }
        };
        let found = group_lookalikes(&facts, &digest);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].members.len(), 2);
    }

    #[test]
    fn pictures_that_only_look_roughly_alike_are_not_grouped() {
        let facts = vec![
            anywhere("a.jpg", "one", 100, 0x0000_0000_0000_0000),
            anywhere("b.jpg", "two", 100, 0xFFFF_FFFF_FFFF_FFFF),
        ];

        assert!(group_lookalikes(&facts, &|p| unhashed(&p.to_string_lossy())).is_empty());
    }

    #[test]
    fn a_picture_no_hash_could_be_read_from_is_skipped() {
        let mut facts = vec![
            anywhere("a.jpg", "one", 100, 0x0F0F_0F0F_0F0F_0F0F),
            anywhere("b.jpg", "two", 100, 0x0F0F_0F0F_0F0F_0F0F),
        ];
        facts[1].hash = None;

        assert!(group_lookalikes(&facts, &|p| unhashed(&p.to_string_lossy())).is_empty());
    }

    #[test]
    fn every_band_covers_the_hash_once() {
        let hash = 0x0123_4567_89AB_CDEFu64;
        let mut rebuilt = 0u64;
        for band in 0..LOOKALIKE_BANDS {
            rebuilt |= band_of(hash, band) << (band * (64 / LOOKALIKE_BANDS));
        }
        assert_eq!(rebuilt, hash);
    }

    fn scene(width: u32, height: u32) -> image::DynamicImage {
        let mut pixels = image::RgbImage::new(width, height);
        for (x, y, pixel) in pixels.enumerate_pixels_mut() {
            let fx = x as f32 / width as f32;
            let fy = y as f32 / height as f32;
            let blob = (((fx - 0.3).powi(2) + (fy - 0.4).powi(2)) * 40.0)
                .exp()
                .min(255.0);
            *pixel = image::Rgb([(fx * 220.0) as u8, (fy * 200.0) as u8, (255.0 - blob) as u8]);
        }
        image::DynamicImage::ImageRgb8(pixels)
    }

    fn reencoded(image: &image::DynamicImage, quality: u8) -> image::DynamicImage {
        let mut body = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut body, quality);
        encoder.encode_image(image).unwrap();
        image::load_from_memory_with_format(&body, image::ImageFormat::Jpeg).unwrap()
    }

    #[test]
    fn the_same_picture_saved_again_at_a_lower_quality_still_matches() {
        let original = scene(400, 300);
        let squeezed = reencoded(&original, 40);

        let apart = hamming(perceptual(&original), perceptual(&squeezed));
        assert!(apart <= LOOKALIKE_BITS, "{apart} bits apart");
    }

    #[test]
    fn the_same_picture_at_half_the_size_still_matches() {
        let original = scene(400, 300);
        let smaller = original.resize_exact(200, 150, image::imageops::FilterType::Lanczos3);

        let apart = hamming(perceptual(&original), perceptual(&smaller));
        assert!(apart <= LOOKALIKE_BITS, "{apart} bits apart");
    }

    #[test]
    fn two_different_pictures_stay_far_apart() {
        let left = scene(400, 300);
        let mut pixels = image::RgbImage::new(400, 300);
        for (x, y, pixel) in pixels.enumerate_pixels_mut() {
            *pixel = image::Rgb([(y % 255) as u8, 20, (x % 255) as u8]);
        }
        let right = image::DynamicImage::ImageRgb8(pixels);

        let apart = hamming(perceptual(&left), perceptual(&right));
        assert!(apart > LOOKALIKE_BITS, "{apart} bits apart");
    }

    #[test]
    fn a_picture_is_always_identical_to_itself() {
        let image = scene(120, 90);
        assert_eq!(perceptual(&image), perceptual(&image));
        assert_eq!(hamming(perceptual(&image), perceptual(&image)), 0);
    }

    #[test]
    fn a_flat_picture_still_produces_a_hash_rather_than_panicking() {
        let flat = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            40,
            40,
            image::Rgb([128, 128, 128]),
        ));
        let _ = perceptual(&flat);
    }

    #[test]
    fn raw_files_are_hashable_now_that_their_previews_can_be_read() {
        assert!(hashable(Path::new("/photos/IMG_1.CR2")));
        assert!(hashable(Path::new("/photos/IMG_1.nef")));
        assert!(hashable(Path::new("/photos/IMG_1.jpg")));
        assert!(!hashable(Path::new("/photos/notes.txt")));
    }

    #[test]
    fn the_median_is_taken_across_an_even_and_an_odd_number_of_values() {
        assert_eq!(median_of(&[1.0, 2.0, 3.0]), 2.0);
        assert_eq!(median_of(&[1.0, 2.0, 3.0, 4.0]), 2.5);
        assert_eq!(median_of(&[]), 0.0);
    }

    #[test]
    fn a_picture_with_no_pattern_to_it_sets_about_half_the_bits() {
        let mut pixels = image::RgbImage::new(128, 128);
        let mut noise: u32 = 0x9E37_79B9;
        for pixel in pixels.pixels_mut() {
            noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *pixel = image::Rgb([(noise >> 16) as u8; 3]);
        }

        let set = perceptual(&image::DynamicImage::ImageRgb8(pixels)).count_ones();
        assert!((24..=40).contains(&set), "{set} bits set");
    }

    fn cropped(image: &image::DynamicImage, percent: u32) -> image::DynamicImage {
        let inset_x = image.width() * percent / 200;
        let inset_y = image.height() * percent / 200;
        image.crop_imm(
            inset_x,
            inset_y,
            image.width() - inset_x * 2,
            image.height() - inset_y * 2,
        )
    }

    #[test]
    fn a_picture_framed_differently_is_a_different_picture() {
        let full = scene(800, 600);
        let base = perceptual(&full);

        for percent in [10u32, 20, 30, 50] {
            let apart = hamming(base, perceptual(&cropped(&full, percent)));
            assert!(
                apart > LOOKALIKE_BITS,
                "{percent}% trimmed away sits {apart} bits from the whole frame"
            );
        }
    }

    #[test]
    fn trimming_the_very_edge_still_counts_as_the_same_picture() {
        let full = scene(800, 600);
        let base = perceptual(&full);

        for percent in [1u32, 2, 5] {
            let apart = hamming(base, perceptual(&cropped(&full, percent)));
            assert!(
                apart <= LOOKALIKE_BITS,
                "{percent}% trimmed away sits {apart} bits from the whole frame"
            );
        }
    }

    #[test]
    fn a_burst_keeps_the_best_looking_shot() {
        let run = vec![
            shot_rated("a.jpg", 0, 9_000_000, 0b0, 4.1),
            shot_rated("b.jpg", 2, 1_000_000, 0b1, 6.8),
            shot_rated("c.jpg", 4, 5_000_000, 0b11, 5.2),
        ];

        let bursts = group_bursts(&run);
        assert_eq!(bursts.len(), 1);
        assert!(
            bursts[0].keeper.ends_with("b.jpg"),
            "the best rated shot keeps, not the biggest file: {:?}",
            bursts[0].keeper
        );
    }

    #[test]
    fn without_ratings_the_biggest_file_still_keeps() {
        let run = vec![
            shot("a.jpg", 0, 1_000_000, 0b0),
            shot("b.jpg", 2, 9_000_000, 0b1),
        ];

        let bursts = group_bursts(&run);
        assert!(
            bursts[0].keeper.ends_with("b.jpg"),
            "{:?}",
            bursts[0].keeper
        );
    }

    #[test]
    fn an_unrated_shot_never_beats_a_rated_one() {
        let run = vec![
            shot("big.jpg", 0, 9_000_000, 0b0),
            shot_rated("rated.jpg", 2, 1_000, 0b1, 3.0),
        ];

        let bursts = group_bursts(&run);
        assert!(
            bursts[0].keeper.ends_with("rated.jpg"),
            "{:?}",
            bursts[0].keeper
        );
    }
}
