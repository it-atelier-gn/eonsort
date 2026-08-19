use crate::error::{Error, Result};
use crate::model::{duplicate_variant, PlanEntry};
use crate::rotate::{self, Written};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub const TEMP_EXTENSION: &str = "eonsort-part";
pub const STAGING_DIR: &str = ".eonsort-tmp";
const MAX_DUPLICATE_ATTEMPTS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopyOptions {
    pub concurrency: usize,
    pub preserve_times: bool,
}

impl Default for CopyOptions {
    fn default() -> Self {
        Self {
            concurrency: default_concurrency(),
            preserve_times: true,
        }
    }
}

pub fn default_concurrency() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().clamp(1, 8))
        .unwrap_or(4)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    Copied { destination: PathBuf },
    Duplicate { destination: PathBuf },
    AlreadyPresent { destination: PathBuf },
    Failed { error: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalRecord {
    pub source: PathBuf,
    #[serde(flatten)]
    pub outcome: Outcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written: Option<Written>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CopyProgress {
    pub files_done: u64,
    pub files_total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub copied: u64,
    pub duplicates: u64,
    pub already_present: u64,
    pub failed: u64,
    pub turned: u64,
    pub not_turned: u64,
    pub current: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CopyReport {
    pub progress: CopyProgress,
    pub failures: Vec<JournalRecord>,
}

pub fn journal_path(plan_path: &Path) -> PathBuf {
    plan_path.with_extension("journal.jsonl")
}

pub fn read_journal(path: &Path) -> Result<HashMap<PathBuf, Outcome>> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(e) => return Err(Error::io(path, e)),
    };
    let mut done = HashMap::new();
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|e| Error::io(path, e))?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(record) = serde_json::from_str::<JournalRecord>(&line) {
            done.insert(record.source, record.outcome);
        }
    }
    Ok(done)
}

pub fn read_written(path: &Path) -> Result<HashMap<PathBuf, Written>> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(e) => return Err(Error::io(path, e)),
    };
    let mut written = HashMap::new();
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|e| Error::io(path, e))?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(record) = serde_json::from_str::<JournalRecord>(&line) {
            match record.written {
                Some(value) => {
                    written.insert(record.source, value);
                }
                None => {
                    written.remove(&record.source);
                }
            }
        }
    }
    Ok(written)
}

pub fn execute(
    plan_path: &Path,
    options: &CopyOptions,
    cancel: &AtomicBool,
    on_progress: &(dyn Fn(CopyProgress) + Sync),
) -> Result<CopyReport> {
    let plan = crate::overrides::load_plan(plan_path)?;
    let journal_file = journal_path(plan_path);
    let done = read_journal(&journal_file)?;

    let staging = plan
        .header
        .destination
        .as_ref()
        .ok_or(Error::NoDestination)?
        .join(STAGING_DIR);
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|e| Error::io(&staging, e))?;

    let pending: Vec<&PlanEntry> = plan
        .entries
        .iter()
        .filter(|e| !done.contains_key(&e.source))
        .collect();

    let state = State {
        files_total: plan.entries.len() as u64,
        bytes_total: plan.total_bytes(),
        files_done: AtomicU64::new((plan.entries.len() - pending.len()) as u64),
        bytes_done: AtomicU64::new(
            plan.entries
                .iter()
                .filter(|e| done.contains_key(&e.source))
                .map(|e| e.size)
                .sum(),
        ),
        copied: AtomicU64::new(count_of(&done, |o| matches!(o, Outcome::Copied { .. }))),
        duplicates: AtomicU64::new(count_of(&done, |o| matches!(o, Outcome::Duplicate { .. }))),
        already_present: AtomicU64::new(count_of(&done, |o| {
            matches!(o, Outcome::AlreadyPresent { .. })
        })),
        failed: AtomicU64::new(count_of(&done, |o| matches!(o, Outcome::Failed { .. }))),
        turned: AtomicU64::new(0),
        not_turned: AtomicU64::new(0),
        reserved: Mutex::new(HashSet::new()),
        locks: Mutex::new(HashMap::new()),
        journal: Mutex::new(Journal::open(&journal_file)?),
        failures: Mutex::new(Vec::new()),
        temp_counter: AtomicU64::new(0),
    };

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(options.concurrency.max(1))
        .build()
        .map_err(|e| Error::ThreadPool(e.to_string()))?;

    let cancelled = pool.install(|| {
        use rayon::prelude::*;
        pending
            .par_iter()
            .map(|entry| {
                if cancel.load(Ordering::Relaxed) {
                    return true;
                }
                let transferred =
                    transfer(entry, &staging, options, &state).unwrap_or_else(|e| Transferred {
                        outcome: Outcome::Failed {
                            error: e.to_string(),
                        },
                        written: None,
                        refused: false,
                    });
                state.record(entry, transferred, on_progress);
                false
            })
            .reduce(|| false, |a, b| a || b)
    });

    state.journal.lock().unwrap().flush()?;
    let _ = fs::remove_dir_all(&staging);

    if cancelled {
        return Err(Error::Cancelled);
    }

    Ok(CopyReport {
        progress: state.snapshot(None),
        failures: state.failures.into_inner().unwrap(),
    })
}

fn count_of(done: &HashMap<PathBuf, Outcome>, pred: fn(&Outcome) -> bool) -> u64 {
    done.values().filter(|o| pred(o)).count() as u64
}

struct State {
    files_total: u64,
    bytes_total: u64,
    files_done: AtomicU64,
    bytes_done: AtomicU64,
    copied: AtomicU64,
    duplicates: AtomicU64,
    already_present: AtomicU64,
    failed: AtomicU64,
    turned: AtomicU64,
    not_turned: AtomicU64,
    reserved: Mutex<HashSet<PathBuf>>,
    locks: Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>,
    journal: Mutex<Journal>,
    failures: Mutex<Vec<JournalRecord>>,
    temp_counter: AtomicU64,
}

impl State {
    fn lock_for(&self, destination: &Path) -> Arc<Mutex<()>> {
        self.locks
            .lock()
            .unwrap()
            .entry(destination.to_path_buf())
            .or_default()
            .clone()
    }

    fn record(
        &self,
        entry: &PlanEntry,
        transferred: Transferred,
        on_progress: &(dyn Fn(CopyProgress) + Sync),
    ) {
        let Transferred {
            outcome,
            written,
            refused,
        } = transferred;

        match &outcome {
            Outcome::Copied { .. } => &self.copied,
            Outcome::Duplicate { .. } => &self.duplicates,
            Outcome::AlreadyPresent { .. } => &self.already_present,
            Outcome::Failed { .. } => &self.failed,
        }
        .fetch_add(1, Ordering::Relaxed);

        if written.is_some() {
            self.turned.fetch_add(1, Ordering::Relaxed);
        }
        if refused {
            self.not_turned.fetch_add(1, Ordering::Relaxed);
        }

        self.files_done.fetch_add(1, Ordering::Relaxed);
        self.bytes_done.fetch_add(entry.size, Ordering::Relaxed);

        let record = JournalRecord {
            source: entry.source.clone(),
            outcome,
            written,
        };
        if matches!(record.outcome, Outcome::Failed { .. }) {
            self.failures.lock().unwrap().push(record.clone());
        }
        let _ = self.journal.lock().unwrap().write(&record);

        on_progress(self.snapshot(Some(entry.source.clone())));
    }

    fn snapshot(&self, current: Option<PathBuf>) -> CopyProgress {
        CopyProgress {
            files_done: self.files_done.load(Ordering::Relaxed),
            files_total: self.files_total,
            bytes_done: self.bytes_done.load(Ordering::Relaxed),
            bytes_total: self.bytes_total,
            copied: self.copied.load(Ordering::Relaxed),
            duplicates: self.duplicates.load(Ordering::Relaxed),
            already_present: self.already_present.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
            turned: self.turned.load(Ordering::Relaxed),
            not_turned: self.not_turned.load(Ordering::Relaxed),
            current,
        }
    }
}

enum Resolution {
    Write { path: PathBuf, duplicate: bool },
    AlreadyPresent { path: PathBuf },
}

struct Transferred {
    outcome: Outcome,
    written: Option<Written>,
    refused: bool,
}

fn transfer(
    entry: &PlanEntry,
    staging: &Path,
    options: &CopyOptions,
    state: &State,
) -> Result<Transferred> {
    let source_meta = fs::metadata(&entry.source).map_err(|e| Error::io(&entry.source, e))?;

    let parent = entry
        .destination
        .parent()
        .ok_or_else(|| Error::InvalidSourcePath(entry.destination.clone()))?;
    fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;

    let temp = staging.join(format!(
        "{:016x}.{TEMP_EXTENSION}",
        state.temp_counter.fetch_add(1, Ordering::Relaxed)
    ));

    let mut refused = false;
    let turned = if entry.rotate.is_identity() {
        None
    } else {
        match rotate::write_rotated(&entry.source, &temp, entry.rotate, entry.reencode) {
            Ok(written) => Some(written),
            Err(Error::RotationNotLossless(_)) => {
                refused = true;
                None
            }
            Err(e) => return Err(e),
        }
    };

    let name_lock = state.lock_for(&entry.destination);
    let _guard = name_lock.lock().unwrap();

    let expected_size = turned.as_ref().map_or(source_meta.len(), |w| w.size);
    let expected_hash = turned.as_ref().map(|w| w.hash.as_str());

    let resolution = resolve(entry, expected_size, expected_hash, state)?;
    let (destination, duplicate) = match resolution {
        Resolution::AlreadyPresent { path } => {
            let _ = fs::remove_file(&temp);
            return Ok(Transferred {
                outcome: Outcome::AlreadyPresent { destination: path },
                written: turned,
                refused,
            });
        }
        Resolution::Write { path, duplicate } => (path, duplicate),
    };

    if turned.is_none() {
        fs::copy(&entry.source, &temp).map_err(|e| Error::io(&entry.source, e))?;
        fs::OpenOptions::new()
            .write(true)
            .open(&temp)
            .and_then(|f| f.sync_all())
            .map_err(|e| Error::io(&temp, e))?;
    }

    if options.preserve_times {
        let mtime = filetime::FileTime::from_last_modification_time(&source_meta);
        let atime = filetime::FileTime::from_last_access_time(&source_meta);
        let _ = filetime::set_file_times(&temp, atime, mtime);
    }

    fs::rename(&temp, &destination).map_err(|e| Error::io(&destination, e))?;

    Ok(Transferred {
        outcome: if duplicate {
            Outcome::Duplicate { destination }
        } else {
            Outcome::Copied { destination }
        },
        written: turned,
        refused,
    })
}

fn resolve(
    entry: &PlanEntry,
    expected_size: u64,
    expected_hash: Option<&str>,
    state: &State,
) -> Result<Resolution> {
    let mut known: Option<String> = expected_hash.map(str::to_string);

    for index in 0..MAX_DUPLICATE_ATTEMPTS {
        let candidate = if index == 0 {
            entry.destination.clone()
        } else {
            duplicate_variant(&entry.destination, index)
        };

        let existing = match fs::metadata(&candidate) {
            Ok(meta) => Some(meta),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(Error::io(&candidate, e)),
        };

        let Some(meta) = existing else {
            let mut reserved = state.reserved.lock().unwrap();
            if reserved.contains(&candidate) {
                continue;
            }
            reserved.insert(candidate.clone());
            return Ok(Resolution::Write {
                path: candidate,
                duplicate: index > 0,
            });
        };

        if meta.len() != expected_size {
            continue;
        }
        let expected = match &known {
            Some(hash) => hash.clone(),
            None => known.insert(hash_file(&entry.source)?).clone(),
        };
        if hash_file(&candidate)? == expected {
            return Ok(Resolution::AlreadyPresent { path: candidate });
        }
    }

    Err(Error::DestinationExhausted(entry.destination.clone()))
}

fn hash_file(path: &Path) -> Result<String> {
    let file = File::open(path).map_err(|e| Error::io(path, e))?;
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(file).map_err(|e| Error::io(path, e))?;
    Ok(hasher.finalize().to_hex().to_string())
}

struct Journal {
    path: PathBuf,
    file: BufWriter<File>,
}

impl Journal {
    fn open(path: &Path) -> Result<Self> {
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| Error::io(path, e))?;
        Ok(Self {
            path: path.to_path_buf(),
            file: BufWriter::new(file),
        })
    }

    fn write(&mut self, record: &JournalRecord) -> Result<()> {
        let line = serde_json::to_string(record)?;
        writeln!(self.file, "{line}").map_err(|e| Error::io(&self.path, e))
    }

    fn flush(&mut self) -> Result<()> {
        self.file.flush().map_err(|e| Error::io(&self.path, e))
    }
}

impl Drop for Journal {
    fn drop(&mut self) {
        let _ = self.file.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DEFAULT_FOLDER_PATTERN;
    use crate::providers::DetectOptions;
    use crate::scan::{scan, ScanOptions};
    use tempfile::{tempdir, TempDir};

    struct Fixture {
        dir: TempDir,
        plan: PathBuf,
    }

    impl Fixture {
        fn new(files: &[(&str, &[u8])]) -> Self {
            let dir = tempdir().unwrap();
            let src = dir.path().join("src");
            fs::create_dir_all(&src).unwrap();
            for (name, body) in files {
                let path = src.join(name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, body).unwrap();
            }

            let plan = dir.path().join("plan.jsonl");
            let options = ScanOptions {
                sources: vec![src],
                destination: Some(dir.path().join("out")),
                folder_pattern: DEFAULT_FOLDER_PATTERN.to_string(),
                detect: DetectOptions::default(),
                follow_symlinks: false,
                auto_rotate: false,
                pair_companions: false,
                upright_model_dir: None,
            };
            scan(&plan, &options, &AtomicBool::new(false), &|_| {}).unwrap();
            Self { dir, plan }
        }

        fn turning(files: &[(&str, &[u8])]) -> Self {
            let dir = tempdir().unwrap();
            let src = dir.path().join("src");
            fs::create_dir_all(&src).unwrap();
            for (name, body) in files {
                fs::write(src.join(name), body).unwrap();
            }

            let plan = dir.path().join("plan.jsonl");
            let options = ScanOptions {
                sources: vec![src],
                destination: Some(dir.path().join("out")),
                folder_pattern: DEFAULT_FOLDER_PATTERN.to_string(),
                detect: DetectOptions::default(),
                follow_symlinks: false,
                auto_rotate: true,
                pair_companions: false,
                upright_model_dir: None,
            };
            scan(&plan, &options, &AtomicBool::new(false), &|_| {}).unwrap();
            Self { dir, plan }
        }

        fn out(&self, relative: &str) -> PathBuf {
            self.dir.path().join("out").join(relative)
        }

        fn landed(&self) -> Vec<PathBuf> {
            let mut found: Vec<PathBuf> = walkdir::WalkDir::new(self.dir.path().join("out"))
                .into_iter()
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.file_type().is_file())
                .map(|entry| entry.into_path())
                .collect();
            found.sort();
            found
        }

        fn run(&self) -> CopyReport {
            execute(
                &self.plan,
                &CopyOptions::default(),
                &AtomicBool::new(false),
                &|_| {},
            )
            .unwrap()
        }
    }

    #[test]
    fn turns_a_sideways_picture_and_records_what_it_wrote() {
        let sideways = crate::exif_write::jpeg_with_exif(64, 32, 6);
        let fixture = Fixture::turning(&[("IMG_20030101_000012.jpg", &sideways)]);

        let report = fixture.run();

        assert_eq!(report.progress.copied, 1);
        assert_eq!(report.progress.turned, 1);
        assert_eq!(report.progress.not_turned, 0);

        let landed = fixture.landed().pop().unwrap();
        assert_eq!(crate::rotate::read_orientation(&landed), 1);

        let bytes = fs::read(&landed).unwrap();
        let decoded =
            image::load_from_memory_with_format(&bytes, image::ImageFormat::Jpeg).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (32, 64));

        let written = read_written(&journal_path(&fixture.plan)).unwrap();
        let record = written
            .get(&fixture.dir.path().join("src/IMG_20030101_000012.jpg"))
            .unwrap();
        assert_eq!(record.size, bytes.len() as u64);
        assert_eq!(record.hash, blake3::hash(&bytes).to_hex().to_string());
    }

    #[test]
    fn copying_a_turned_picture_again_recognises_it_rather_than_duplicating_it() {
        let sideways = crate::exif_write::jpeg_with_exif(64, 32, 6);
        let fixture = Fixture::turning(&[("IMG_20030101_000012.jpg", &sideways)]);

        fixture.run();
        fs::remove_file(journal_path(&fixture.plan)).unwrap();
        let second = fixture.run();

        assert_eq!(second.progress.already_present, 1);
        assert_eq!(second.progress.copied, 0);
        assert_eq!(fixture.landed().len(), 1);
    }

    #[test]
    fn a_picture_that_cannot_be_turned_losslessly_is_copied_as_it_is() {
        let ragged = crate::exif_write::jpeg_with_exif(99, 49, 6);
        let fixture = Fixture::turning(&[("IMG_20030101_000012.jpg", &ragged)]);

        let report = fixture.run();

        assert_eq!(report.progress.copied, 1);
        assert_eq!(report.progress.turned, 0);
        assert_eq!(report.progress.not_turned, 1);

        let landed = fixture.landed().pop().unwrap();
        assert_eq!(fs::read(&landed).unwrap(), ragged);
        assert_eq!(crate::rotate::read_orientation(&landed), 6);
    }

    #[test]
    fn an_upright_picture_is_left_completely_alone() {
        let upright = crate::exif_write::jpeg_with_exif(64, 32, 1);
        let fixture = Fixture::turning(&[("IMG_20030101_000012.jpg", &upright)]);

        let report = fixture.run();

        assert_eq!(report.progress.turned, 0);
        assert_eq!(report.progress.not_turned, 0);
        assert_eq!(fs::read(fixture.landed().pop().unwrap()).unwrap(), upright);
        assert!(read_written(&journal_path(&fixture.plan))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn copies_to_the_folder_the_user_corrected_the_date_to() {
        let fixture = Fixture::new(&[("IMG_20030101_000012.jpg", b"alpha")]);
        let source = fixture
            .dir
            .path()
            .join("src")
            .join("IMG_20030101_000012.jpg");

        let mut corrections = crate::overrides::Overrides::default();
        corrections.set(
            source,
            crate::overrides::DateOverride {
                taken: chrono::NaiveDate::from_ymd_opt(2019, 7, 4)
                    .unwrap()
                    .and_hms_opt(18, 30, 0)
                    .unwrap(),
                origin: crate::overrides::OverrideOrigin::Manual,
                at: chrono::NaiveDate::from_ymd_opt(2026, 8, 6)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap(),
            },
        );
        crate::overrides::write(
            &crate::overrides::overrides_path(&fixture.plan),
            &corrections,
        )
        .unwrap();

        let report = fixture.run();

        assert_eq!(report.progress.copied, 1);
        assert!(!fixture.out("2003/01/IMG_20030101_000012.jpg").exists());
        assert_eq!(
            fs::read(fixture.out("2019/07/IMG_20030101_000012.jpg")).unwrap(),
            b"alpha"
        );
    }

    #[test]
    fn copies_files_into_year_and_month_folders() {
        let fixture = Fixture::new(&[
            ("IMG_20230506_101112.jpg", b"alpha"),
            ("IMG_20191102_080910.jpg", b"beta"),
        ]);

        let report = fixture.run();

        assert_eq!(report.progress.copied, 2);
        assert_eq!(report.progress.failed, 0);
        assert_eq!(
            fs::read(fixture.out("2023/05/IMG_20230506_101112.jpg")).unwrap(),
            b"alpha"
        );
        assert_eq!(
            fs::read(fixture.out("2019/11/IMG_20191102_080910.jpg")).unwrap(),
            b"beta"
        );
    }

    #[test]
    fn leaves_no_staging_directory_behind() {
        let fixture = Fixture::new(&[("IMG_20230506_101112.jpg", b"alpha")]);
        fixture.run();
        assert!(!fixture.out(STAGING_DIR).exists());
    }

    #[test]
    fn preserves_the_source_modification_time() {
        let fixture = Fixture::new(&[("IMG_20230506_101112.jpg", b"alpha")]);
        let source = fixture.dir.path().join("src/IMG_20230506_101112.jpg");
        let backdated = filetime::FileTime::from_unix_time(1_000_000_000, 0);
        filetime::set_file_mtime(&source, backdated).unwrap();

        fixture.run();

        let copied = fs::metadata(fixture.out("2023/05/IMG_20230506_101112.jpg")).unwrap();
        assert_eq!(
            filetime::FileTime::from_last_modification_time(&copied),
            backdated
        );
    }

    #[test]
    fn running_twice_does_not_duplicate_anything() {
        let fixture = Fixture::new(&[("IMG_20230506_101112.jpg", b"alpha")]);
        fixture.run();

        fs::remove_file(journal_path(&fixture.plan)).unwrap();
        let report = fixture.run();

        assert_eq!(report.progress.already_present, 1);
        assert_eq!(report.progress.copied, 0);
        assert!(!fixture
            .out("2023/05/IMG_20230506_101112_dup_1.jpg")
            .exists());
    }

    #[test]
    fn resumes_from_the_journal_without_recopying() {
        let fixture = Fixture::new(&[
            ("IMG_20230506_101112.jpg", b"alpha"),
            ("IMG_20230507_101112.jpg", b"beta"),
        ]);
        fixture.run();

        let report = fixture.run();
        assert_eq!(report.progress.files_done, 2);
        assert_eq!(report.progress.copied, 2);
        assert_eq!(report.progress.already_present, 0);
    }

    #[test]
    fn a_same_name_file_with_different_content_becomes_a_duplicate() {
        let fixture = Fixture::new(&[("IMG_20230506_101112.jpg", b"alpha")]);
        let existing = fixture.out("2023/05/IMG_20230506_101112.jpg");
        fs::create_dir_all(existing.parent().unwrap()).unwrap();
        fs::write(&existing, b"something else").unwrap();

        let report = fixture.run();

        assert_eq!(report.progress.duplicates, 1);
        assert_eq!(
            fs::read(fixture.out("2023/05/IMG_20230506_101112_dup_1.jpg")).unwrap(),
            b"alpha"
        );
        assert_eq!(fs::read(&existing).unwrap(), b"something else");
    }

    #[test]
    fn distinct_sources_with_the_same_name_land_side_by_side() {
        let fixture = Fixture::new(&[
            ("a/IMG_20230506_101112.jpg", b"alpha"),
            ("b/IMG_20230506_101112.jpg", b"bravo"),
        ]);

        let report = fixture.run();

        assert_eq!(report.progress.copied, 1);
        assert_eq!(report.progress.duplicates, 1);
        let mut bodies = vec![
            fs::read(fixture.out("2023/05/IMG_20230506_101112.jpg")).unwrap(),
            fs::read(fixture.out("2023/05/IMG_20230506_101112_dup_1.jpg")).unwrap(),
        ];
        bodies.sort();
        assert_eq!(bodies, vec![b"alpha".to_vec(), b"bravo".to_vec()]);
    }

    #[test]
    fn identical_content_under_the_same_name_is_stored_once() {
        let fixture = Fixture::new(&[
            ("a/IMG_20230506_101112.jpg", b"same"),
            ("b/IMG_20230506_101112.jpg", b"same"),
        ]);

        let report = fixture.run();

        assert_eq!(report.progress.copied, 1);
        assert_eq!(report.progress.already_present, 1);
        assert!(!fixture
            .out("2023/05/IMG_20230506_101112_dup_1.jpg")
            .exists());
    }

    #[test]
    fn records_a_failure_when_the_source_vanished() {
        let fixture = Fixture::new(&[("IMG_20230506_101112.jpg", b"alpha")]);
        fs::remove_file(fixture.dir.path().join("src/IMG_20230506_101112.jpg")).unwrap();

        let report = fixture.run();

        assert_eq!(report.progress.failed, 1);
        assert_eq!(report.failures.len(), 1);
    }

    #[test]
    fn cancelling_leaves_the_run_resumable() {
        let fixture = Fixture::new(&[("IMG_20230506_101112.jpg", b"alpha")]);
        let err = execute(
            &fixture.plan,
            &CopyOptions::default(),
            &AtomicBool::new(true),
            &|_| {},
        )
        .unwrap_err();

        assert!(matches!(err, Error::Cancelled));
        assert!(read_journal(&journal_path(&fixture.plan))
            .unwrap()
            .is_empty());

        let report = fixture.run();
        assert_eq!(report.progress.copied, 1);
    }
}
