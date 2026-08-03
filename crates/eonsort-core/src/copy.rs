use crate::error::{Error, Result};
use crate::model::{duplicate_variant, PlanEntry};
use crate::plan::read_plan;
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

/// Copies every planned file. Already-journalled sources are skipped, so an
/// interrupted run continues where it stopped.
pub fn execute(
    plan_path: &Path,
    options: &CopyOptions,
    cancel: &AtomicBool,
    on_progress: &(dyn Fn(CopyProgress) + Sync),
) -> Result<CopyReport> {
    let plan = read_plan(plan_path)?;
    let journal_file = journal_path(plan_path);
    let done = read_journal(&journal_file)?;

    let staging = plan.header.destination.join(STAGING_DIR);
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
                let outcome = transfer(entry, &staging, options, &state).unwrap_or_else(|e| {
                    Outcome::Failed {
                        error: e.to_string(),
                    }
                });
                state.record(entry, outcome, on_progress);
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
    reserved: Mutex<HashSet<PathBuf>>,
    locks: Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>,
    journal: Mutex<Journal>,
    failures: Mutex<Vec<JournalRecord>>,
    temp_counter: AtomicU64,
}

impl State {
    /// Serialises every worker aiming at the same planned destination, so a
    /// second copy of identical content sees the first one already on disk
    /// instead of racing it into a `_dup_1` name.
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
        outcome: Outcome,
        on_progress: &(dyn Fn(CopyProgress) + Sync),
    ) {
        match &outcome {
            Outcome::Copied { .. } => &self.copied,
            Outcome::Duplicate { .. } => &self.duplicates,
            Outcome::AlreadyPresent { .. } => &self.already_present,
            Outcome::Failed { .. } => &self.failed,
        }
        .fetch_add(1, Ordering::Relaxed);

        self.files_done.fetch_add(1, Ordering::Relaxed);
        self.bytes_done.fetch_add(entry.size, Ordering::Relaxed);

        let record = JournalRecord {
            source: entry.source.clone(),
            outcome,
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
            current,
        }
    }
}

enum Resolution {
    Write { path: PathBuf, duplicate: bool },
    AlreadyPresent { path: PathBuf },
}

fn transfer(
    entry: &PlanEntry,
    staging: &Path,
    options: &CopyOptions,
    state: &State,
) -> Result<Outcome> {
    let source_meta = fs::metadata(&entry.source).map_err(|e| Error::io(&entry.source, e))?;

    let parent = entry
        .destination
        .parent()
        .ok_or_else(|| Error::InvalidSourcePath(entry.destination.clone()))?;
    fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;

    let name_lock = state.lock_for(&entry.destination);
    let _guard = name_lock.lock().unwrap();

    let resolution = resolve(entry, source_meta.len(), state)?;
    let (destination, duplicate) = match resolution {
        Resolution::AlreadyPresent { path } => {
            return Ok(Outcome::AlreadyPresent { destination: path })
        }
        Resolution::Write { path, duplicate } => (path, duplicate),
    };

    let temp = staging.join(format!(
        "{:016x}.{TEMP_EXTENSION}",
        state.temp_counter.fetch_add(1, Ordering::Relaxed)
    ));
    fs::copy(&entry.source, &temp).map_err(|e| Error::io(&entry.source, e))?;
    fs::OpenOptions::new()
        .write(true)
        .open(&temp)
        .and_then(|f| f.sync_all())
        .map_err(|e| Error::io(&temp, e))?;

    if options.preserve_times {
        let mtime = filetime::FileTime::from_last_modification_time(&source_meta);
        let atime = filetime::FileTime::from_last_access_time(&source_meta);
        let _ = filetime::set_file_times(&temp, atime, mtime);
    }

    fs::rename(&temp, &destination).map_err(|e| Error::io(&destination, e))?;

    Ok(if duplicate {
        Outcome::Duplicate { destination }
    } else {
        Outcome::Copied { destination }
    })
}

fn resolve(entry: &PlanEntry, source_size: u64, state: &State) -> Result<Resolution> {
    let mut source_hash: Option<blake3::Hash> = None;

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

        if meta.len() != source_size {
            continue;
        }
        let expected = match &source_hash {
            Some(hash) => *hash,
            None => *source_hash.insert(hash_file(&entry.source)?),
        };
        if hash_file(&candidate)? == expected {
            return Ok(Resolution::AlreadyPresent { path: candidate });
        }
    }

    Err(Error::DestinationExhausted(entry.destination.clone()))
}

fn hash_file(path: &Path) -> Result<blake3::Hash> {
    let file = File::open(path).map_err(|e| Error::io(path, e))?;
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(file).map_err(|e| Error::io(path, e))?;
    Ok(hasher.finalize())
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
                destination: dir.path().join("out"),
                folder_pattern: DEFAULT_FOLDER_PATTERN.to_string(),
                detect: DetectOptions::default(),
                follow_symlinks: false,
            };
            scan(&plan, &options, &AtomicBool::new(false), &|_| {}).unwrap();
            Self { dir, plan }
        }

        fn out(&self, relative: &str) -> PathBuf {
            self.dir.path().join("out").join(relative)
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
