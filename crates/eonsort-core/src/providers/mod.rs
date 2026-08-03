mod exif;
mod filename;
mod filesystem;
mod media;

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::fs::Metadata;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Filename,
    Exif,
    Media,
    Filesystem,
}

impl Provider {
    pub const ALL: [Provider; 4] = [
        Provider::Filename,
        Provider::Exif,
        Provider::Media,
        Provider::Filesystem,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Provider::Filename => "filename",
            Provider::Exif => "exif",
            Provider::Media => "media",
            Provider::Filesystem => "filesystem",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detection {
    pub provider: Provider,
    pub info: Option<String>,
    pub taken: NaiveDateTime,
}

/// How to pick a single date when several providers report one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Strategy {
    /// Take the earliest date any provider reports.
    #[default]
    Oldest,
    /// Take the first date reported, following the configured provider order.
    Priority,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectOptions {
    pub providers: Vec<Provider>,
    pub strategy: Strategy,
}

impl Default for DetectOptions {
    fn default() -> Self {
        Self {
            providers: Provider::ALL.to_vec(),
            strategy: Strategy::default(),
        }
    }
}

pub fn detect(path: &Path, meta: &Metadata, opts: &DetectOptions) -> Option<Detection> {
    match opts.strategy {
        Strategy::Priority => opts.providers.iter().find_map(|p| run(*p, path, meta)),
        Strategy::Oldest => opts
            .providers
            .iter()
            .filter_map(|p| run(*p, path, meta))
            .min_by(|a, b| a.taken.cmp(&b.taken).then(a.provider.cmp(&b.provider))),
    }
}

fn run(provider: Provider, path: &Path, meta: &Metadata) -> Option<Detection> {
    match provider {
        Provider::Filename => filename::detect(path),
        Provider::Exif => exif::detect(path),
        Provider::Media => media::detect(path),
        Provider::Filesystem => filesystem::detect(meta),
    }
}

pub(crate) fn extension_lowercase(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::fs;
    use tempfile::tempdir;

    fn write(dir: &Path, name: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        fs::write(&path, b"payload").unwrap();
        path
    }

    #[test]
    fn oldest_strategy_beats_filesystem_time() {
        let dir = tempdir().unwrap();
        let path = write(dir.path(), "IMG_20050102_030405.dat");
        let meta = fs::metadata(&path).unwrap();

        let found = detect(&path, &meta, &DetectOptions::default()).unwrap();
        assert_eq!(found.provider, Provider::Filename);
        assert_eq!(
            found.taken,
            NaiveDate::from_ymd_opt(2005, 1, 2)
                .unwrap()
                .and_hms_opt(3, 4, 5)
                .unwrap()
        );
    }

    #[test]
    fn falls_back_to_filesystem_when_name_has_no_date() {
        let dir = tempdir().unwrap();
        let path = write(dir.path(), "holiday.dat");
        let meta = fs::metadata(&path).unwrap();

        let found = detect(&path, &meta, &DetectOptions::default()).unwrap();
        assert_eq!(found.provider, Provider::Filesystem);
    }

    #[test]
    fn priority_strategy_follows_provider_order() {
        let dir = tempdir().unwrap();
        let path = write(dir.path(), "IMG_20050102_030405.dat");
        let meta = fs::metadata(&path).unwrap();

        let opts = DetectOptions {
            providers: vec![Provider::Filesystem, Provider::Filename],
            strategy: Strategy::Priority,
        };
        let found = detect(&path, &meta, &opts).unwrap();
        assert_eq!(found.provider, Provider::Filesystem);
    }

    #[test]
    fn disabled_providers_are_skipped() {
        let dir = tempdir().unwrap();
        let path = write(dir.path(), "holiday.dat");
        let meta = fs::metadata(&path).unwrap();

        let opts = DetectOptions {
            providers: vec![Provider::Filename],
            strategy: Strategy::Oldest,
        };
        assert!(detect(&path, &meta, &opts).is_none());
    }
}
