use crate::error::Result;
use crate::weights::Weight;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

pub use crate::weights::Progress as QualityProgress;

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
pub const GOOD_TAG: &str = "a good picture";
pub const BEAUTIFUL_TAG: &str = "a beautiful picture";

pub fn tags_for(score: f32) -> Vec<String> {
    if score >= BEAUTIFUL {
        vec![BEAUTIFUL_TAG.to_string(), GOOD_TAG.to_string()]
    } else if score >= GOOD {
        vec![GOOD_TAG.to_string()]
    } else {
        Vec::new()
    }
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

    fn planes(path: &Path, device: &Device) -> Result<Tensor> {
        let opened = crate::imageio::open(path).ok_or_else(|| {
            Error::Tagging(format!("{}: cannot be read as a picture", path.display()))
        })?;
        let scaled = opened
            .resize_exact(
                SIDE as u32,
                SIDE as u32,
                image::imageops::FilterType::CatmullRom,
            )
            .to_rgb8();

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
    fn a_dull_picture_earns_no_tag_and_a_lovely_one_earns_both() {
        assert!(tags_for(3.0).is_empty());
        assert_eq!(tags_for(GOOD), vec![GOOD_TAG.to_string()]);
        assert_eq!(
            tags_for(9.0),
            vec![BEAUTIFUL_TAG.to_string(), GOOD_TAG.to_string()]
        );
        assert!(tags_for(f32::NAN).is_empty());
    }

    #[test]
    fn the_picture_is_normalised_the_way_the_model_was_trained() {
        assert_eq!(MEAN.len(), 3);
        assert_eq!(DEVIATION.len(), 3);
        for value in MEAN.iter().chain(DEVIATION.iter()) {
            assert!(*value > 0.0 && *value < 1.0, "{value}");
        }
    }
}
