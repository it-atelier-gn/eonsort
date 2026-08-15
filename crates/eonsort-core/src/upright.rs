use crate::error::{Error, Result};
use crate::rotate::Transform;
use crate::weights::{self, Weight};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

pub use crate::weights::Progress as UprightProgress;

pub const WEIGHTS: [Weight; 1] = [Weight {
    repo: "lmz/candle-yolo-v8",
    revision: "be388c6fab95ae3035a039070e1b883b9c5a1325",
    file: "yolov8n.safetensors",
    bytes: 6_369_332,
}];

pub const INPUT_SIZE: u32 = 640;
pub const CONFIDENCE: f32 = 0.35;
pub const NMS_THRESHOLD: f32 = 0.45;
pub const CONFIDENT: f32 = 1.2;
pub const MIN_SCORE: f32 = 0.5;

pub const NOTHING: &str = "nothing recognisable";

pub const TURNS: [Transform; 4] = [
    Transform::None,
    Transform::Rotate90,
    Transform::Rotate180,
    Transform::Rotate270,
];

pub const PORTRAIT_CLASSES: [&str; 9] = [
    "person",
    "bottle",
    "chair",
    "pottedplant",
    "traffic light",
    "fire hydrant",
    "parking meter",
    "refrigerator",
    "vase",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Detection {
    pub label: String,
    pub confidence: f32,
    pub bounds: [f32; 4],
}

impl Detection {
    pub fn width(&self) -> f32 {
        (self.bounds[2] - self.bounds[0]).abs()
    }

    pub fn height(&self) -> f32 {
        (self.bounds[3] - self.bounds[1]).abs()
    }

    pub fn stands_upright(&self) -> bool {
        PORTRAIT_CLASSES.contains(&self.label.as_str()) && self.height() > self.width()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Guess {
    pub transform: Transform,
    pub confidence: f32,
    pub reason: String,
}

impl Guess {
    pub fn nothing() -> Self {
        Self {
            transform: Transform::None,
            confidence: 0.0,
            reason: NOTHING.to_string(),
        }
    }
}

pub fn score(detections: &[Detection]) -> f32 {
    detections
        .iter()
        .map(|found| {
            let base = found.confidence * found.confidence;
            let prior = if found.stands_upright() {
                0.5 * found.confidence
            } else {
                0.0
            };
            base + prior
        })
        .sum()
}

pub fn reason(detections: &[Detection]) -> String {
    if detections.is_empty() {
        return NOTHING.to_string();
    }

    let mut counts: Vec<(String, usize)> = Vec::new();
    for found in detections {
        match counts.iter_mut().find(|(label, _)| label == &found.label) {
            Some((_, seen)) => *seen += 1,
            None => counts.push((found.label.clone(), 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    counts.truncate(3);

    counts
        .iter()
        .map(|(label, seen)| format!("{seen} {}", plural(label, *seen)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn plural(label: &str, count: usize) -> String {
    if count == 1 {
        return label.to_string();
    }
    match label {
        "person" => "people".to_string(),
        "bus" | "sandwich" => format!("{label}es"),
        "skis" | "scissors" => label.to_string(),
        _ => format!("{label}s"),
    }
}

pub fn choose(candidates: &[(Transform, Vec<Detection>)]) -> Guess {
    let mut best: Option<(Transform, f32, &Vec<Detection>)> = None;
    for (transform, detections) in candidates {
        let found = score(detections);
        let better = match best {
            Some((_, so_far, _)) => found > so_far,
            None => true,
        };
        if better {
            best = Some((*transform, found, detections));
        }
    }

    match best {
        Some((transform, found, detections)) if found >= MIN_SCORE => Guess {
            transform,
            confidence: found,
            reason: reason(detections),
        },
        _ => Guess::nothing(),
    }
}

pub fn path_of(dir: &Path, weight: &Weight) -> PathBuf {
    weights::path_of(dir, weight)
}

pub fn total_bytes() -> u64 {
    weights::total_bytes(&WEIGHTS)
}

pub fn present_bytes(dir: &Path) -> u64 {
    weights::present_bytes(dir, &WEIGHTS)
}

pub fn installed(dir: &Path) -> bool {
    weights::installed(dir, &WEIGHTS)
}

pub fn remove(dir: &Path) -> Result<()> {
    weights::remove(dir, &WEIGHTS)
}

#[cfg(not(feature = "upright"))]
pub fn download(
    _dir: &Path,
    _cancel: &AtomicBool,
    _on_progress: &dyn Fn(UprightProgress),
) -> Result<()> {
    Err(Error::Upright(
        "this build was made without the upright model".into(),
    ))
}

#[cfg(feature = "upright")]
pub fn download(
    dir: &Path,
    cancel: &AtomicBool,
    on_progress: &dyn Fn(UprightProgress),
) -> Result<()> {
    weights::download(dir, &WEIGHTS, cancel, on_progress)
}

#[derive(Debug)]
pub struct Detector {
    #[cfg(feature = "upright")]
    model: crate::yolo::YoloV8,
}

#[cfg(not(feature = "upright"))]
impl Detector {
    pub fn load(_dir: &Path) -> Result<Detector> {
        Err(Error::Upright(
            "this build was made without the upright model".into(),
        ))
    }

    pub fn guess(&self, _source: &Path) -> Result<Guess> {
        Err(Error::Upright(
            "this build was made without the upright model".into(),
        ))
    }

    pub fn objects(&self, _source: &Path) -> Result<Vec<Detection>> {
        Err(Error::Upright(
            "this build was made without the upright model".into(),
        ))
    }
}

#[cfg(feature = "upright")]
fn oops(error: impl std::fmt::Display) -> Error {
    Error::Upright(error.to_string())
}

#[cfg(feature = "upright")]
impl Detector {
    pub fn load(dir: &Path) -> Result<Detector> {
        use crate::yolo::{Multiples, YoloV8, CLASSES};
        use candle_core::{DType, Device};
        use candle_nn::VarBuilder;

        let file = path_of(dir, &WEIGHTS[0]);
        if !file.exists() {
            return Err(Error::Upright(
                "the upright model has not been downloaded yet".into(),
            ));
        }

        let builder = unsafe {
            VarBuilder::from_mmaped_safetensors(&[file], DType::F32, &Device::Cpu).map_err(oops)?
        };
        let model = YoloV8::load(builder, Multiples::n(), CLASSES.len()).map_err(oops)?;
        Ok(Detector { model })
    }

    pub fn guess(&self, source: &Path) -> Result<Guess> {
        let base = scaled(source)?;
        let as_found = self.detect(&base)?;
        if score(&as_found) >= CONFIDENT {
            return Ok(choose(&[(Transform::None, as_found)]));
        }

        let mut candidates = vec![(Transform::None, as_found)];
        for turn in &TURNS[1..] {
            let turned = match turn {
                Transform::Rotate90 => image::imageops::rotate90(&base),
                Transform::Rotate180 => image::imageops::rotate180(&base),
                _ => image::imageops::rotate270(&base),
            };
            candidates.push((*turn, self.detect(&turned)?));
        }
        Ok(choose(&candidates))
    }

    pub fn objects(&self, source: &Path) -> Result<Vec<Detection>> {
        let base = scaled(source)?;
        self.detect(&base)
    }

    fn detect(&self, content: &image::RgbImage) -> Result<Vec<Detection>> {
        use candle_core::{Device, Module, Tensor};
        use candle_transformers::object_detection::{non_maximum_suppression, Bbox};

        let (canvas, pad_x, pad_y) = letterbox(content);
        let edge = INPUT_SIZE as usize;
        let mut planes = Vec::with_capacity(3 * edge * edge);
        for channel in 0..3 {
            for pixel in canvas.pixels() {
                planes.push(f32::from(pixel[channel]) / 255.0);
            }
        }

        let input = Tensor::from_vec(planes, (1, 3, edge, edge), &Device::Cpu).map_err(oops)?;
        let prediction = self
            .model
            .forward(&input)
            .map_err(oops)?
            .squeeze(0)
            .map_err(oops)?;

        let (rows, anchors) = prediction.dims2().map_err(oops)?;
        let values: Vec<f32> = prediction
            .flatten_all()
            .map_err(oops)?
            .to_vec1()
            .map_err(oops)?;

        let classes = rows.saturating_sub(4);
        let mut buckets: Vec<Vec<Bbox<()>>> = vec![Vec::new(); classes];
        for anchor in 0..anchors {
            let at = |row: usize| values[row * anchors + anchor];
            let mut best = 0usize;
            let mut confidence = 0f32;
            for class in 0..classes {
                let found = at(4 + class);
                if found > confidence {
                    confidence = found;
                    best = class;
                }
            }
            if confidence <= CONFIDENCE {
                continue;
            }
            let (cx, cy, w, h) = (at(0), at(1), at(2), at(3));
            buckets[best].push(Bbox {
                xmin: cx - w / 2.0,
                ymin: cy - h / 2.0,
                xmax: cx + w / 2.0,
                ymax: cy + h / 2.0,
                confidence,
                data: (),
            });
        }
        non_maximum_suppression(&mut buckets, NMS_THRESHOLD);

        let width = content.width() as f32;
        let height = content.height() as f32;
        let (pad_x, pad_y) = (pad_x as f32, pad_y as f32);
        let mut found = Vec::new();
        for (class, boxes) in buckets.iter().enumerate() {
            let label = crate::yolo::CLASSES.get(class).copied().unwrap_or("thing");
            for area in boxes {
                found.push(Detection {
                    label: label.to_string(),
                    confidence: area.confidence,
                    bounds: [
                        ((area.xmin - pad_x) / width).clamp(0.0, 1.0),
                        ((area.ymin - pad_y) / height).clamp(0.0, 1.0),
                        ((area.xmax - pad_x) / width).clamp(0.0, 1.0),
                        ((area.ymax - pad_y) / height).clamp(0.0, 1.0),
                    ],
                });
            }
        }
        Ok(found)
    }
}

#[cfg(feature = "upright")]
fn scaled(source: &Path) -> Result<image::RgbImage> {
    let picture =
        image::open(source).map_err(|e| Error::Upright(format!("{}: {e}", source.display())))?;
    Ok(picture
        .resize(
            INPUT_SIZE,
            INPUT_SIZE,
            image::imageops::FilterType::Triangle,
        )
        .to_rgb8())
}

#[cfg(feature = "upright")]
fn letterbox(content: &image::RgbImage) -> (image::RgbImage, u32, u32) {
    const PADDING: u8 = 114;

    let mut canvas = image::RgbImage::from_pixel(INPUT_SIZE, INPUT_SIZE, image::Rgb([PADDING; 3]));
    let pad_x = INPUT_SIZE.saturating_sub(content.width()) / 2;
    let pad_y = INPUT_SIZE.saturating_sub(content.height()) / 2;
    image::imageops::replace(&mut canvas, content, i64::from(pad_x), i64::from(pad_y));
    (canvas, pad_x, pad_y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(label: &str, confidence: f32, width: f32, height: f32) -> Detection {
        Detection {
            label: label.to_string(),
            confidence,
            bounds: [0.1, 0.1, 0.1 + width, 0.1 + height],
        }
    }

    #[test]
    fn the_weights_are_pinned_to_a_revision() {
        for weight in &WEIGHTS {
            assert_eq!(weight.revision.len(), 40, "{}", weight.file);
            assert!(weight.file.ends_with(".safetensors"));
            assert!(weight.bytes > 1_000_000);
        }
        assert_eq!(total_bytes(), 6_369_332);
    }

    #[test]
    fn a_standing_person_scores_higher_than_a_lying_one() {
        let standing = vec![found("person", 0.9, 0.2, 0.6)];
        let lying = vec![found("person", 0.9, 0.6, 0.2)];
        assert!(score(&standing) > score(&lying));
    }

    #[test]
    fn the_upright_prior_only_helps_classes_that_stand_up() {
        let tall_dog = vec![found("dog", 0.8, 0.2, 0.6)];
        let wide_dog = vec![found("dog", 0.8, 0.6, 0.2)];
        assert_eq!(score(&tall_dog), score(&wide_dog));
    }

    #[test]
    fn more_and_surer_things_score_higher() {
        let one = vec![found("dog", 0.6, 0.3, 0.3)];
        let two = vec![found("dog", 0.6, 0.3, 0.3), found("cat", 0.6, 0.3, 0.3)];
        let surer = vec![found("dog", 0.95, 0.3, 0.3)];
        assert!(score(&two) > score(&one));
        assert!(score(&surer) > score(&one));
    }

    #[test]
    fn a_sideways_picture_is_turned_towards_the_reading_that_stands_up() {
        let candidates = vec![
            (Transform::None, vec![found("person", 0.5, 0.6, 0.2)]),
            (Transform::Rotate90, vec![found("person", 0.92, 0.2, 0.6)]),
            (Transform::Rotate180, vec![]),
            (Transform::Rotate270, vec![found("person", 0.4, 0.6, 0.2)]),
        ];
        let guess = choose(&candidates);
        assert_eq!(guess.transform, Transform::Rotate90);
        assert_eq!(guess.reason, "1 person");
    }

    #[test]
    fn nothing_recognisable_is_left_alone() {
        let guess = choose(&[
            (Transform::None, vec![]),
            (Transform::Rotate90, vec![]),
            (Transform::Rotate180, vec![]),
            (Transform::Rotate270, vec![]),
        ]);
        assert_eq!(guess, Guess::nothing());
        assert_eq!(choose(&[]), Guess::nothing());
    }

    #[test]
    fn a_faint_reading_is_not_enough_to_turn_a_picture() {
        let faint = vec![found("dog", 0.4, 0.3, 0.3)];
        assert!(score(&faint) < MIN_SCORE);
        let guess = choose(&[(Transform::Rotate90, faint)]);
        assert_eq!(guess.transform, Transform::None);
    }

    #[test]
    fn a_tie_leaves_the_picture_as_it_is() {
        let both = vec![found("person", 0.9, 0.2, 0.6)];
        let guess = choose(&[
            (Transform::None, both.clone()),
            (Transform::Rotate180, both),
        ]);
        assert_eq!(guess.transform, Transform::None);
    }

    #[test]
    fn the_guess_is_never_a_mirror() {
        for turn in TURNS {
            assert!(!matches!(
                turn,
                Transform::FlipH | Transform::FlipV | Transform::Transpose | Transform::Transverse
            ));
        }
    }

    #[test]
    fn the_reason_counts_what_was_seen() {
        let seen = vec![
            found("person", 0.9, 0.2, 0.6),
            found("person", 0.8, 0.2, 0.6),
            found("chair", 0.7, 0.2, 0.3),
        ];
        assert_eq!(reason(&seen), "2 people, 1 chair");
        assert_eq!(reason(&[]), NOTHING);
    }

    #[test]
    fn the_reason_names_at_most_three_kinds_of_thing() {
        let seen: Vec<Detection> = ["dog", "cat", "bird", "boat", "kite"]
            .iter()
            .map(|label| found(label, 0.6, 0.2, 0.2))
            .collect();
        assert_eq!(reason(&seen).split(", ").count(), 3);
    }

    #[test]
    fn a_confident_upright_reading_clears_the_bar_a_lying_one_does_not() {
        let standing = vec![found("person", 0.9, 0.2, 0.6)];
        let lying = vec![found("person", 0.9, 0.6, 0.2)];
        assert!(score(&standing) >= CONFIDENT);
        assert!(score(&lying) < CONFIDENT);
    }

    #[cfg(not(feature = "upright"))]
    #[test]
    fn a_build_without_the_feature_says_so_rather_than_pretending() {
        let error = Detector::load(Path::new(".")).unwrap_err();
        assert!(
            error.to_string().contains("without the upright model"),
            "{error}"
        );
    }

    #[test]
    fn nothing_is_installed_in_an_empty_folder() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!installed(dir.path()));
        assert_eq!(present_bytes(dir.path()), 0);
        remove(dir.path()).unwrap();
    }
}
