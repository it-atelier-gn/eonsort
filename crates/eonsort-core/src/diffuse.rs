use crate::error::{Error, Result};
use crate::weights::{self, Weight};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

pub use crate::weights::Progress as DiffuseProgress;

pub const SIDE: usize = 512;
pub const LATENT: usize = SIDE / 8;
pub const MIN_STEPS: usize = 4;
pub const MAX_STEPS: usize = 50;
pub const STEPS: usize = 20;
pub const GUIDANCE: f64 = 7.5;
pub const LATENT_SCALE: f64 = 0.18215;

const REPO: &str = "stable-diffusion-v1-5/stable-diffusion-inpainting";
const REVISION: &str = "8a4288a76071f7280aedbdb3253bdb9e9d5d84bb";

pub const WEIGHTS: [Weight; 4] = [
    Weight {
        repo: REPO,
        revision: REVISION,
        file: "text_encoder/model.fp16.safetensors",
        bytes: 246_144_864,
    },
    Weight {
        repo: REPO,
        revision: REVISION,
        file: "vae/diffusion_pytorch_model.fp16.safetensors",
        bytes: 167_335_342,
    },
    Weight {
        repo: REPO,
        revision: REVISION,
        file: "unet/diffusion_pytorch_model.fp16.safetensors",
        bytes: 1_719_154_104,
    },
    Weight {
        repo: "openai/clip-vit-base-patch32",
        revision: "3d74acf9a28c67741b2f4f2ea7635f0aaf6f0268",
        file: "tokenizer.json",
        bytes: 2_224_041,
    },
];

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

pub fn steps_within(steps: usize) -> usize {
    steps.clamp(MIN_STEPS, MAX_STEPS)
}

#[cfg(not(feature = "diffuse"))]
pub fn download(
    _dir: &Path,
    _cancel: &AtomicBool,
    _on_progress: &dyn Fn(DiffuseProgress),
) -> Result<()> {
    Err(Error::Diffuse(
        "this build was made without the painting model".into(),
    ))
}

#[cfg(feature = "diffuse")]
pub fn download(
    dir: &Path,
    cancel: &AtomicBool,
    on_progress: &dyn Fn(DiffuseProgress),
) -> Result<()> {
    weights::download(dir, &WEIGHTS, cancel, on_progress)
}

#[cfg(not(feature = "diffuse"))]
pub fn fill(
    _dir: &Path,
    _image: &[u8],
    _mask: &[u8],
    _prompt: &str,
    _steps: usize,
) -> Result<Vec<u8>> {
    Err(Error::Diffuse(
        "this build was made without the painting model".into(),
    ))
}

#[cfg(feature = "diffuse")]
pub fn fill(dir: &Path, image: &[u8], mask: &[u8], prompt: &str, steps: usize) -> Result<Vec<u8>> {
    use candle_core::{DType, Device, IndexOp, Module, Tensor};
    use candle_transformers::models::stable_diffusion::{
        build_clip_transformer, StableDiffusionConfig,
    };
    use tokenizers::Tokenizer;

    if !installed(dir) {
        return Err(Error::Diffuse(
            "the painting model is not downloaded yet".into(),
        ));
    }

    let device = Device::Cpu;
    let dtype = DType::F32;
    let steps = steps_within(steps);

    let picture = planes(image)?;
    let stencil = paintable(mask)?;
    if stencil.iter().all(|&wanted| wanted == 0.0) {
        return Err(Error::Diffuse("there is nothing to paint".into()));
    }

    let config = StableDiffusionConfig::v1_5(None, Some(SIDE), Some(SIDE));

    let tokenizer = Tokenizer::from_file(path_of(dir, &WEIGHTS[3]))
        .map_err(|e| Error::Diffuse(format!("the words could not be read: {e}")))?;
    let clip = build_clip_transformer(&config.clip, path_of(dir, &WEIGHTS[0]), &device, dtype)
        .map_err(stalled)?;

    let said = spoken(&tokenizer, prompt, &device)?;
    let unsaid = spoken(&tokenizer, "", &device)?;
    let embedding = Tensor::cat(
        &[
            clip.forward(&unsaid).map_err(stalled)?,
            clip.forward(&said).map_err(stalled)?,
        ],
        0,
    )
    .map_err(stalled)?;

    let vae = config
        .build_vae(path_of(dir, &WEIGHTS[1]), &device, dtype)
        .map_err(stalled)?;
    let unet = config
        .build_unet(path_of(dir, &WEIGHTS[2]), &device, 9, false, dtype)
        .map_err(stalled)?;
    let mut scheduler = config.build_scheduler(steps).map_err(stalled)?;

    let seen = Tensor::from_vec(picture, (1, 3, SIDE, SIDE), &device)
        .map_err(stalled)?
        .to_dtype(dtype)
        .map_err(stalled)?;
    let hidden = Tensor::from_vec(stencil.clone(), (1, 1, SIDE, SIDE), &device)
        .map_err(stalled)?
        .to_dtype(dtype)
        .map_err(stalled)?;

    let kept = ((hidden.neg().map_err(stalled)?) + 1.0).map_err(stalled)?;
    let covered = seen.broadcast_mul(&kept).map_err(stalled)?;

    let latents_of_covered = (vae
        .encode(&covered)
        .map_err(stalled)?
        .sample()
        .map_err(stalled)?
        * LATENT_SCALE)
        .map_err(stalled)?;

    let small = Tensor::from_vec(shrink(&stencil), (1, 1, LATENT, LATENT), &device)
        .map_err(stalled)?
        .to_dtype(dtype)
        .map_err(stalled)?;

    let both_masks = Tensor::cat(&[&small, &small], 0).map_err(stalled)?;
    let both_covered =
        Tensor::cat(&[&latents_of_covered, &latents_of_covered], 0).map_err(stalled)?;

    let mut latents = (Tensor::randn(0f32, 1f32, (1, 4, LATENT, LATENT), &device)
        .map_err(stalled)?
        .to_dtype(dtype)
        .map_err(stalled)?
        * scheduler.init_noise_sigma())
    .map_err(stalled)?;

    let timesteps = scheduler.timesteps().to_vec();
    for &at in timesteps.iter() {
        let doubled = Tensor::cat(&[&latents, &latents], 0).map_err(stalled)?;
        let scaled = scheduler.scale_model_input(doubled, at).map_err(stalled)?;
        let input = Tensor::cat(&[&scaled, &both_masks, &both_covered], 1).map_err(stalled)?;

        let guessed = unet
            .forward(&input, at as f64, &embedding)
            .map_err(stalled)?;
        let without = guessed.i(0..1).map_err(stalled)?;
        let with = guessed.i(1..2).map_err(stalled)?;
        let noise = (&without
            + ((with - &without).map_err(stalled)? * GUIDANCE).map_err(stalled)?)
        .map_err(stalled)?;

        latents = scheduler.step(&noise, at, &latents).map_err(stalled)?;
    }

    let painted = vae
        .decode(&(&latents / LATENT_SCALE).map_err(stalled)?)
        .map_err(stalled)?;
    let painted = ((painted / 2.0).map_err(stalled)? + 0.5)
        .map_err(stalled)?
        .clamp(0f32, 1f32)
        .map_err(stalled)?;

    encode(&painted)
}

#[cfg(feature = "diffuse")]
fn stalled(error: candle_core::Error) -> Error {
    Error::Diffuse(format!("the painting model stopped: {error}"))
}

#[cfg(feature = "diffuse")]
fn spoken(
    tokenizer: &tokenizers::Tokenizer,
    prompt: &str,
    device: &candle_core::Device,
) -> Result<candle_core::Tensor> {
    const LENGTH: usize = 77;
    const PADDING: u32 = 49407;

    let encoded = tokenizer
        .encode(prompt, true)
        .map_err(|e| Error::Diffuse(format!("the words could not be counted: {e}")))?;
    let mut ids = encoded.get_ids().to_vec();
    ids.truncate(LENGTH);
    while ids.len() < LENGTH {
        ids.push(PADDING);
    }

    candle_core::Tensor::new(ids.as_slice(), device)
        .and_then(|tensor| tensor.unsqueeze(0))
        .map_err(stalled)
}

#[cfg(feature = "diffuse")]
fn planes(png: &[u8]) -> Result<Vec<f32>> {
    let picture = image::load_from_memory(png)
        .map_err(|e| Error::Diffuse(format!("the photograph could not be read: {e}")))?
        .resize_exact(
            SIDE as u32,
            SIDE as u32,
            image::imageops::FilterType::CatmullRom,
        )
        .to_rgb8();

    let mut values = vec![0f32; 3 * SIDE * SIDE];
    for (index, pixel) in picture.pixels().enumerate() {
        for channel in 0..3 {
            values[channel * SIDE * SIDE + index] = f32::from(pixel.0[channel]) / 127.5 - 1.0;
        }
    }
    Ok(values)
}

#[cfg(feature = "diffuse")]
fn paintable(png: &[u8]) -> Result<Vec<f32>> {
    let stencil = image::load_from_memory(png)
        .map_err(|e| Error::Diffuse(format!("the mask could not be read: {e}")))?
        .resize_exact(
            SIDE as u32,
            SIDE as u32,
            image::imageops::FilterType::Nearest,
        )
        .to_rgba8();

    Ok(stencil
        .pixels()
        .map(|pixel| if pixel.0[3] < 128 { 1.0 } else { 0.0 })
        .collect())
}

#[cfg(feature = "diffuse")]
fn encode(painted: &candle_core::Tensor) -> Result<Vec<u8>> {
    let flat = painted
        .squeeze(0)
        .and_then(|t| t.permute((1, 2, 0)))
        .and_then(|t| t.flatten_all())
        .and_then(|t| t.to_vec1::<f32>())
        .map_err(stalled)?;

    let bytes: Vec<u8> = flat
        .iter()
        .map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect();

    let picture = image::RgbImage::from_raw(SIDE as u32, SIDE as u32, bytes)
        .ok_or_else(|| Error::Diffuse("the painted picture came back the wrong size".into()))?;

    let mut out = std::io::Cursor::new(Vec::new());
    picture
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| Error::Diffuse(format!("the painted picture could not be saved: {e}")))?;
    Ok(out.into_inner())
}

#[cfg(feature = "diffuse")]
fn shrink(stencil: &[f32]) -> Vec<f32> {
    let mut small = vec![0f32; LATENT * LATENT];
    let step = SIDE / LATENT;
    for row in 0..LATENT {
        for column in 0..LATENT {
            let mut wanted = 0f32;
            for inner in 0..step {
                for across in 0..step {
                    let at = (row * step + inner) * SIDE + column * step + across;
                    if stencil[at] > wanted {
                        wanted = stencil[at];
                    }
                }
            }
            small[row * LATENT + column] = wanted;
        }
    }
    small
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_weights_are_pinned_to_one_revision_each() {
        for weight in WEIGHTS.iter() {
            assert_eq!(weight.revision.len(), 40, "{}", weight.file);
            assert!(weight.bytes > 0, "{}", weight.file);
        }
    }

    #[test]
    fn the_pieces_of_the_painter_are_all_there() {
        let files: Vec<&str> = WEIGHTS.iter().map(|weight| weight.file).collect();
        assert!(files.iter().any(|file| file.contains("text_encoder")));
        assert!(files.iter().any(|file| file.contains("vae")));
        assert!(files.iter().any(|file| file.contains("unet")));
        assert!(files.contains(&"tokenizer.json"));
    }

    #[test]
    fn every_weight_lands_on_a_path_of_its_own() {
        let dir = Path::new("models");
        let mut paths: Vec<PathBuf> = WEIGHTS.iter().map(|w| path_of(dir, w)).collect();
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), WEIGHTS.len());
    }

    #[test]
    fn the_download_is_about_two_gigabytes() {
        let total = total_bytes();
        assert!(total > 2_000_000_000, "{total}");
        assert!(total < 2_400_000_000, "{total}");
    }

    #[test]
    fn a_step_count_is_kept_inside_what_is_worth_waiting_for() {
        assert_eq!(steps_within(0), MIN_STEPS);
        assert_eq!(steps_within(999), MAX_STEPS);
        assert_eq!(steps_within(STEPS), STEPS);
    }

    #[test]
    fn nothing_is_present_before_anything_is_downloaded() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!installed(dir.path()));
        assert_eq!(present_bytes(dir.path()), 0);
    }

    #[cfg(not(feature = "diffuse"))]
    #[test]
    fn a_build_without_the_painter_says_so_plainly() {
        let dir = tempfile::tempdir().unwrap();
        let message = fill(dir.path(), &[], &[], "", STEPS)
            .unwrap_err()
            .to_string();
        assert!(message.contains("without the painting model"), "{message}");
    }
}
