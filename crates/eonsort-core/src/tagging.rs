use crate::error::Result;
use crate::weights::Weight;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

pub use crate::weights::Progress as TagProgress;

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

pub const SIDE: usize = 224;
pub const WORDS: usize = 64;

pub const VOCABULARY: [&str; 72] = [
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
        words: Tensor,
        vocabulary: Vec<Vec<f32>>,
        tokenizer: Tokenizer,
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
            let model = siglip::Model::new(&config, builder).map_err(stalled)?;
            let tokenizer = Tokenizer::from_file(&vocabulary_file).map_err(stalled)?;

            let words = spoken(&tokenizer, &VOCABULARY, &device)?;
            let features = model.get_text_features(&words).map_err(stalled)?;
            let vocabulary = rows(&features)?;

            Ok(Self {
                model,
                device,
                words,
                vocabulary,
                tokenizer,
            })
        }

        pub fn look(&self, path: &Path) -> Result<Sighting> {
            let pixels = planes(path, &self.device)?;
            let features = self.model.get_image_features(&pixels).map_err(stalled)?;
            let mut vector = rows(&features)?.into_iter().next().unwrap_or_default();
            crate::tags::normalise(&mut vector);

            let scores: Vec<f32> = self
                .vocabulary
                .iter()
                .map(|word| crate::tags::cosine(&vector, word))
                .collect();

            Ok(Sighting {
                tags: crate::tags::top_tags(&VOCABULARY, &scores),
                vector,
            })
        }

        pub fn phrase(&self, words: &str) -> Result<Vec<f32>> {
            let asked = spoken(&self.tokenizer, &[words], &self.device)?;
            let features = self.model.get_text_features(&asked).map_err(stalled)?;
            let mut vector = rows(&features)?.into_iter().next().unwrap_or_default();
            crate::tags::normalise(&mut vector);
            Ok(vector)
        }

        pub fn vocabulary_size(&self) -> usize {
            self.words.dims()[0]
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
        let opened = image::open(path).map_err(stalled)?;
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

    pub fn phrase(&self, _words: &str) -> Result<Vec<f32>> {
        Err(crate::error::Error::Tagging(
            "this build was made without the tagging model".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
