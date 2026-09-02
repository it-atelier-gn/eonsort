use crate::error::Result;
use crate::weights::Weight;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

pub use crate::weights::Progress as TagProgress;

pub const CREDIT: &str = if cfg!(feature = "tagging") {
    "Tagging by SigLIP, Apache 2.0, Copyright (c) Google LLC"
} else {
    ""
};

const REPO: &str = "google/siglip-base-patch16-224";
const REVISION: &str = "7fd15f0689c79d79e38b1c2e2e2370a7bf2761ed";

pub const WEIGHTS: [Weight; 2] = [
    Weight {
        repo: REPO,
        revision: REVISION,
        file: "model.safetensors",
        bytes: 812_672_320,
    },
    Weight {
        repo: REPO,
        revision: REVISION,
        file: "tokenizer.json",
        bytes: 2_399_357,
    },
];

pub fn stamp() -> String {
    let name = REPO.rsplit('/').next().unwrap_or(REPO);
    format!("{name}@{}", &REVISION[..12.min(REVISION.len())])
}

pub const SIDE: usize = 224;
pub const WORDS: usize = 64;
pub const PROMPT_PREFIX: &str = "a photo of ";

pub fn vocabulary_stamp() -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(PROMPT_PREFIX.as_bytes());
    hasher.update(REVISION.as_bytes());
    for word in &VOCABULARY {
        hasher.update(word.as_bytes());
        hasher.update(b"\0");
    }
    hasher.finalize().to_hex()[..16].to_string()
}

pub fn vocabulary_cache(dir: &Path) -> PathBuf {
    dir.join(format!("vocabulary-{}.bin", vocabulary_stamp()))
}

pub const VOCABULARY: [&str; 279] = [
    "a forest",
    "a mountain",
    "a beach",
    "the sea",
    "a lake or river",
    "a field or meadow",
    "a desert",
    "snow",
    "a garden",
    "flowers",
    "a tree",
    "a sunset",
    "the night sky",
    "clouds",
    "rain",
    "a city street",
    "a village",
    "a bridge",
    "a church",
    "a castle",
    "a house",
    "a room indoors",
    "a kitchen",
    "a restaurant",
    "a shop",
    "an office",
    "a museum",
    "a station or airport",
    "a road",
    "a car",
    "a bicycle",
    "a boat",
    "a train",
    "an aeroplane",
    "a dog",
    "a cat",
    "a horse",
    "a bird",
    "a farm animal",
    "a wild animal",
    "a fish",
    "an insect",
    "one person",
    "a group of people",
    "a child",
    "a baby",
    "a portrait of a face",
    "a crowd",
    "a wedding",
    "a birthday party",
    "a concert",
    "a sports game",
    "someone swimming",
    "someone hiking",
    "someone skiing",
    "someone dancing",
    "food on a plate",
    "a cake",
    "a drink",
    "a book or newspaper",
    "a document or receipt",
    "a screenshot",
    "a computer screen",
    "a phone",
    "a painting or drawing",
    "a statue",
    "a sign with writing",
    "a christmas tree",
    "fireworks",
    "a campfire",
    "a toy",
    "a piece of furniture",
    "a waterfall",
    "a glacier",
    "a cave",
    "a canyon",
    "a valley",
    "a cliff",
    "an island",
    "a volcano",
    "sand dunes",
    "a rainforest",
    "a moor",
    "rocks and boulders",
    "a harbour",
    "a pier",
    "a swimming pool",
    "a sunrise",
    "the stars",
    "the moon",
    "a storm",
    "lightning",
    "a rainbow",
    "fog or mist",
    "the northern lights",
    "autumn leaves",
    "blossom on a tree",
    "a cactus",
    "a palm tree",
    "a vineyard",
    "a field of crops",
    "a mushroom",
    "a plant in a pot",
    "a bouquet",
    "a city skyline",
    "a cathedral",
    "a mosque",
    "a temple",
    "ruins",
    "a block of flats",
    "a skyscraper",
    "a lighthouse",
    "a windmill",
    "a barn",
    "a market",
    "a playground",
    "a cemetery",
    "a monument",
    "a fountain",
    "a staircase",
    "a narrow alley",
    "a tunnel",
    "a building site",
    "a car park",
    "a fence or wall",
    "a doorway",
    "a window",
    "a bathroom",
    "a bedroom",
    "a living room",
    "a cafe",
    "a bar",
    "a supermarket",
    "a classroom",
    "a library",
    "an art gallery",
    "a hospital",
    "a hotel room",
    "a gym",
    "a theatre",
    "inside a church",
    "a workshop",
    "a laboratory",
    "a corridor",
    "a balcony",
    "a motorcycle",
    "a sailing boat",
    "a ship",
    "a tram",
    "a bus",
    "a lorry",
    "a helicopter",
    "a hot air balloon",
    "a tractor",
    "a railway track",
    "traffic",
    "a cow",
    "a sheep",
    "a goat",
    "a pig",
    "a chicken",
    "a duck",
    "an owl",
    "a bird of prey",
    "a seagull",
    "a butterfly",
    "a bee",
    "a spider",
    "a deer",
    "a fox",
    "a bear",
    "a monkey",
    "an elephant",
    "a lizard",
    "a snake",
    "a turtle",
    "a squirrel",
    "a rabbit",
    "a frog",
    "a jellyfish",
    "a couple",
    "a family",
    "a selfie",
    "someone smiling",
    "an older person",
    "people around a table",
    "a person seen from behind",
    "someone asleep",
    "someone at work",
    "a team photograph",
    "a festival",
    "a parade",
    "a graduation",
    "a funeral",
    "a protest",
    "a fairground",
    "a picnic",
    "a campsite",
    "a market stall",
    "a christening",
    "a party at night",
    "a school event",
    "someone running",
    "someone cycling",
    "someone climbing",
    "someone fishing",
    "someone surfing",
    "someone sailing",
    "someone playing football",
    "someone playing an instrument",
    "someone singing",
    "someone cooking",
    "someone reading",
    "someone painting",
    "someone riding a horse",
    "someone at a computer",
    "someone taking a photograph",
    "someone gardening",
    "someone shopping",
    "someone snowboarding",
    "someone rowing",
    "someone skateboarding",
    "bread",
    "a pizza",
    "fruit",
    "vegetables",
    "a salad",
    "a plate of meat",
    "a dessert",
    "ice cream",
    "a cup of coffee",
    "a glass of wine",
    "a glass of beer",
    "a cocktail",
    "a barbecue",
    "a table laid for a meal",
    "handwriting",
    "a map",
    "a clock",
    "jewellery",
    "a wristwatch",
    "a camera",
    "a musical instrument",
    "tools",
    "machinery",
    "a bicycle wheel",
    "a pair of shoes",
    "clothes",
    "a suitcase",
    "a box or parcel",
    "a candle",
    "balloons",
    "a wrapped present",
    "a flag",
    "a poster",
    "a chalkboard",
    "a keyboard",
    "a television",
    "a mirror",
    "a lamp",
    "a rug or carpet",
    "cutlery",
    "a bottle",
    "a basket",
    "a close-up of a texture",
    "a reflection in water",
    "a shadow",
    "a silhouette",
    "a black and white photograph",
    "a blurred photograph",
    "a dark photograph",
    "an aerial view",
    "a panorama",
    "a macro photograph of a flower",
    "halloween pumpkins",
    "easter eggs",
    "a decorated room",
    "a snowman",
    "a nativity scene",
];

pub fn path_of(dir: &Path, weight: &Weight) -> PathBuf {
    crate::weights::path_of(dir, weight)
}

pub fn total_bytes() -> u64 {
    crate::weights::total_bytes(&WEIGHTS)
}

pub fn present_bytes(dir: &Path) -> u64 {
    crate::weights::present_bytes(dir, &WEIGHTS)
}

pub fn installed(dir: &Path) -> bool {
    crate::weights::installed(dir, &WEIGHTS)
}

pub fn remove(dir: &Path) -> Result<()> {
    crate::weights::remove(dir, &WEIGHTS)
}

#[cfg(feature = "tagging")]
pub fn download(dir: &Path, cancel: &AtomicBool, on_progress: &dyn Fn(TagProgress)) -> Result<()> {
    crate::weights::download(dir, &WEIGHTS, cancel, on_progress)
}

#[cfg(not(feature = "tagging"))]
pub fn download(
    _dir: &Path,
    _cancel: &AtomicBool,
    _on_progress: &dyn Fn(TagProgress),
) -> Result<()> {
    Err(crate::error::Error::Tagging(
        "this build was made without the tagging model".into(),
    ))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sighting {
    pub tags: Vec<String>,
    pub vector: Vec<f32>,
}

#[cfg(feature = "tagging")]
mod real {
    use super::*;
    use crate::error::Error;
    use candle_core::{DType, Device, Tensor};
    use candle_nn::VarBuilder;
    use candle_transformers::models::siglip;
    use tokenizers::Tokenizer;

    const PADDING: u32 = 1;

    fn stalled(e: impl std::fmt::Display) -> Error {
        Error::Tagging(e.to_string())
    }

    pub struct Tagger {
        model: siglip::Model,
        device: Device,
        vocabulary: Vec<Vec<f32>>,
        tokenizer: Tokenizer,
        scale: f32,
        bias: f32,
    }

    impl Tagger {
        pub fn load(dir: &Path) -> Result<Self> {
            let device = Device::Cpu;
            let config = siglip::Config::base_patch16_224();

            let weights = path_of(dir, &WEIGHTS[0]);
            let vocabulary_file = path_of(dir, &WEIGHTS[1]);

            let builder = unsafe {
                VarBuilder::from_mmaped_safetensors(&[weights], DType::F32, &device)
                    .map_err(stalled)?
            };
            let scale = scalar(&builder.get(&[1], "logit_scale").map_err(stalled)?)?.exp();
            let bias = scalar(&builder.get(&[1], "logit_bias").map_err(stalled)?)?;
            let model = siglip::Model::new(&config, builder).map_err(stalled)?;
            let tokenizer = Tokenizer::from_file(&vocabulary_file).map_err(stalled)?;

            let cache = vocabulary_cache(dir);
            let vocabulary = match read_vocabulary(&cache) {
                Some(held) => held,
                None => {
                    let templated: Vec<String> = VOCABULARY
                        .iter()
                        .map(|word| format!("{PROMPT_PREFIX}{word}."))
                        .collect();
                    let asked: Vec<&str> = templated.iter().map(String::as_str).collect();
                    let spelled = spoken(&tokenizer, &asked, &device)?;
                    let features = model.get_text_features(&spelled).map_err(stalled)?;
                    let mut built = rows(&features)?;
                    for vector in &mut built {
                        crate::tags::normalise(vector);
                    }
                    write_vocabulary(&cache, &built);
                    built
                }
            };

            Ok(Self {
                model,
                device,
                vocabulary,
                tokenizer,
                scale,
                bias,
            })
        }

        pub fn look(&self, path: &Path) -> Result<Sighting> {
            let pixels = planes(path, &self.device)?;
            let features = self.model.get_image_features(&pixels).map_err(stalled)?;
            let mut vector = rows(&features)?.into_iter().next().unwrap_or_default();
            crate::tags::normalise(&mut vector);

            Ok(Sighting {
                tags: self.project(&vector),
                vector,
            })
        }

        pub fn project(&self, vector: &[f32]) -> Vec<String> {
            crate::tags::top_tags(&VOCABULARY, &self.scores(vector))
        }

        pub fn scores(&self, vector: &[f32]) -> Vec<f32> {
            self.vocabulary
                .iter()
                .map(|word| {
                    crate::tags::confidence(
                        crate::tags::cosine(vector, word),
                        self.scale,
                        self.bias,
                    )
                })
                .collect()
        }

        pub fn phrase(&self, words: &str) -> Result<Vec<f32>> {
            let asked = spoken(&self.tokenizer, &[words], &self.device)?;
            let features = self.model.get_text_features(&asked).map_err(stalled)?;
            let mut vector = rows(&features)?.into_iter().next().unwrap_or_default();
            crate::tags::normalise(&mut vector);
            Ok(vector)
        }

        pub fn vocabulary_size(&self) -> usize {
            self.vocabulary.len()
        }
    }

    fn scalar(tensor: &Tensor) -> Result<f32> {
        let flat = tensor.flatten_all().map_err(stalled)?;
        let values = flat.to_vec1::<f32>().map_err(stalled)?;
        values
            .first()
            .copied()
            .ok_or_else(|| Error::Tagging("the model holds no calibration".into()))
    }

    pub(crate) fn read_vocabulary(path: &Path) -> Option<Vec<Vec<f32>>> {
        let body = std::fs::read(path).ok()?;
        if body.len() < 8 {
            return None;
        }
        let count = u32::from_le_bytes(body[0..4].try_into().ok()?) as usize;
        let width = u32::from_le_bytes(body[4..8].try_into().ok()?) as usize;
        if count != VOCABULARY.len() || width == 0 {
            return None;
        }
        if body.len() != 8 + count * width * 4 {
            return None;
        }

        let mut held = Vec::with_capacity(count);
        for row in 0..count {
            let mut vector = Vec::with_capacity(width);
            for column in 0..width {
                let at = 8 + (row * width + column) * 4;
                vector.push(f32::from_le_bytes(body[at..at + 4].try_into().ok()?));
            }
            held.push(vector);
        }
        Some(held)
    }

    pub(crate) fn write_vocabulary(path: &Path, vocabulary: &[Vec<f32>]) {
        let Some(width) = vocabulary.first().map(Vec::len) else {
            return;
        };
        if vocabulary.iter().any(|row| row.len() != width) {
            return;
        }

        let mut body = Vec::with_capacity(8 + vocabulary.len() * width * 4);
        body.extend_from_slice(&(vocabulary.len() as u32).to_le_bytes());
        body.extend_from_slice(&(width as u32).to_le_bytes());
        for row in vocabulary {
            for value in row {
                body.extend_from_slice(&value.to_le_bytes());
            }
        }

        let part = path.with_extension("part");
        if std::fs::write(&part, &body).is_ok() {
            let _ = std::fs::rename(&part, path);
            sweep_old_caches(path);
        }
    }

    pub(crate) fn sweep_old_caches(keep: &Path) {
        let Some(dir) = keep.parent() else {
            return;
        };
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path == keep {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("vocabulary-") && name.ends_with(".bin") {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    fn spoken(tokenizer: &Tokenizer, phrases: &[&str], device: &Device) -> Result<Tensor> {
        let mut rows = Vec::with_capacity(phrases.len());
        for phrase in phrases {
            let encoded = tokenizer.encode(*phrase, true).map_err(stalled)?;
            let mut ids = encoded.get_ids().to_vec();
            ids.resize(WORDS, PADDING);
            rows.push(ids);
        }
        let flat: Vec<u32> = rows.concat();
        Tensor::from_vec(flat, (phrases.len(), WORDS), device).map_err(stalled)
    }

    fn planes(path: &Path, device: &Device) -> Result<Tensor> {
        let opened = crate::imageio::open(path).ok_or_else(|| {
            Error::Tagging(format!("{}: cannot be read as a picture", path.display()))
        })?;
        let scaled = opened
            .resize_exact(
                SIDE as u32,
                SIDE as u32,
                image::imageops::FilterType::Triangle,
            )
            .to_rgb8();

        let mut planes = vec![0f32; 3 * SIDE * SIDE];
        for (x, y, pixel) in scaled.enumerate_pixels() {
            let at = y as usize * SIDE + x as usize;
            for channel in 0..3 {
                planes[channel * SIDE * SIDE + at] = pixel.0[channel] as f32 / 127.5 - 1.0;
            }
        }

        Tensor::from_vec(planes, (1, 3, SIDE, SIDE), device).map_err(stalled)
    }

    fn rows(features: &Tensor) -> Result<Vec<Vec<f32>>> {
        features
            .to_dtype(DType::F32)
            .map_err(stalled)?
            .to_vec2::<f32>()
            .map_err(stalled)
    }
}

#[cfg(feature = "tagging")]
pub use real::Tagger;

#[cfg(not(feature = "tagging"))]
#[derive(Debug)]
pub struct Tagger;

#[cfg(not(feature = "tagging"))]
impl Tagger {
    pub fn load(_dir: &Path) -> Result<Self> {
        Err(crate::error::Error::Tagging(
            "this build was made without the tagging model".into(),
        ))
    }

    pub fn look(&self, _path: &Path) -> Result<Sighting> {
        Err(crate::error::Error::Tagging(
            "this build was made without the tagging model".into(),
        ))
    }

    pub fn project(&self, _vector: &[f32]) -> Vec<String> {
        Vec::new()
    }

    pub fn phrase(&self, _words: &str) -> Result<Vec<f32>> {
        Err(crate::error::Error::Tagging(
            "this build was made without the tagging model".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "tagging")]
    use super::real::{read_vocabulary, write_vocabulary};
    use super::*;

    #[test]
    #[cfg(feature = "tagging")]
    fn the_credit_names_the_licence_and_the_author() {
        assert!(CREDIT.contains("Apache"), "{CREDIT}");
        assert!(CREDIT.contains("Google"), "{CREDIT}");
        assert!(CREDIT.contains("SigLIP"), "{CREDIT}");
    }

    #[test]
    fn every_weight_is_pinned_to_a_revision() {
        for weight in &WEIGHTS {
            assert_eq!(weight.revision.len(), 40, "{}", weight.file);
            assert!(weight.bytes > 0, "{}", weight.file);
        }
    }

    #[test]
    fn the_download_is_about_eight_hundred_megabytes() {
        let total = total_bytes();
        assert!(total > 700 * 1024 * 1024, "{total}");
        assert!(total < 900 * 1024 * 1024, "{total}");
    }

    #[test]
    fn nothing_is_installed_in_an_empty_folder() {
        let dir = std::env::temp_dir().join("eonsort-tagging-empty");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!installed(&dir));
        assert_eq!(present_bytes(&dir), 0);
    }

    #[test]
    fn the_vocabulary_is_plain_lowercase_english() {
        for word in &VOCABULARY {
            assert!(!word.is_empty());
            assert_eq!(*word, word.to_lowercase(), "{word}");
        }
    }

    #[test]
    fn no_word_is_offered_twice() {
        let mut seen = VOCABULARY.to_vec();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), before);
    }

    #[test]
    fn every_word_fits_the_sequence_the_model_expects() {
        for word in &VOCABULARY {
            assert!(word.split_whitespace().count() < WORDS, "{word}");
        }
    }

    #[cfg(not(feature = "tagging"))]
    #[test]
    fn a_build_without_the_tagger_says_so_plainly() {
        let dir = std::env::temp_dir();
        let error = Tagger::load(&dir).unwrap_err().to_string();
        assert!(error.contains("without the tagging model"), "{error}");
    }
    #[test]
    fn the_stamp_names_the_model_and_the_revision_behind_it() {
        let stamp = stamp();
        assert!(stamp.starts_with("siglip-base-patch16-224@"), "{stamp}");
        assert!(stamp.contains("7fd15f0689c7"), "{stamp}");
        assert_eq!(stamp, stamp, "the stamp must not wander between calls");
    }

    #[test]
    fn every_word_still_reads_as_a_label_once_the_article_is_dropped() {
        for word in &VOCABULARY {
            let bare = word
                .strip_prefix("a ")
                .or_else(|| word.strip_prefix("an "))
                .or_else(|| word.strip_prefix("the "))
                .unwrap_or(word);
            assert!(!bare.trim().is_empty(), "{word}");
            assert!(!word.contains("  "), "{word}");
            assert_eq!(*word, word.trim(), "{word}");
            assert!(!word.ends_with('.') && !word.ends_with(','), "{word}");
        }
    }

    #[test]
    fn the_vocabulary_reaches_past_the_handful_it_started_with() {
        assert!(VOCABULARY.len() > 200, "{}", VOCABULARY.len());
    }

    #[test]
    fn the_everyday_subjects_of_a_photo_archive_are_all_in_there() {
        for wanted in [
            "a dog",
            "a cat",
            "a wedding",
            "a beach",
            "a pizza",
            "a screenshot",
            "a selfie",
            "a family",
            "a butterfly",
            "someone cooking",
            "a snowman",
            "an aerial view",
        ] {
            assert!(VOCABULARY.contains(&wanted), "{wanted} is missing");
        }
    }

    #[test]
    fn growing_the_vocabulary_asks_for_a_fresh_projection() {
        let before = crate::tags::projection_stamp(&["a dog", "a cat"]);
        let after = crate::tags::projection_stamp(&["a dog", "a cat", "a fox"]);
        assert_ne!(before, after);
        assert_eq!(
            crate::tags::projection_stamp(&VOCABULARY),
            crate::tags::projection_stamp(&VOCABULARY)
        );
    }

    #[cfg(feature = "tagging")]
    #[test]
    #[ignore = "needs the downloaded model; run with EONSORT_MODELS set"]
    fn tags_real_pictures_with_the_whole_vocabulary() {
        let Ok(dir) = std::env::var("EONSORT_MODELS") else {
            return;
        };

        let started = std::time::Instant::now();
        let tagger = Tagger::load(Path::new(&dir)).expect("the model should load");
        println!("load {:?}, {} words", started.elapsed(), VOCABULARY.len());
        assert_eq!(tagger.vocabulary_size(), VOCABULARY.len());

        let mut pictures: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(folder) = std::env::var("EONSORT_PICTURES") {
            let mut found: Vec<std::path::PathBuf> = walkdir::WalkDir::new(&folder)
                .into_iter()
                .filter_map(std::result::Result::ok)
                .filter(|e| e.file_type().is_file())
                .map(|e| e.path().to_path_buf())
                .filter(|p| {
                    matches!(
                        p.extension()
                            .and_then(|e| e.to_str())
                            .map(str::to_lowercase)
                            .as_deref(),
                        Some("jpg") | Some("jpeg") | Some("png")
                    )
                })
                .take(4000)
                .collect();
            found.sort();
            let want: usize = std::env::var("EONSORT_SAMPLE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(24);
            let step = (found.len() / want.max(1)).max(1);
            pictures = found.into_iter().step_by(step).take(want).collect();
        }
        if pictures.is_empty() {
            pictures.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/screenshot.png"));
        }

        let mut every: Vec<f32> = Vec::new();
        let mut bare = 0usize;

        for picture in &pictures {
            let Ok(sighting) = tagger.look(picture) else {
                continue;
            };
            let mut scored: Vec<(&str, f32)> = tagger
                .scores(&sighting.vector)
                .into_iter()
                .enumerate()
                .map(|(index, score)| (VOCABULARY[index], score))
                .collect();
            scored.sort_by(|a, b| b.1.total_cmp(&a.1));
            every.extend(scored.iter().map(|(_, p)| *p));
            if sighting.tags.is_empty() {
                bare += 1;
            }

            let shown: Vec<String> = scored
                .iter()
                .take(5)
                .map(|(word, p)| format!("{word} {p:.3}"))
                .collect();
            println!(
                "{:<28} kept {:?}  | top {}",
                picture.file_name().unwrap_or_default().to_string_lossy(),
                sighting.tags,
                shown.join(", ")
            );
        }

        every.sort_by(f32::total_cmp);
        let at = |q: f32| every[((every.len() - 1) as f32 * q) as usize];
        println!(
            "
{} pictures, {} phrase scores: p50 {:.4} p90 {:.4} p99 {:.4} p999 {:.4} max {:.4}",
            pictures.len(),
            every.len(),
            at(0.50),
            at(0.90),
            at(0.99),
            at(0.999),
            every.last().copied().unwrap_or(0.0)
        );
        for cut in [0.001f32, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.3] {
            let kept = every.iter().filter(|p| **p >= cut).count();
            println!(
                "  at {cut:.2}: {:.2} tags per picture",
                kept as f32 / pictures.len() as f32
            );
        }
        println!("{bare} of {} pictures got no tag at all", pictures.len());
    }

    #[test]
    fn the_cache_is_named_after_the_words_it_holds() {
        let stamp = vocabulary_stamp();
        assert_eq!(stamp.len(), 16);
        assert_eq!(stamp, vocabulary_stamp());

        let path = vocabulary_cache(Path::new("/models"));
        assert!(path.to_string_lossy().contains(&stamp));
        assert_eq!(path.extension().unwrap(), "bin");
    }

    #[cfg(feature = "tagging")]
    #[test]
    fn the_cached_words_come_back_exactly_as_they_went_in() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vocabulary.bin");
        let built: Vec<Vec<f32>> = (0..VOCABULARY.len())
            .map(|row| (0..8).map(|col| (row * 8 + col) as f32 / 97.0).collect())
            .collect();

        write_vocabulary(&path, &built);
        assert_eq!(read_vocabulary(&path).unwrap(), built);
    }

    #[cfg(feature = "tagging")]
    #[test]
    fn a_cache_from_a_different_vocabulary_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vocabulary.bin");
        let short: Vec<Vec<f32>> = vec![vec![0.5; 8]; 3];

        write_vocabulary(&path, &short);
        assert!(read_vocabulary(&path).is_none());
    }

    #[cfg(feature = "tagging")]
    #[test]
    fn a_truncated_or_missing_cache_is_refused_rather_than_trusted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vocabulary.bin");
        assert!(read_vocabulary(&path).is_none());

        let built: Vec<Vec<f32>> = vec![vec![0.25; 4]; VOCABULARY.len()];
        write_vocabulary(&path, &built);
        let body = std::fs::read(&path).unwrap();
        std::fs::write(&path, &body[..body.len() - 5]).unwrap();
        assert!(read_vocabulary(&path).is_none());

        std::fs::write(&path, b"no").unwrap();
        assert!(read_vocabulary(&path).is_none());
    }

    #[cfg(feature = "tagging")]
    #[test]
    fn a_cache_of_ragged_rows_is_never_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vocabulary.bin");
        write_vocabulary(&path, &[vec![1.0, 2.0], vec![3.0]]);
        assert!(!path.exists());
    }

    #[test]
    fn every_word_is_asked_about_the_way_the_model_was_trained() {
        assert!(PROMPT_PREFIX.starts_with("a photo of"));
        assert!(PROMPT_PREFIX.ends_with(' '));
        for word in &VOCABULARY {
            let asked = format!("{PROMPT_PREFIX}{word}.");
            assert!(asked.ends_with('.'), "{asked}");
            assert!(asked.split_whitespace().count() < WORDS, "{asked}");
        }
    }

    #[cfg(feature = "tagging")]
    #[test]
    fn writing_a_cache_clears_away_the_one_it_replaces() {
        let dir = tempfile::tempdir().unwrap();
        let stale = dir.path().join("vocabulary-0000000000000000.bin");
        let mine = dir.path().join("vocabulary-1111111111111111.bin");
        let keep = dir.path().join("model.safetensors");
        std::fs::write(&stale, b"old").unwrap();
        std::fs::write(&keep, b"weights").unwrap();

        write_vocabulary(&mine, &vec![vec![0.5; 4]; VOCABULARY.len()]);

        assert!(mine.is_file());
        assert!(!stale.exists());
        assert!(keep.is_file(), "other files are left alone");
    }
}
