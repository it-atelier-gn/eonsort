use crate::error::{Error, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

pub struct Weight {
    pub repo: &'static str,
    pub revision: &'static str,
    pub file: &'static str,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub file: String,
    pub completed: u64,
    pub total: u64,
}

pub fn path_of(dir: &Path, weight: &Weight) -> PathBuf {
    dir.join(weight.file)
}

pub fn total_bytes(weights: &[Weight]) -> u64 {
    weights.iter().map(|weight| weight.bytes).sum()
}

pub fn present_bytes(dir: &Path, weights: &[Weight]) -> u64 {
    weights
        .iter()
        .filter_map(|weight| std::fs::metadata(path_of(dir, weight)).ok())
        .map(|meta| meta.len())
        .sum()
}

pub fn installed(dir: &Path, weights: &[Weight]) -> bool {
    weights.iter().all(|weight| path_of(dir, weight).exists())
}

pub fn remove(dir: &Path, weights: &[Weight]) -> Result<()> {
    for weight in weights {
        let path = path_of(dir, weight);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(Error::io(&path, e)),
        }
    }
    Ok(())
}

#[cfg(feature = "download")]
pub fn download(
    dir: &Path,
    weights: &[Weight],
    cancel: &AtomicBool,
    on_progress: &dyn Fn(Progress),
) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    for weight in weights {
        let target = path_of(dir, weight);
        if target.exists() {
            continue;
        }
        fetch(weight, &target, cancel, on_progress)?;
    }
    Ok(())
}

#[cfg(not(feature = "download"))]
pub fn download(
    _dir: &Path,
    _weights: &[Weight],
    _cancel: &AtomicBool,
    _on_progress: &dyn Fn(Progress),
) -> Result<()> {
    Err(Error::Download(
        "this build was made without model downloading".into(),
    ))
}

#[cfg(feature = "download")]
fn fetch(
    weight: &Weight,
    target: &Path,
    cancel: &AtomicBool,
    on_progress: &dyn Fn(Progress),
) -> Result<()> {
    use std::io::{Read, Write};
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }

    let url = format!(
        "https://huggingface.co/{}/resolve/{}/{}",
        weight.repo, weight.revision, weight.file
    );
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .build()
        .into();

    let response = agent
        .get(&url)
        .call()
        .map_err(|e| Error::Download(format!("could not reach {url}: {e}")))?;

    let total = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(weight.bytes);

    let part = target.with_extension("part");
    if let Some(parent) = part.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let mut file = std::fs::File::create(&part).map_err(|e| Error::io(&part, e))?;
    let mut reader = response.into_body().into_reader();
    let mut buffer = vec![0u8; 1 << 16];
    let mut completed = 0u64;

    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(file);
            let _ = std::fs::remove_file(&part);
            return Err(Error::Cancelled);
        }

        let read = reader.read(&mut buffer).map_err(|e| {
            Error::Download(format!(
                "the download of {} stopped early: {e}",
                weight.file
            ))
        })?;
        if read == 0 {
            break;
        }

        file.write_all(&buffer[..read])
            .map_err(|e| Error::io(&part, e))?;
        completed += read as u64;
        on_progress(Progress {
            file: weight.file.to_string(),
            completed,
            total,
        });
    }

    file.sync_all().map_err(|e| Error::io(&part, e))?;
    drop(file);
    std::fs::rename(&part, target).map_err(|e| Error::io(target, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: [Weight; 2] = [
        Weight {
            repo: "someone/a-model",
            revision: "0123456789012345678901234567890123456789",
            file: "first.safetensors",
            bytes: 10,
        },
        Weight {
            repo: "someone/a-model",
            revision: "0123456789012345678901234567890123456789",
            file: "second.safetensors",
            bytes: 32,
        },
    ];

    #[test]
    fn the_total_is_the_sum_of_every_weight() {
        assert_eq!(total_bytes(&SAMPLE), 42);
        assert_eq!(total_bytes(&[]), 0);
    }

    #[test]
    fn nothing_is_installed_in_an_empty_folder() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!installed(dir.path(), &SAMPLE));
        assert_eq!(present_bytes(dir.path(), &SAMPLE), 0);
    }

    #[test]
    fn a_half_finished_download_does_not_count_as_installed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(path_of(dir.path(), &SAMPLE[0]), [0u8; 10]).unwrap();
        assert!(!installed(dir.path(), &SAMPLE));
        assert_eq!(present_bytes(dir.path(), &SAMPLE), 10);

        std::fs::write(path_of(dir.path(), &SAMPLE[1]), [0u8; 32]).unwrap();
        assert!(installed(dir.path(), &SAMPLE));
        assert_eq!(present_bytes(dir.path(), &SAMPLE), 42);
    }

    #[test]
    fn removing_what_is_not_there_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        remove(dir.path(), &SAMPLE).unwrap();

        std::fs::write(path_of(dir.path(), &SAMPLE[0]), [0u8; 10]).unwrap();
        remove(dir.path(), &SAMPLE).unwrap();
        assert!(!path_of(dir.path(), &SAMPLE[0]).exists());
    }
}
