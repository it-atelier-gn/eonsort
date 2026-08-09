mod exif;
mod filename;
mod filesystem;
mod media;
pub mod vision;

use crate::ai::{Client, Reading};
use crate::suspect::{self, Flag};
use chrono::{Local, NaiveDateTime};
use serde::{Deserialize, Serialize};
use std::fs::Metadata;
use std::path::Path;

const HARD_FLAG_PENALTY: i64 = 1000;
const CONSENSUS_BONUS: i64 = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Filename,
    Exif,
    Media,
    Vision,
    Filesystem,
}

impl Provider {
    pub const ALL: [Provider; 5] = [
        Provider::Filename,
        Provider::Exif,
        Provider::Media,
        Provider::Vision,
        Provider::Filesystem,
    ];

    pub const DEFAULT: [Provider; 4] = [
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
            Provider::Vision => "vision",
            Provider::Filesystem => "filesystem",
        }
    }

    pub fn needs_model(self) -> bool {
        matches!(self, Provider::Vision)
    }

    pub fn trust_rank(self) -> i64 {
        match self {
            Provider::Exif | Provider::Media => 40,
            Provider::Filename => 30,
            Provider::Vision => 25,
            Provider::Filesystem => 10,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detection {
    pub provider: Provider,
    pub info: Option<String>,
    pub taken: NaiveDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Strategy {
    #[default]
    Smart,
    Oldest,
    Priority,
}

impl Strategy {
    pub fn label(self) -> &'static str {
        match self {
            Strategy::Smart => "smart",
            Strategy::Oldest => "oldest",
            Strategy::Priority => "priority",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectOptions {
    pub providers: Vec<Provider>,
    pub strategy: Strategy,
}

impl Default for DetectOptions {
    fn default() -> Self {
        Self {
            providers: Provider::DEFAULT.to_vec(),
            strategy: Strategy::default(),
        }
    }
}

pub struct DetectContext {
    pub filesystem_latest: Option<NaiveDateTime>,
    pub now: NaiveDateTime,
}

impl DetectContext {
    pub fn for_file(meta: &Metadata) -> Self {
        Self {
            filesystem_latest: filesystem::latest(meta),
            now: Local::now().naive_local(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub chosen: Detection,
    pub candidates: Vec<Detection>,
    pub flags: Vec<Flag>,
    pub reading: Option<Reading>,
}

pub fn detect_all(
    path: &Path,
    meta: &Metadata,
    providers: &[Provider],
    reading: Option<&Reading>,
) -> Vec<Detection> {
    let mut found: Vec<Detection> = providers
        .iter()
        .filter_map(|p| run(*p, path, meta, reading))
        .collect();
    found.sort_by_key(|d| d.provider);
    found.dedup_by_key(|d| d.provider);
    found
}

pub fn choose(
    candidates: &[Detection],
    opts: &DetectOptions,
    ctx: &DetectContext,
) -> Option<Detection> {
    match opts.strategy {
        Strategy::Priority => opts
            .providers
            .iter()
            .find_map(|p| candidates.iter().find(|c| c.provider == *p).cloned()),
        Strategy::Oldest => candidates
            .iter()
            .min_by(|a, b| a.taken.cmp(&b.taken).then(a.provider.cmp(&b.provider)))
            .cloned(),
        Strategy::Smart => smart(candidates, ctx),
    }
}

pub fn resolve(
    path: &Path,
    meta: &Metadata,
    opts: &DetectOptions,
    ai: Option<&Client>,
) -> Option<Resolved> {
    let ctx = DetectContext::for_file(meta);
    let reading = opts
        .providers
        .iter()
        .any(|p| p.needs_model())
        .then(|| vision::read(path, ai).ok())
        .flatten();

    let candidates = detect_all(path, meta, &opts.providers, reading.as_ref());
    let chosen = choose(&candidates, opts, &ctx)?;
    let flags = suspect::entry_flags(chosen.taken, &candidates, ctx.filesystem_latest, ctx.now);
    Some(Resolved {
        chosen,
        candidates,
        flags,
        reading,
    })
}

pub fn detect(path: &Path, meta: &Metadata, opts: &DetectOptions) -> Option<Detection> {
    resolve(path, meta, opts, None).map(|r| r.chosen)
}

fn smart(candidates: &[Detection], ctx: &DetectContext) -> Option<Detection> {
    candidates
        .iter()
        .map(|c| (score(c, candidates, ctx), c))
        .max_by(|(left_score, left), (right_score, right)| {
            left_score
                .cmp(right_score)
                .then(right.taken.cmp(&left.taken))
                .then(left.provider.trust_rank().cmp(&right.provider.trust_rank()))
        })
        .map(|(_, c)| c.clone())
}

fn score(candidate: &Detection, candidates: &[Detection], ctx: &DetectContext) -> i64 {
    let penalties =
        suspect::date_flags(candidate.taken, ctx.filesystem_latest, ctx.now).len() as i64;
    let corroborators = candidates
        .iter()
        .filter(|other| other.provider != candidate.provider)
        .filter(|other| {
            (other.taken - candidate.taken).num_hours().abs() < suspect::CONSENSUS_HOURS
        })
        .count() as i64;

    candidate.provider.trust_rank() - HARD_FLAG_PENALTY * penalties
        + CONSENSUS_BONUS * corroborators
}

fn run(
    provider: Provider,
    path: &Path,
    meta: &Metadata,
    reading: Option<&Reading>,
) -> Option<Detection> {
    match provider {
        Provider::Filename => filename::detect(path),
        Provider::Exif => exif::detect(path),
        Provider::Media => media::detect(path),
        Provider::Vision => reading
            .filter(|r| r.date_confident)
            .and_then(vision::into_detection),
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

    fn at(y: i32, m: u32, d: u32, hh: u32, mm: u32, ss: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(hh, mm, ss)
            .unwrap()
    }

    fn detection(provider: Provider, taken: NaiveDateTime) -> Detection {
        Detection {
            provider,
            info: None,
            taken,
        }
    }

    fn context(filesystem_latest: NaiveDateTime) -> DetectContext {
        DetectContext {
            filesystem_latest: Some(filesystem_latest),
            now: at(2026, 8, 6, 12, 0, 0),
        }
    }

    fn strategy(strategy: Strategy) -> DetectOptions {
        DetectOptions {
            providers: Provider::ALL.to_vec(),
            strategy,
        }
    }

    #[test]
    fn a_dated_filename_beats_the_filesystem_time() {
        let dir = tempdir().unwrap();
        let path = write(dir.path(), "IMG_20050102_030405.dat");
        let meta = fs::metadata(&path).unwrap();

        let found = detect(&path, &meta, &DetectOptions::default()).unwrap();
        assert_eq!(found.provider, Provider::Filename);
        assert_eq!(found.taken, at(2005, 1, 2, 3, 4, 5));
    }

    #[test]
    fn keeps_every_provider_that_reported_a_date() {
        let dir = tempdir().unwrap();
        let path = write(dir.path(), "IMG_20050102_030405.dat");
        let meta = fs::metadata(&path).unwrap();

        let candidates = detect_all(&path, &meta, &Provider::ALL, None);
        let providers: Vec<Provider> = candidates.iter().map(|c| c.provider).collect();
        assert_eq!(providers, vec![Provider::Filename, Provider::Filesystem]);
    }

    #[test]
    fn smart_rejects_a_camera_reset_date_in_favour_of_the_file_time() {
        let written = at(2019, 7, 4, 10, 0, 0);
        let candidates = vec![
            detection(Provider::Exif, at(2003, 1, 1, 0, 0, 0)),
            detection(Provider::Filesystem, written),
        ];

        let chosen = choose(&candidates, &strategy(Strategy::Smart), &context(written)).unwrap();
        assert_eq!(chosen.provider, Provider::Filesystem);
        assert_eq!(chosen.taken, written);
    }

    #[test]
    fn oldest_still_walks_into_the_camera_reset_date() {
        let written = at(2019, 7, 4, 10, 0, 0);
        let candidates = vec![
            detection(Provider::Exif, at(2003, 1, 1, 0, 0, 0)),
            detection(Provider::Filesystem, written),
        ];

        let chosen = choose(&candidates, &strategy(Strategy::Oldest), &context(written)).unwrap();
        assert_eq!(chosen.provider, Provider::Exif);
    }

    #[test]
    fn smart_keeps_a_believable_capture_date_older_than_the_file() {
        let written = at(2023, 1, 1, 0, 0, 0);
        let candidates = vec![
            detection(Provider::Exif, at(2019, 7, 4, 10, 0, 0)),
            detection(Provider::Filesystem, written),
        ];

        let chosen = choose(&candidates, &strategy(Strategy::Smart), &context(written)).unwrap();
        assert_eq!(chosen.provider, Provider::Exif);
    }

    #[test]
    fn smart_prefers_the_candidate_other_providers_corroborate() {
        let written = at(2019, 7, 4, 11, 0, 0);
        let candidates = vec![
            detection(Provider::Filename, at(2019, 7, 4, 10, 0, 0)),
            detection(Provider::Filesystem, written),
        ];

        let chosen = choose(&candidates, &strategy(Strategy::Smart), &context(written)).unwrap();
        assert_eq!(chosen.provider, Provider::Filename);
    }

    #[test]
    fn smart_still_returns_a_date_when_every_candidate_is_flagged() {
        let written = at(2019, 7, 4, 10, 0, 0);
        let candidates = vec![
            detection(Provider::Exif, at(2003, 1, 1, 0, 0, 0)),
            detection(Provider::Filename, at(2030, 1, 1, 0, 0, 0)),
        ];

        let chosen = choose(&candidates, &strategy(Strategy::Smart), &context(written)).unwrap();
        assert_eq!(chosen.taken, at(2003, 1, 1, 0, 0, 0));
    }

    #[test]
    fn resolve_reports_the_disagreement_between_providers() {
        let dir = tempdir().unwrap();
        let path = write(dir.path(), "IMG_20050102_030405.dat");
        let meta = fs::metadata(&path).unwrap();

        let resolved = resolve(&path, &meta, &DetectOptions::default(), None).unwrap();
        assert_eq!(resolved.candidates.len(), 2);
        assert!(resolved
            .flags
            .iter()
            .any(|f| matches!(f, crate::suspect::Flag::ProviderSpread { .. })));
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
