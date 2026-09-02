use crate::error::Result;
use crate::weights::Weight;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

pub use crate::weights::Progress as QualityProgress;

pub const CREDIT: &str = if cfg!(feature = "quality") {
    "Rating by the LAION aesthetic predictor v1 over OpenAI CLIP, both MIT licensed, packaged by Shunsuke Kitada"
} else {
    ""
};

const REPO: &str = "shunk031/aesthetics-predictor-v1-vit-base-patch32";
const REVISION: &str = "79f02af186a06a53dce2f67b05f427f3d6a84b5e";

pub const WEIGHTS: [Weight; 1] = [Weight {
    repo: REPO,
    revision: REVISION,
    file: "model.safetensors",
    bytes: 351_423_476,
}];

pub const SIDE: usize = 224;
pub const MEAN: [f32; 3] = [0.481_454_66, 0.457_827_5, 0.408_210_73];
pub const DEVIATION: [f32; 3] = [0.268_629_54, 0.261_302_6, 0.275_777_1];

pub const GOOD: f32 = 5.5;
pub const BEAUTIFUL: f32 = 6.2;
pub const RETIRED_TAGS: [&str; 2] = ["a good picture", "a beautiful picture"];

pub fn was_a_rating(tag: &str) -> bool {
    RETIRED_TAGS.contains(&tag)
}

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

#[cfg(feature = "quality")]
pub fn download(
    dir: &Path,
    cancel: &AtomicBool,
    on_progress: &dyn Fn(QualityProgress),
) -> Result<()> {
    crate::weights::download(dir, &WEIGHTS, cancel, on_progress)
}

#[cfg(not(feature = "quality"))]
pub fn download(
    _dir: &Path,
    _cancel: &AtomicBool,
    _on_progress: &dyn Fn(QualityProgress),
) -> Result<()> {
    Err(crate::error::Error::Tagging(
        "this build was made without the quality model".into(),
    ))
}

#[cfg(feature = "quality")]
mod real {
    use super::*;
    use crate::error::Error;
    use candle_core::{DType, Device, Module, Tensor};
    use candle_nn::VarBuilder;
    use candle_transformers::models::clip::vision_model::{
        ClipVisionConfig, ClipVisionTransformer,
    };

    fn stalled(e: impl std::fmt::Display) -> Error {
        Error::Tagging(e.to_string())
    }

    pub struct Rater {
        tower: ClipVisionTransformer,
        projection: candle_nn::Linear,
        head: candle_nn::Linear,
        device: Device,
    }

    impl Rater {
        pub fn load(dir: &Path) -> Result<Self> {
            let device = Device::Cpu;
            let config = ClipVisionConfig::vit_base_patch32();
            let file = path_of(dir, &WEIGHTS[0]);

            let builder = unsafe {
                VarBuilder::from_mmaped_safetensors(&[file], DType::F32, &device)
                    .map_err(stalled)?
            };

            let tower =
                ClipVisionTransformer::new(builder.pp("vision_model"), &config).map_err(stalled)?;
            let projection = candle_nn::linear_no_bias(
                config.embed_dim,
                config.projection_dim,
                builder.pp("visual_projection"),
            )
            .map_err(stalled)?;
            let head = candle_nn::linear(config.projection_dim, 1, builder.pp("predictor"))
                .map_err(stalled)?;

            Ok(Self {
                tower,
                projection,
                head,
                device,
            })
        }

        pub fn score(&self, path: &Path) -> Result<f32> {
            let pixels = planes(path, &self.device)?;
            self.score_pixels(&pixels)
        }

        fn score_pixels(&self, pixels: &Tensor) -> Result<f32> {
            let pooled = self.tower.forward(pixels).map_err(stalled)?;
            let embedded = self.projection.forward(&pooled).map_err(stalled)?;
            let length = embedded
                .sqr()
                .and_then(|t| t.sum_keepdim(1))
                .and_then(|t| t.sqrt())
                .map_err(stalled)?;
            let unit = embedded.broadcast_div(&length).map_err(stalled)?;

            self.head
                .forward(&unit)
                .and_then(|t| t.flatten_all())
                .and_then(|t| t.to_vec1::<f32>())
                .map_err(stalled)?
                .first()
                .copied()
                .ok_or_else(|| Error::Tagging("the quality model said nothing".into()))
        }
    }

    pub(super) fn centred(picture: &image::DynamicImage) -> image::DynamicImage {
        let side = SIDE as u32;
        let (width, height) = (picture.width().max(1), picture.height().max(1));
        let scale = side as f32 / width.min(height) as f32;
        let scaled = picture.resize_exact(
            ((width as f32 * scale).round() as u32).max(side),
            ((height as f32 * scale).round() as u32).max(side),
            image::imageops::FilterType::CatmullRom,
        );
        scaled.crop_imm(
            (scaled.width() - side) / 2,
            (scaled.height() - side) / 2,
            side,
            side,
        )
    }

    fn planes(path: &Path, device: &Device) -> Result<Tensor> {
        let opened = crate::imageio::open(path).ok_or_else(|| {
            Error::Tagging(format!("{}: cannot be read as a picture", path.display()))
        })?;
        let scaled = centred(&opened).to_rgb8();

        let mut planes = vec![0f32; 3 * SIDE * SIDE];
        for (x, y, pixel) in scaled.enumerate_pixels() {
            let at = y as usize * SIDE + x as usize;
            for channel in 0..3 {
                let value = pixel.0[channel] as f32 / 255.0;
                planes[channel * SIDE * SIDE + at] = (value - MEAN[channel]) / DEVIATION[channel];
            }
        }

        Tensor::from_vec(planes, (1, 3, SIDE, SIDE), device).map_err(stalled)
    }
}

#[cfg(feature = "quality")]
pub use real::Rater;

#[cfg(not(feature = "quality"))]
#[derive(Debug)]
pub struct Rater;

#[cfg(not(feature = "quality"))]
impl Rater {
    pub fn load(_dir: &Path) -> Result<Self> {
        Err(crate::error::Error::Tagging(
            "this build was made without the quality model".into(),
        ))
    }

    pub fn score(&self, _path: &Path) -> Result<f32> {
        Err(crate::error::Error::Tagging(
            "this build was made without the quality model".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "quality")]
    fn the_credit_names_the_licence_and_the_author() {
        assert!(CREDIT.contains("MIT"), "{CREDIT}");
        assert!(CREDIT.contains("LAION"), "{CREDIT}");
        assert!(CREDIT.contains("Kitada"), "{CREDIT}");
    }

    #[test]
    fn the_weight_is_pinned_to_a_revision() {
        for weight in &WEIGHTS {
            assert_eq!(weight.revision.len(), 40, "{}", weight.file);
            assert!(weight.bytes > 0, "{}", weight.file);
        }
    }

    #[test]
    fn the_download_is_about_a_third_of_a_gigabyte() {
        let total = total_bytes();
        assert!(total > 300 * 1024 * 1024, "{total}");
        assert!(total < 400 * 1024 * 1024, "{total}");
    }

    #[test]
    fn nothing_is_installed_in_an_empty_folder() {
        let dir = std::env::temp_dir().join("eonsort-quality-empty");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!installed(&dir));
        assert_eq!(present_bytes(&dir), 0);
    }

    #[test]
    fn the_ratings_that_used_to_be_tags_are_known_by_name() {
        assert!(was_a_rating("a good picture"));
        assert!(was_a_rating("a beautiful picture"));
        assert!(!was_a_rating("a dog"));
    }

    #[test]
    fn the_picture_is_normalised_the_way_the_model_was_trained() {
        assert_eq!(MEAN.len(), 3);
        assert_eq!(DEVIATION.len(), 3);
        for value in MEAN.iter().chain(DEVIATION.iter()) {
            assert!(*value > 0.0 && *value < 1.0, "{value}");
        }
    }

    #[cfg(feature = "quality")]
    #[test]
    #[ignore = "needs the downloaded model; run with EONSORT_MODELS set"]
    fn rates_real_pictures() {
        let Ok(dir) = std::env::var("EONSORT_MODELS") else {
            return;
        };
        let Ok(folder) = std::env::var("EONSORT_PICTURES") else {
            return;
        };

        let rater = Rater::load(Path::new(&dir)).expect("the model should load");

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
            .collect();
        found.sort();

        let mut scored: Vec<(f32, String)> = Vec::new();
        for picture in &found {
            let Ok(score) = rater.score(picture) else {
                continue;
            };
            let name = picture
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .chars()
                .take(46)
                .collect::<String>();
            scored.push((score, name));
        }

        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (score, name) in &scored {
            println!("  {score:6.3}  {name}");
        }

        let values: Vec<f32> = scored.iter().map(|(s, _)| *s).collect();
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        let spread = values.first().copied().unwrap_or(0.0) - values.last().copied().unwrap_or(0.0);
        println!(
            "
{} pictures | best {:.3} worst {:.3} mean {:.3} spread {:.3}",
            values.len(),
            values.first().copied().unwrap_or(0.0),
            values.last().copied().unwrap_or(0.0),
            mean,
            spread
        );
        println!("good >= {GOOD}, beautiful >= {BEAUTIFUL}");
        println!(
            "above good: {} | above beautiful: {}",
            values.iter().filter(|v| **v >= GOOD).count(),
            values.iter().filter(|v| **v >= BEAUTIFUL).count()
        );
    }

    #[cfg(feature = "quality")]
    #[test]
    fn a_wide_picture_is_cropped_to_the_middle_rather_than_squashed() {
        let wide = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(600, 200, |x, _| {
            image::Rgb([if x < 200 { 255 } else { 0 }, 0, 0])
        }));

        let centred = super::real::centred(&wide);
        assert_eq!(
            (centred.width(), centred.height()),
            (SIDE as u32, SIDE as u32)
        );

        let middle = centred.to_rgb8();
        assert_eq!(
            middle.get_pixel(SIDE as u32 / 2, SIDE as u32 / 2).0[0],
            0,
            "the middle of the picture should survive, not the left edge"
        );
    }

    #[cfg(feature = "quality")]
    #[test]
    fn a_tall_picture_keeps_its_shape_too() {
        let tall = image::DynamicImage::ImageRgb8(image::RgbImage::new(200, 900));
        let centred = super::real::centred(&tall);
        assert_eq!(
            (centred.width(), centred.height()),
            (SIDE as u32, SIDE as u32)
        );
    }

    #[cfg(feature = "quality")]
    #[test]
    fn a_picture_smaller_than_the_window_is_still_filled() {
        let small = image::DynamicImage::ImageRgb8(image::RgbImage::new(80, 60));
        let centred = super::real::centred(&small);
        assert_eq!(
            (centred.width(), centred.height()),
            (SIDE as u32, SIDE as u32)
        );
    }
}
