use crate::companion;
use crate::error::{Error, Result};
use crate::model::{
    destination_root, destination_with_facts, validate_folder_pattern, validate_name_pattern,
    PlanEntry, PlanHeader, PlanRecord, SkippedEntry, PLAN_VERSION,
};
use crate::plan::{read_plan, Plan, PlanWriter};
use crate::providers::{resolve, DetectOptions};
use crate::rotate::Transform;
use crate::upright::Detector;
use chrono::Local;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use walkdir::WalkDir;

const BATCH: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanOptions {
    pub sources: Vec<PathBuf>,
    #[serde(default)]
    pub destination: Option<PathBuf>,
    pub folder_pattern: String,
    #[serde(default = "default_name_pattern")]
    pub name_pattern: String,
    #[serde(default)]
    pub gazetteer: Option<PathBuf>,
    pub detect: DetectOptions,
    pub follow_symlinks: bool,
    #[serde(default)]
    pub auto_rotate: bool,
    #[serde(default = "yes")]
    pub pair_companions: bool,
    #[serde(default)]
    pub upright_model_dir: Option<PathBuf>,
}

fn yes() -> bool {
    true
}

fn default_name_pattern() -> String {
    crate::naming::DEFAULT_NAME_PATTERN.to_string()
}

struct Models<'a> {
    upright: Option<&'a Detector>,
    gazetteer: Option<&'a crate::geocode::Gazetteer>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanPhase {
    Counting,
    Analysing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScanProgress {
    pub phase: ScanPhase,
    pub files_seen: u64,
    pub files_total: u64,
    pub bytes_total: u64,
    pub current: Option<PathBuf>,
}

pub fn scan(
    plan_path: &Path,
    options: &ScanOptions,
    cancel: &AtomicBool,
    on_progress: &dyn Fn(ScanProgress),
) -> Result<Plan> {
    validate_folder_pattern(&options.folder_pattern)?;
    validate_name_pattern(&options.name_pattern)?;

    let header = PlanHeader {
        version: PLAN_VERSION,
        created_at: Local::now().naive_local(),
        sources: options.sources.clone(),
        destination: options.destination.clone(),
        folder_pattern: options.folder_pattern.clone(),
        name_pattern: options.name_pattern.clone(),
        detect: options.detect.clone(),
    };

    let resumed = resumable(plan_path, &header);
    let done: HashSet<PathBuf> = resumed
        .as_ref()
        .map(|p| p.analysed_sources())
        .unwrap_or_default();
    let mut writer = match &resumed {
        Some(_) => PlanWriter::append(plan_path)?,
        None => PlanWriter::create(plan_path, &header)?,
    };

    let upright = match &options.upright_model_dir {
        Some(dir) => Some(Detector::load(dir)?),
        None => None,
    };
    let gazetteer = match &options.gazetteer {
        Some(dir) => Some(crate::geocode::Gazetteer::load(dir)?),
        None => None,
    };
    let models = Models {
        upright: upright.as_ref(),
        gazetteer: gazetteer.as_ref(),
    };

    let (files_total, bytes_total) = count(options, cancel, on_progress)?;
    let mut files_seen = done.len() as u64;
    let mut batch: Vec<(PathBuf, Metadata)> = Vec::with_capacity(BATCH);

    for entry in walk(options) {
        check_cancelled(cancel)?;
        let Ok(meta) = entry.metadata() else { continue };
        let path = entry.into_path();
        if done.contains(&path) {
            continue;
        }
        batch.push((path, meta));

        if batch.len() >= BATCH {
            files_seen += flush_batch(&mut batch, options, &models, &mut writer)? as u64;
            on_progress(ScanProgress {
                phase: ScanPhase::Analysing,
                files_seen,
                files_total,
                bytes_total,
                current: None,
            });
        }
    }

    files_seen += flush_batch(&mut batch, options, &models, &mut writer)? as u64;
    writer.flush()?;
    drop(writer);

    on_progress(ScanProgress {
        phase: ScanPhase::Analysing,
        files_seen,
        files_total,
        bytes_total,
        current: None,
    });

    let mut plan = read_plan(plan_path)?;
    if options.pair_companions {
        let root = plan.header.root().to_path_buf();
        let pattern = plan.header.folder_pattern.clone();
        if companion::pair(&mut plan.entries, &root, &pattern)? > 0 {
            crate::plan::rewrite(plan_path, &plan)?;
        }
    }

    Ok(plan)
}

fn resumable(plan_path: &Path, header: &PlanHeader) -> Option<Plan> {
    let plan = read_plan(plan_path).ok()?;
    let compatible = plan.header.sources == header.sources
        && plan.header.destination == header.destination
        && plan.header.folder_pattern == header.folder_pattern
        && plan.header.detect == header.detect;
    compatible.then_some(plan)
}

fn flush_batch(
    batch: &mut Vec<(PathBuf, Metadata)>,
    options: &ScanOptions,
    models: &Models<'_>,
    writer: &mut PlanWriter,
) -> Result<usize> {
    if batch.is_empty() {
        return Ok(0);
    }
    let records: Vec<PlanRecord> = batch
        .par_iter()
        .map(|(path, meta)| analyse(path, meta, options, models))
        .collect();
    let count = records.len();
    for record in &records {
        writer.write(record)?;
    }
    batch.clear();
    Ok(count)
}

fn analyse(path: &Path, meta: &Metadata, options: &ScanOptions, models: &Models<'_>) -> PlanRecord {
    let Some(found) = resolve(path, meta, &options.detect) else {
        return PlanRecord::Skipped(SkippedEntry {
            source: path.to_path_buf(),
            reason: "no creation date found".into(),
        });
    };

    let subject: Option<String> = None;

    let turnable = crate::rotate::lossless_extension(path);
    let orientation = if turnable {
        crate::rotate::read_orientation(path)
    } else {
        0
    };
    let from_exif = Transform::for_orientation(orientation);

    let guess = match models.upright {
        Some(detector) if orientation <= 1 => detector.guess(path).ok(),
        _ => None,
    };
    let guessed = guess
        .as_ref()
        .filter(|guess| !guess.transform.is_identity());

    let rotate_reason = guessed.map(|guess| guess.reason.clone());
    let rotate = if !options.auto_rotate {
        Transform::None
    } else if !from_exif.is_identity() {
        from_exif
    } else if turnable {
        guessed.map(|guess| guess.transform).unwrap_or_default()
    } else {
        Transform::None
    };

    let place = models
        .gazetteer
        .and_then(|gazetteer| crate::geocode::read(path).map(|at| gazetteer.place(at)))
        .unwrap_or_default();
    let facts = crate::naming::Facts::for_source(path)
        .with_subject(subject.as_deref())
        .with_place(place.clone());

    match destination_with_facts(
        found.chosen.taken,
        &facts,
        destination_root(options.destination.as_deref()),
        &options.folder_pattern,
        &options.name_pattern,
    ) {
        Ok(destination) => PlanRecord::Entry(PlanEntry {
            source: path.to_path_buf(),
            destination,
            taken: found.chosen.taken,
            place: place.clone(),
            provider: found.chosen.provider,
            provider_info: found.chosen.info,
            size: meta.len(),
            candidates: found.candidates,
            flags: found.flags,
            tags: Vec::new(),
            caption: None,
            subject,
            orientation,
            rotate,
            rotate_reason,
            reencode: false,
        }),
        Err(err) => PlanRecord::Skipped(SkippedEntry {
            source: path.to_path_buf(),
            reason: err.to_string(),
        }),
    }
}

fn count(
    options: &ScanOptions,
    cancel: &AtomicBool,
    on_progress: &dyn Fn(ScanProgress),
) -> Result<(u64, u64)> {
    let mut files = 0u64;
    let mut bytes = 0u64;
    for entry in walk(options) {
        check_cancelled(cancel)?;
        files += 1;
        bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
        if files.is_multiple_of(1024) {
            on_progress(ScanProgress {
                phase: ScanPhase::Counting,
                files_seen: files,
                files_total: files,
                bytes_total: bytes,
                current: Some(entry.into_path()),
            });
        }
    }
    on_progress(ScanProgress {
        phase: ScanPhase::Counting,
        files_seen: files,
        files_total: files,
        bytes_total: bytes,
        current: None,
    });
    Ok((files, bytes))
}

fn walk(options: &ScanOptions) -> impl Iterator<Item = walkdir::DirEntry> + '_ {
    let destination = options
        .destination
        .as_ref()
        .map(|d| std::fs::canonicalize(d).unwrap_or_else(|_| d.clone()));

    options.sources.iter().flat_map(move |source| {
        let destination = destination.clone();
        WalkDir::new(source)
            .follow_links(options.follow_symlinks)
            .into_iter()
            .filter_entry(move |e| {
                !e.file_type().is_dir()
                    || destination.as_ref().is_none_or(|destination| {
                        std::fs::canonicalize(e.path())
                            .map(|p| &p != destination)
                            .unwrap_or(true)
                    })
            })
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| !is_temp_artifact(e.path()))
    })
}

fn is_temp_artifact(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case(crate::copy::TEMP_EXTENSION))
}

fn check_cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DEFAULT_FOLDER_PATTERN;
    use crate::providers::Provider;
    use std::fs;
    use tempfile::tempdir;

    fn options(sources: Vec<PathBuf>, destination: PathBuf) -> ScanOptions {
        ScanOptions {
            sources,
            destination: Some(destination),
            folder_pattern: DEFAULT_FOLDER_PATTERN.to_string(),
            gazetteer: None,
            name_pattern: crate::naming::DEFAULT_NAME_PATTERN.to_string(),
            detect: DetectOptions::default(),
            follow_symlinks: false,
            auto_rotate: false,
            pair_companions: false,
            upright_model_dir: None,
        }
    }

    fn noop(_: ScanProgress) {}

    #[test]
    fn a_companion_takes_the_date_of_the_picture_it_belongs_to() {
        use filetime::{set_file_mtime, FileTime};

        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("IMG_1234.jpg"), b"picture").unwrap();
        fs::write(src.join("IMG_1234.mov"), b"clip").unwrap();
        set_file_mtime(
            src.join("IMG_1234.jpg"),
            FileTime::from_unix_time(1_557_818_523, 0),
        )
        .unwrap();
        set_file_mtime(
            src.join("IMG_1234.mov"),
            FileTime::from_unix_time(1_672_531_200, 0),
        )
        .unwrap();

        let plan_path = dir.path().join("plan.jsonl");
        let mut opts = options(vec![src], dir.path().join("out"));
        opts.pair_companions = true;
        let plan = scan(&plan_path, &opts, &AtomicBool::new(false), &noop).unwrap();

        let picture = plan
            .entries
            .iter()
            .find(|e| e.source.extension().unwrap() == "jpg")
            .unwrap();
        let clip = plan
            .entries
            .iter()
            .find(|e| e.source.extension().unwrap() == "mov")
            .unwrap();

        assert_eq!(clip.taken, picture.taken);
        assert_eq!(clip.destination.parent(), picture.destination.parent());
        assert_eq!(clip.provider_info.as_deref(), Some("beside IMG_1234.jpg"));
        assert_eq!(
            read_plan(&plan_path).unwrap().entries.len(),
            plan.entries.len()
        );
    }

    #[test]
    fn companions_are_left_alone_when_the_pairing_is_switched_off() {
        use filetime::{set_file_mtime, FileTime};

        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("IMG_1234.jpg"), b"picture").unwrap();
        fs::write(src.join("IMG_1234.mov"), b"clip").unwrap();
        set_file_mtime(
            src.join("IMG_1234.jpg"),
            FileTime::from_unix_time(1_557_818_523, 0),
        )
        .unwrap();
        set_file_mtime(
            src.join("IMG_1234.mov"),
            FileTime::from_unix_time(1_672_531_200, 0),
        )
        .unwrap();

        let plan_path = dir.path().join("plan.jsonl");
        let opts = options(vec![src], dir.path().join("out"));
        let plan = scan(&plan_path, &opts, &AtomicBool::new(false), &noop).unwrap();

        let dates: std::collections::HashSet<_> = plan.entries.iter().map(|e| e.taken).collect();
        assert_eq!(dates.len(), 2);
    }

    #[test]
    fn plans_a_destination_for_every_file() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(src.join("nested")).unwrap();
        fs::write(src.join("IMG_20230506_101112.jpg"), b"one").unwrap();
        fs::write(src.join("nested/VID_20191102_080910.mp4"), b"two").unwrap();

        let plan_path = dir.path().join("plan.jsonl");
        let opts = options(vec![src], dir.path().join("out"));
        let plan = scan(&plan_path, &opts, &AtomicBool::new(false), &noop).unwrap();

        assert_eq!(plan.entries.len(), 2);
        let jpg = plan
            .entries
            .iter()
            .find(|e| e.source.extension().unwrap() == "jpg")
            .unwrap();
        assert_eq!(jpg.provider, Provider::Filename);
        assert!(jpg.destination.ends_with("2023/05/IMG_20230506_101112.jpg"));
        assert_eq!(jpg.size, 3);
    }

    #[test]
    fn resumes_without_reanalysing_known_files() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("IMG_20230506_101112.jpg"), b"one").unwrap();

        let plan_path = dir.path().join("plan.jsonl");
        let opts = options(vec![src.clone()], dir.path().join("out"));
        scan(&plan_path, &opts, &AtomicBool::new(false), &noop).unwrap();

        fs::write(src.join("IMG_20240101_000000.jpg"), b"two").unwrap();
        let plan = scan(&plan_path, &opts, &AtomicBool::new(false), &noop).unwrap();

        assert_eq!(plan.entries.len(), 2);
        assert_eq!(
            plan.entries
                .iter()
                .filter(|e| e.source.ends_with("IMG_20230506_101112.jpg"))
                .count(),
            1
        );
    }

    #[test]
    fn starts_over_when_the_options_changed() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("IMG_20230506_101112.jpg"), b"one").unwrap();

        let plan_path = dir.path().join("plan.jsonl");
        scan(
            &plan_path,
            &options(vec![src.clone()], dir.path().join("out")),
            &AtomicBool::new(false),
            &noop,
        )
        .unwrap();

        let mut changed = options(vec![src], dir.path().join("other"));
        changed.folder_pattern = "%Y".into();
        let plan = scan(&plan_path, &changed, &AtomicBool::new(false), &noop).unwrap();

        assert_eq!(plan.header.folder_pattern, "%Y");
        assert_eq!(plan.entries.len(), 1);
        assert!(plan.entries[0]
            .destination
            .ends_with("2023/IMG_20230506_101112.jpg"));
    }

    #[test]
    fn does_not_walk_into_the_destination_directory() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        let out = src.join("out");
        fs::create_dir_all(&out).unwrap();
        fs::write(src.join("IMG_20230506_101112.jpg"), b"one").unwrap();
        fs::write(out.join("IMG_20230506_101112.jpg"), b"one").unwrap();

        let plan_path = dir.path().join("plan.jsonl");
        let plan = scan(
            &plan_path,
            &options(vec![src], out),
            &AtomicBool::new(false),
            &noop,
        )
        .unwrap();

        assert_eq!(plan.entries.len(), 1);
    }

    #[test]
    fn records_files_no_provider_can_date() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("mystery.bin"), b"one").unwrap();

        let plan_path = dir.path().join("plan.jsonl");
        let mut opts = options(vec![src], dir.path().join("out"));
        opts.detect.providers = vec![Provider::Filename];

        let plan = scan(&plan_path, &opts, &AtomicBool::new(false), &noop).unwrap();
        assert!(plan.entries.is_empty());
        assert_eq!(plan.skipped.len(), 1);
    }

    #[test]
    fn plans_relative_destinations_when_no_destination_is_chosen() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("IMG_20230506_101112.jpg"), b"one").unwrap();

        let plan_path = dir.path().join("plan.jsonl");
        let mut opts = options(vec![src], dir.path().join("out"));
        opts.destination = None;

        let plan = scan(&plan_path, &opts, &AtomicBool::new(false), &noop).unwrap();

        assert_eq!(plan.header.destination, None);
        assert_eq!(plan.entries.len(), 1);
        assert_eq!(
            plan.entries[0].destination,
            PathBuf::from("2023")
                .join("05")
                .join("IMG_20230506_101112.jpg")
        );
        assert!(plan.entries[0].destination.is_relative());
    }

    #[test]
    fn stops_when_cancelled() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("IMG_20230506_101112.jpg"), b"one").unwrap();

        let plan_path = dir.path().join("plan.jsonl");
        let opts = options(vec![src], dir.path().join("out"));
        let err = scan(&plan_path, &opts, &AtomicBool::new(true), &noop).unwrap_err();
        assert!(matches!(err, Error::Cancelled));
    }
}
