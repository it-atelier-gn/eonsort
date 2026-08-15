use crate::error::{Error, Result};
use crate::weights::{self, Weight};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

pub use crate::weights::Progress as DepthProgress;

pub const IMAGE_SIZE: usize = 518;
pub const MIN_EDGE: u32 = 64;
pub const MAX_EDGE: u32 = 384;

#[cfg(feature = "depth")]
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
#[cfg(feature = "depth")]
const STD: [f32; 3] = [0.229, 0.224, 0.225];

pub const WEIGHTS: [Weight; 2] = [
    Weight {
        repo: "lmz/candle-dino-v2",
        revision: "550d8ab8dc45ae1e23815176ea881ee3dfd47627",
        file: "dinov2_vits14.safetensors",
        bytes: 91_318_288,
    },
    Weight {
        repo: "jeroenvlek/depth-anything-v2-safetensors",
        revision: "a8cd5eb93485537f612b31b78864b41f659b245d",
        file: "depth_anything_v2_vits.safetensors",
        bytes: 99_165_428,
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepthGrid {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
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

#[cfg(not(feature = "depth"))]
pub fn download(
    _dir: &Path,
    _cancel: &AtomicBool,
    _on_progress: &dyn Fn(DepthProgress),
) -> Result<()> {
    Err(Error::Depth(
        "this build was made without the depth model".into(),
    ))
}

#[cfg(feature = "depth")]
pub fn download(
    dir: &Path,
    cancel: &AtomicBool,
    on_progress: &dyn Fn(DepthProgress),
) -> Result<()> {
    weights::download(dir, &WEIGHTS, cancel, on_progress)
}

pub fn grid_size(width: u32, height: u32, edge: u32) -> (u32, u32) {
    let edge = edge.clamp(MIN_EDGE, MAX_EDGE);
    if width == 0 || height == 0 {
        return (edge, edge);
    }
    if width >= height {
        let scaled = (u64::from(edge) * u64::from(height)) / u64::from(width);
        (edge, (scaled as u32).max(1))
    } else {
        let scaled = (u64::from(edge) * u64::from(width)) / u64::from(height);
        ((scaled as u32).max(1), edge)
    }
}

pub fn normalise(values: &[f32]) -> Vec<u8> {
    let mut low = f32::INFINITY;
    let mut high = f32::NEG_INFINITY;
    for value in values {
        if !value.is_finite() {
            continue;
        }
        if *value < low {
            low = *value;
        }
        if *value > high {
            high = *value;
        }
    }

    if !low.is_finite() || !high.is_finite() || high <= low {
        return vec![0; values.len()];
    }

    let range = high - low;
    values
        .iter()
        .map(|value| {
            if !value.is_finite() {
                return 0;
            }
            (((value - low) / range) * 255.0).round().clamp(0.0, 255.0) as u8
        })
        .collect()
}

#[cfg(not(feature = "depth"))]
pub fn estimate(_dir: &Path, _source: &Path, _edge: u32) -> Result<DepthGrid> {
    Err(Error::Depth(
        "this build was made without the depth model".into(),
    ))
}

#[cfg(feature = "depth")]
pub fn estimate(dir: &Path, source: &Path, edge: u32) -> Result<DepthGrid> {
    use candle_core::{DType, Device, Module, Tensor};
    use candle_nn::VarBuilder;
    use candle_transformers::models::depth_anything_v2::{DepthAnythingV2, DepthAnythingV2Config};
    use candle_transformers::models::dinov2;
    use std::sync::Arc;

    fn oops(error: impl std::fmt::Display) -> Error {
        Error::Depth(error.to_string())
    }

    let backbone_file = path_of(dir, &WEIGHTS[0]);
    let head_file = path_of(dir, &WEIGHTS[1]);
    if !backbone_file.exists() || !head_file.exists() {
        return Err(Error::Depth(
            "the depth model has not been downloaded yet".into(),
        ));
    }

    let picture =
        image::open(source).map_err(|e| Error::Depth(format!("{}: {e}", source.display())))?;
    let (width, height) = (picture.width(), picture.height());

    let square = picture
        .resize_exact(
            IMAGE_SIZE as u32,
            IMAGE_SIZE as u32,
            image::imageops::FilterType::Triangle,
        )
        .to_rgb8();

    let mut planes = Vec::with_capacity(3 * IMAGE_SIZE * IMAGE_SIZE);
    for channel in 0..3 {
        for pixel in square.pixels() {
            planes.push((f32::from(pixel[channel]) / 255.0 - MEAN[channel]) / STD[channel]);
        }
    }

    let device = Device::Cpu;
    let input = Tensor::from_vec(planes, (1, 3, IMAGE_SIZE, IMAGE_SIZE), &device).map_err(oops)?;

    let backbone = unsafe {
        VarBuilder::from_mmaped_safetensors(&[backbone_file], DType::F32, &device).map_err(oops)?
    };
    let backbone = dinov2::vit_small(backbone).map_err(oops)?;

    let head = unsafe {
        VarBuilder::from_mmaped_safetensors(&[head_file], DType::F32, &device).map_err(oops)?
    };
    let model = DepthAnythingV2::new(Arc::new(backbone), DepthAnythingV2Config::vit_small(), head)
        .map_err(oops)?;

    let depth = model.forward(&input).map_err(oops)?;
    let (grid_width, grid_height) = grid_size(width, height, edge);
    let resampled = depth
        .interpolate2d(grid_height as usize, grid_width as usize)
        .map_err(oops)?;
    let values: Vec<f32> = resampled
        .flatten_all()
        .map_err(oops)?
        .to_vec1()
        .map_err(oops)?;

    Ok(DepthGrid {
        width: grid_width,
        height: grid_height,
        data: normalise(&values),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_weights_are_pinned_to_a_revision() {
        for weight in &WEIGHTS {
            assert_eq!(weight.revision.len(), 40, "{}", weight.file);
            assert!(weight.file.ends_with(".safetensors"));
            assert!(weight.bytes > 1_000_000);
        }
        assert_eq!(total_bytes(), 190_483_716);
    }

    #[test]
    fn normalising_puts_the_nearest_at_the_top_of_the_range() {
        let out = normalise(&[1.0, 3.0, 2.0, 5.0]);
        assert_eq!(out[0], 0);
        assert_eq!(out[3], 255);
        assert!(out[1] < out[3] && out[1] > out[0]);
        assert!(out[2] < out[1]);
    }

    #[test]
    fn normalising_is_monotone() {
        let values: Vec<f32> = (0..64).map(|i| i as f32 * 0.5 - 3.0).collect();
        let out = normalise(&values);
        for pair in out.windows(2) {
            assert!(pair[1] >= pair[0]);
        }
    }

    #[test]
    fn a_flat_or_broken_field_normalises_to_nothing() {
        assert_eq!(normalise(&[2.0, 2.0, 2.0]), vec![0, 0, 0]);
        assert_eq!(normalise(&[]), Vec::<u8>::new());
        assert_eq!(normalise(&[f32::NAN, f32::NAN]), vec![0, 0]);
    }

    #[test]
    fn a_broken_reading_does_not_poison_the_rest() {
        let out = normalise(&[0.0, f32::NAN, 10.0]);
        assert_eq!(out[0], 0);
        assert_eq!(out[1], 0);
        assert_eq!(out[2], 255);
    }

    #[test]
    fn the_grid_keeps_the_shape_of_the_picture() {
        assert_eq!(grid_size(3000, 2000, 300), (300, 200));
        assert_eq!(grid_size(2000, 3000, 300), (200, 300));
        assert_eq!(grid_size(1000, 1000, 256), (256, 256));
    }

    #[test]
    fn the_grid_stays_inside_sane_bounds() {
        assert_eq!(grid_size(1000, 1000, 0), (MIN_EDGE, MIN_EDGE));
        assert_eq!(grid_size(1000, 1000, 99_999), (MAX_EDGE, MAX_EDGE));
        assert_eq!(grid_size(0, 0, 128), (128, 128));

        let (width, height) = grid_size(10_000, 3, 384);
        assert_eq!(width, 384);
        assert_eq!(height, 1);
    }

    #[cfg(not(feature = "depth"))]
    #[test]
    fn a_build_without_the_feature_says_so_rather_than_pretending() {
        let error = estimate(Path::new("."), Path::new("a.jpg"), 256).unwrap_err();
        assert!(
            error.to_string().contains("without the depth model"),
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
