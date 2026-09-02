use crate::copy::{journal_path, read_journal, read_written, Outcome};
use crate::error::{Error, Result};
use crate::rotate::Written;
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Removable,
    Missing,
    Changed,
    LeftAlone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Candidate {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub bytes: u64,
    pub verdict: Verdict,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UndoReport {
    pub removed: u64,
    pub bytes: u64,
    pub missing: u64,
    pub changed: u64,
    pub left_alone: u64,
    pub sidecars: u64,
    pub folders: u64,
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UndoOptions {
    pub dry_run: bool,
    pub prune_folders: bool,
}

impl Default for UndoOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            prune_folders: true,
        }
    }
}

pub fn survey(plan_path: &Path) -> Result<Vec<Candidate>> {
    let journal = journal_path(plan_path);
    let outcomes = read_journal(&journal)?;
    let written = read_written(&journal)?;

    let mut candidates: Vec<Candidate> = outcomes
        .into_iter()
        .filter_map(|(source, outcome)| candidate(source, outcome, &written))
        .collect();
    candidates.sort_by(|a, b| a.destination.cmp(&b.destination));
    Ok(candidates)
}

fn candidate(
    source: PathBuf,
    outcome: Outcome,
    written: &HashMap<PathBuf, Written>,
) -> Option<Candidate> {
    let (destination, ours) = match outcome {
        Outcome::Copied { destination } | Outcome::Duplicate { destination } => (destination, true),
        Outcome::AlreadyPresent { destination } => (destination, false),
        Outcome::Failed { .. } => return None,
    };

    if !ours {
        return Some(Candidate {
            source,
            destination,
            bytes: 0,
            verdict: Verdict::LeftAlone,
        });
    }

    let Ok(meta) = std::fs::metadata(&destination) else {
        return Some(Candidate {
            source,
            destination,
            bytes: 0,
            verdict: Verdict::Missing,
        });
    };
    if !meta.is_file() {
        return Some(Candidate {
            source,
            destination,
            bytes: 0,
            verdict: Verdict::LeftAlone,
        });
    }

    let bytes = meta.len();
    let verdict = match untouched(&source, &destination, bytes, written) {
        true => Verdict::Removable,
        false => Verdict::Changed,
    };
    Some(Candidate {
        source,
        destination,
        bytes,
        verdict,
    })
}

fn untouched(
    source: &Path,
    destination: &Path,
    bytes: u64,
    written: &HashMap<PathBuf, Written>,
) -> bool {
    if let Some(record) = written.get(source) {
        return record.size == bytes && hash_of(destination).as_deref() == Some(&record.hash);
    }
    match std::fs::metadata(source) {
        Ok(meta) => meta.len() == bytes,
        Err(_) => false,
    }
}

fn hash_of(path: &Path) -> Option<String> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(std::fs::File::open(path).ok()?).ok()?;
    Some(hasher.finalize().to_hex().to_string())
}

pub fn execute(plan_path: &Path, options: &UndoOptions, cancel: &AtomicBool) -> Result<UndoReport> {
    let candidates = survey(plan_path)?;
    let mut report = UndoReport::default();
    let mut folders: BTreeSet<PathBuf> = BTreeSet::new();

    for candidate in &candidates {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        match candidate.verdict {
            Verdict::Missing => report.missing += 1,
            Verdict::Changed => report.changed += 1,
            Verdict::LeftAlone => report.left_alone += 1,
            Verdict::Removable => {
                if options.dry_run {
                    report.removed += 1;
                    report.bytes += candidate.bytes;
                    continue;
                }
                match std::fs::remove_file(&candidate.destination) {
                    Ok(()) => {
                        report.removed += 1;
                        report.bytes += candidate.bytes;
                        let sidecar = crate::xmp_write::sidecar_path(&candidate.destination);
                        if sidecar.is_file() {
                            let _ = std::fs::remove_file(&sidecar);
                            report.sidecars += 1;
                        }
                        if let Some(parent) = candidate.destination.parent() {
                            folders.insert(parent.to_path_buf());
                        }
                    }
                    Err(e) => report
                        .failures
                        .push(format!("{}: {e}", candidate.destination.display())),
                }
            }
        }
    }

    if options.prune_folders && !options.dry_run {
        report.folders = prune(&folders, root_of(plan_path)?.as_deref());
    }

    if !options.dry_run && report.removed > 0 {
        let journal = journal_path(plan_path);
        if journal.is_file() {
            std::fs::remove_file(&journal).map_err(|e| Error::io(&journal, e))?;
        }
    }
    Ok(report)
}

fn root_of(plan_path: &Path) -> Result<Option<PathBuf>> {
    Ok(crate::plan::read_plan(plan_path)?.header.destination)
}

fn prune(folders: &BTreeSet<PathBuf>, root: Option<&Path>) -> u64 {
    let mut removed = 0;
    let mut queue: Vec<PathBuf> = folders.iter().cloned().collect();
    queue.sort();
    queue.reverse();

    while let Some(folder) = queue.pop() {
        if root.is_some_and(|root| !folder.starts_with(root) || folder == root) {
            continue;
        }
        if root.is_none() {
            continue;
        }
        let empty = std::fs::read_dir(&folder)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
        if !empty {
            continue;
        }
        if std::fs::remove_dir(&folder).is_ok() {
            removed += 1;
            if let Some(parent) = folder.parent() {
                queue.push(parent.to_path_buf());
            }
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::copy::{CopyOptions, JournalRecord};
    use crate::model::{PlanEntry, PlanHeader, PlanRecord, DEFAULT_FOLDER_PATTERN, PLAN_VERSION};
    use crate::plan::PlanWriter;
    use crate::providers::DetectOptions;
    use chrono::NaiveDate;
    use std::fs;
    use tempfile::TempDir;

    struct Fixture {
        _dir: TempDir,
        plan: PathBuf,
        source: PathBuf,
        destination: PathBuf,
    }

    fn fixture(names: &[&str]) -> Fixture {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("src");
        let destination = dir.path().join("out");
        fs::create_dir_all(&source).unwrap();

        let plan = dir.path().join("plan.jsonl");
        let header = PlanHeader {
            version: PLAN_VERSION,
            created_at: NaiveDate::from_ymd_opt(2026, 8, 6)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            sources: vec![source.clone()],
            destination: Some(destination.clone()),
            folder_pattern: DEFAULT_FOLDER_PATTERN.to_string(),
            name_pattern: crate::naming::DEFAULT_NAME_PATTERN.to_string(),
            detect: DetectOptions::default(),
        };

        let mut writer = PlanWriter::create(&plan, &header).unwrap();
        for name in names {
            let file = source.join(name);
            fs::write(&file, format!("payload of {name}")).unwrap();
            writer
                .write(&PlanRecord::Entry(PlanEntry {
                    source: file.clone(),
                    destination: destination.join("2023").join("05").join(name),
                    taken: NaiveDate::from_ymd_opt(2023, 5, 6)
                        .unwrap()
                        .and_hms_opt(0, 0, 0)
                        .unwrap(),
                    size: fs::metadata(&file).unwrap().len(),
                    ..PlanEntry::default()
                }))
                .unwrap();
        }
        drop(writer);

        Fixture {
            _dir: dir,
            plan,
            source,
            destination,
        }
    }

    fn copied(fixture: &Fixture) {
        crate::copy::execute(
            &fixture.plan,
            &CopyOptions::default(),
            &AtomicBool::new(false),
            &|_| {},
        )
        .unwrap();
    }

    fn journal_line(fixture: &Fixture, record: &JournalRecord) {
        let path = journal_path(&fixture.plan);
        let mut body = fs::read_to_string(&path).unwrap_or_default();
        body.push_str(&serde_json::to_string(record).unwrap());
        body.push('\n');
        fs::write(&path, body).unwrap();
    }

    #[test]
    fn takes_back_every_file_the_copy_wrote() {
        let fixture = fixture(&["a.jpg", "b.jpg"]);
        copied(&fixture);
        assert!(fixture.destination.join("2023/05/a.jpg").is_file());

        let report = execute(
            &fixture.plan,
            &UndoOptions::default(),
            &AtomicBool::new(false),
        )
        .unwrap();

        assert_eq!(report.removed, 2);
        assert!(report.bytes > 0);
        assert!(!fixture.destination.join("2023/05/a.jpg").exists());
        assert!(!fixture.destination.join("2023/05/b.jpg").exists());
    }

    #[test]
    fn the_sources_are_never_touched() {
        let fixture = fixture(&["a.jpg"]);
        copied(&fixture);
        execute(
            &fixture.plan,
            &UndoOptions::default(),
            &AtomicBool::new(false),
        )
        .unwrap();

        assert!(fixture.source.join("a.jpg").is_file());
    }

    #[test]
    fn a_file_that_was_already_there_is_left_where_it_stands() {
        let fixture = fixture(&["a.jpg"]);
        let landing = fixture.destination.join("2023").join("05");
        fs::create_dir_all(&landing).unwrap();
        let same = fs::read(fixture.source.join("a.jpg")).unwrap();
        fs::write(landing.join("a.jpg"), &same).unwrap();
        copied(&fixture);

        let report = execute(
            &fixture.plan,
            &UndoOptions::default(),
            &AtomicBool::new(false),
        )
        .unwrap();

        assert_eq!(report.removed, 0);
        assert_eq!(report.left_alone, 1);
        assert_eq!(fs::read(landing.join("a.jpg")).unwrap(), same);
    }

    #[test]
    fn a_stranger_of_the_same_name_keeps_its_file_and_loses_only_our_variant() {
        let fixture = fixture(&["a.jpg"]);
        let landing = fixture.destination.join("2023").join("05");
        fs::create_dir_all(&landing).unwrap();
        fs::write(landing.join("a.jpg"), "someone else's picture").unwrap();
        copied(&fixture);

        let ours = landing.join("a_dup_1.jpg");
        assert!(ours.is_file());

        let report = execute(
            &fixture.plan,
            &UndoOptions::default(),
            &AtomicBool::new(false),
        )
        .unwrap();

        assert_eq!(report.removed, 1);
        assert!(!ours.exists());
        assert_eq!(
            fs::read_to_string(landing.join("a.jpg")).unwrap(),
            "someone else's picture"
        );
    }

    #[test]
    fn a_copy_edited_since_it_landed_is_kept() {
        let fixture = fixture(&["a.jpg"]);
        copied(&fixture);
        let landed = fixture.destination.join("2023").join("05").join("a.jpg");
        fs::write(&landed, "edited since the copy").unwrap();

        let report = execute(
            &fixture.plan,
            &UndoOptions::default(),
            &AtomicBool::new(false),
        )
        .unwrap();

        assert_eq!(report.removed, 0);
        assert_eq!(report.changed, 1);
        assert!(landed.is_file());
    }

    #[test]
    fn a_copy_someone_already_deleted_is_only_counted() {
        let fixture = fixture(&["a.jpg"]);
        copied(&fixture);
        fs::remove_file(fixture.destination.join("2023").join("05").join("a.jpg")).unwrap();

        let report = execute(
            &fixture.plan,
            &UndoOptions::default(),
            &AtomicBool::new(false),
        )
        .unwrap();

        assert_eq!(report.removed, 0);
        assert_eq!(report.missing, 1);
    }

    #[test]
    fn a_dry_run_reports_without_removing_anything() {
        let fixture = fixture(&["a.jpg", "b.jpg"]);
        copied(&fixture);

        let options = UndoOptions {
            dry_run: true,
            ..UndoOptions::default()
        };
        let report = execute(&fixture.plan, &options, &AtomicBool::new(false)).unwrap();

        assert_eq!(report.removed, 2);
        assert!(fixture.destination.join("2023/05/a.jpg").is_file());
        assert!(journal_path(&fixture.plan).is_file());
    }

    #[test]
    fn the_folders_the_copy_made_are_cleared_away_behind_it() {
        let fixture = fixture(&["a.jpg"]);
        copied(&fixture);
        assert!(fixture.destination.join("2023").join("05").is_dir());

        let report = execute(
            &fixture.plan,
            &UndoOptions::default(),
            &AtomicBool::new(false),
        )
        .unwrap();

        assert!(report.folders >= 2);
        assert!(!fixture.destination.join("2023").exists());
    }

    #[test]
    fn a_folder_somebody_else_left_something_in_survives() {
        let fixture = fixture(&["a.jpg"]);
        copied(&fixture);
        let landing = fixture.destination.join("2023").join("05");
        fs::write(landing.join("notes.txt"), "mine").unwrap();

        execute(
            &fixture.plan,
            &UndoOptions::default(),
            &AtomicBool::new(false),
        )
        .unwrap();

        assert!(landing.is_dir());
        assert!(landing.join("notes.txt").is_file());
    }

    #[test]
    fn the_destination_root_itself_is_never_removed() {
        let fixture = fixture(&["a.jpg"]);
        copied(&fixture);
        execute(
            &fixture.plan,
            &UndoOptions::default(),
            &AtomicBool::new(false),
        )
        .unwrap();

        assert!(fixture.destination.is_dir());
    }

    #[test]
    fn the_journal_is_cleared_so_a_second_undo_finds_nothing() {
        let fixture = fixture(&["a.jpg"]);
        copied(&fixture);

        let first = execute(
            &fixture.plan,
            &UndoOptions::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(first.removed, 1);

        let second = execute(
            &fixture.plan,
            &UndoOptions::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(second.removed, 0);
    }

    #[test]
    fn a_failed_copy_leaves_nothing_to_take_back() {
        let fixture = fixture(&["a.jpg"]);
        journal_line(
            &fixture,
            &JournalRecord {
                source: fixture.source.join("a.jpg"),
                outcome: Outcome::Failed {
                    error: "disk full".to_string(),
                },
                written: None,
            },
        );

        assert!(survey(&fixture.plan).unwrap().is_empty());
    }

    #[test]
    fn a_turned_copy_is_checked_against_the_hash_the_copy_recorded() {
        let fixture = fixture(&["a.jpg"]);
        let landed = fixture.destination.join("2023").join("05").join("a.jpg");
        fs::create_dir_all(landed.parent().unwrap()).unwrap();
        fs::write(&landed, "turned bytes").unwrap();

        journal_line(
            &fixture,
            &JournalRecord {
                source: fixture.source.join("a.jpg"),
                outcome: Outcome::Copied {
                    destination: landed.clone(),
                },
                written: Some(Written {
                    size: fs::metadata(&landed).unwrap().len(),
                    hash: hash_of(&landed).unwrap(),
                }),
            },
        );

        let before = survey(&fixture.plan).unwrap();
        assert_eq!(before[0].verdict, Verdict::Removable);

        fs::write(&landed, "turned bytes, then meddled with").unwrap();
        let after = survey(&fixture.plan).unwrap();
        assert_eq!(after[0].verdict, Verdict::Changed);
    }

    #[test]
    fn nothing_at_all_to_undo_is_not_an_error() {
        let fixture = fixture(&["a.jpg"]);
        let report = execute(
            &fixture.plan,
            &UndoOptions::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(report, UndoReport::default());
    }

    #[test]
    fn a_cancelled_undo_stops_where_it_was_asked_to() {
        let fixture = fixture(&["a.jpg"]);
        copied(&fixture);

        let cancelled = AtomicBool::new(true);
        assert!(matches!(
            execute(&fixture.plan, &UndoOptions::default(), &cancelled),
            Err(Error::Cancelled)
        ));
        assert!(fixture.destination.join("2023/05/a.jpg").is_file());
    }

    #[test]
    fn taking_a_copy_back_takes_its_sidecar_with_it() {
        let fixture = fixture(&["a.jpg"]);
        let options = crate::copy::CopyOptions {
            write_sidecars: true,
            ..crate::copy::CopyOptions::default()
        };
        crate::copy::execute(&fixture.plan, &options, &AtomicBool::new(false), &|_| {}).unwrap();

        let landed = fixture.destination.join("2023").join("05").join("a.jpg");
        let sidecar = crate::xmp_write::sidecar_path(&landed);
        assert!(sidecar.is_file());

        let report = execute(
            &fixture.plan,
            &UndoOptions::default(),
            &AtomicBool::new(false),
        )
        .unwrap();

        assert_eq!(report.removed, 1);
        assert_eq!(report.sidecars, 1);
        assert!(!sidecar.exists());
        assert!(!landed.exists());
    }

    #[test]
    fn a_copy_without_a_sidecar_is_taken_back_just_the_same() {
        let fixture = fixture(&["a.jpg"]);
        copied(&fixture);

        let report = execute(
            &fixture.plan,
            &UndoOptions::default(),
            &AtomicBool::new(false),
        )
        .unwrap();

        assert_eq!(report.removed, 1);
        assert_eq!(report.sidecars, 0);
    }

    #[test]
    fn a_sidecar_beside_a_copy_we_are_leaving_alone_stays() {
        let fixture = fixture(&["a.jpg"]);
        copied(&fixture);
        let landed = fixture.destination.join("2023").join("05").join("a.jpg");
        std::fs::write(&landed, "edited since the copy").unwrap();
        let sidecar = crate::xmp_write::sidecar_path(&landed);
        std::fs::write(&sidecar, "<x/>").unwrap();

        let report = execute(
            &fixture.plan,
            &UndoOptions::default(),
            &AtomicBool::new(false),
        )
        .unwrap();

        assert_eq!(report.changed, 1);
        assert_eq!(report.sidecars, 0);
        assert!(sidecar.is_file());
    }
}
