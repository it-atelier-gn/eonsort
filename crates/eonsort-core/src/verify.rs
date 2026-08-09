use crate::error::{Error, Result};
use crate::model::{duplicate_variant, PlanEntry};
use crate::rotate::Written;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

const MAX_DUPLICATE_PROBES: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct VerifyOptions {
    pub compare_hashes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerifyProgress {
    pub checked: u64,
    pub total: u64,
    pub current: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueKind {
    SourceMissing,
    DestinationMissing,
    ContentMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyIssue {
    pub kind: IssueKind,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub source_size: u64,
    pub destination_size: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct VerifyReport {
    pub ok: u64,
    pub source_missing: u64,
    pub destination_missing: u64,
    pub content_mismatch: u64,
    pub source_bytes: u64,
    pub destination_bytes: u64,
    pub duplicate_files: u64,
    pub duplicate_bytes: u64,
    pub issues: Vec<VerifyIssue>,
}

pub fn verify(
    plan_path: &Path,
    options: &VerifyOptions,
    cancel: &AtomicBool,
    on_progress: &dyn Fn(VerifyProgress),
) -> Result<VerifyReport> {
    let plan = crate::overrides::load_plan(plan_path)?;
    if plan.header.destination.is_none() {
        return Err(Error::NoDestination);
    }
    let written = crate::copy::read_written(&crate::copy::journal_path(plan_path))?;
    let total = plan.entries.len() as u64;
    let mut report = VerifyReport::default();
    let mut counted = HashSet::new();

    for (index, entry) in plan.entries.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let expected = if entry.rotate.is_identity() {
            Expected::Source
        } else {
            match written.get(&entry.source) {
                Some(record) => Expected::Turned(record.clone()),
                None => Expected::Unknown,
            }
        };
        check(entry, options, &expected, &mut report, &mut counted);
        on_progress(VerifyProgress {
            checked: index as u64 + 1,
            total,
            current: Some(entry.source.clone()),
        });
    }

    Ok(report)
}

enum Expected {
    Source,
    Turned(Written),
    Unknown,
}

fn check(
    entry: &PlanEntry,
    options: &VerifyOptions,
    expected: &Expected,
    report: &mut VerifyReport,
    counted: &mut HashSet<PathBuf>,
) {
    let Ok(source_meta) = fs::metadata(&entry.source) else {
        report.source_missing += 1;
        report.issues.push(VerifyIssue {
            kind: IssueKind::SourceMissing,
            source: entry.source.clone(),
            destination: entry.destination.clone(),
            source_size: entry.size,
            destination_size: None,
        });
        return;
    };

    let source_size = source_meta.len();
    report.source_bytes += source_size;

    let mut matched = false;
    let mut any_candidate = false;

    for index in 0..MAX_DUPLICATE_PROBES {
        let candidate = if index == 0 {
            entry.destination.clone()
        } else {
            duplicate_variant(&entry.destination, index)
        };
        let Ok(meta) = fs::metadata(&candidate) else {
            if index > 0 {
                break;
            }
            continue;
        };

        any_candidate = true;
        if counted.insert(candidate.clone()) {
            report.destination_bytes += meta.len();
            if index > 0 {
                report.duplicate_files += 1;
                report.duplicate_bytes += meta.len();
            }
        }

        let size_matches = match expected {
            Expected::Source => meta.len() == source_size,
            Expected::Turned(record) => meta.len() == record.size,
            Expected::Unknown => true,
        };
        if !matched && size_matches && same_content(entry, &candidate, options, expected) {
            matched = true;
        }
    }

    if matched {
        report.ok += 1;
    } else if any_candidate {
        report.content_mismatch += 1;
        report.issues.push(VerifyIssue {
            kind: IssueKind::ContentMismatch,
            source: entry.source.clone(),
            destination: entry.destination.clone(),
            source_size,
            destination_size: fs::metadata(&entry.destination).ok().map(|m| m.len()),
        });
    } else {
        report.destination_missing += 1;
        report.issues.push(VerifyIssue {
            kind: IssueKind::DestinationMissing,
            source: entry.source.clone(),
            destination: entry.destination.clone(),
            source_size,
            destination_size: None,
        });
    }
}

fn same_content(
    entry: &PlanEntry,
    candidate: &Path,
    options: &VerifyOptions,
    expected: &Expected,
) -> bool {
    if !options.compare_hashes {
        return true;
    }
    match expected {
        Expected::Unknown => true,
        Expected::Turned(record) => hash(candidate).is_some_and(|found| found == record.hash),
        Expected::Source => match (hash(&entry.source), hash(candidate)) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        },
    }
}

fn hash(path: &Path) -> Option<String> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(fs::File::open(path).ok()?).ok()?;
    Some(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PlanHeader, PlanRecord, DEFAULT_FOLDER_PATTERN, PLAN_VERSION};
    use crate::plan::PlanWriter;
    use crate::providers::{DetectOptions, Provider};
    use chrono::NaiveDate;
    use tempfile::{tempdir, TempDir};

    fn plan_with(entries: Vec<PlanEntry>, dir: &TempDir) -> PathBuf {
        let path = dir.path().join("plan.jsonl");
        let header = PlanHeader {
            version: PLAN_VERSION,
            created_at: NaiveDate::from_ymd_opt(2024, 1, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            sources: vec![dir.path().join("src")],
            destination: Some(dir.path().join("out")),
            folder_pattern: DEFAULT_FOLDER_PATTERN.to_string(),
            detect: DetectOptions::default(),
        };
        let mut writer = PlanWriter::create(&path, &header).unwrap();
        for entry in entries {
            writer.write(&PlanRecord::Entry(entry)).unwrap();
        }
        drop(writer);
        path
    }

    fn entry(source: PathBuf, destination: PathBuf, size: u64) -> PlanEntry {
        PlanEntry {
            source,
            destination,
            taken: NaiveDate::from_ymd_opt(2023, 5, 6)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            provider: Provider::Filename,
            provider_info: None,
            size,
            ..PlanEntry::default()
        }
    }

    fn turned_entry(source: PathBuf, destination: PathBuf, size: u64) -> PlanEntry {
        PlanEntry {
            rotate: crate::rotate::Transform::Rotate90,
            ..entry(source, destination, size)
        }
    }

    fn journal_a_turn(plan: &Path, source: &Path, destination: &Path, body: &[u8]) {
        let record = crate::copy::JournalRecord {
            source: source.to_path_buf(),
            outcome: crate::copy::Outcome::Copied {
                destination: destination.to_path_buf(),
            },
            written: Some(Written {
                size: body.len() as u64,
                hash: blake3::hash(body).to_hex().to_string(),
            }),
        };
        fs::write(
            crate::copy::journal_path(plan),
            format!("{}\n", serde_json::to_string(&record).unwrap()),
        )
        .unwrap();
    }

    #[test]
    fn a_turned_copy_is_checked_against_what_the_copy_wrote() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("a.jpg");
        let dest = dir.path().join("out/2023/05/a.jpg");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&src, b"the original bytes").unwrap();
        fs::write(&dest, b"turned").unwrap();

        let path = plan_with(vec![turned_entry(src.clone(), dest.clone(), 18)], &dir);
        journal_a_turn(&path, &src, &dest, b"turned");

        let report = verify(
            &path,
            &VerifyOptions {
                compare_hashes: true,
            },
            &AtomicBool::new(false),
            &|_| {},
        )
        .unwrap();

        assert_eq!(report.ok, 1);
        assert!(report.issues.is_empty());
    }

    #[test]
    fn a_turned_copy_that_was_tampered_with_is_reported() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("a.jpg");
        let dest = dir.path().join("out/2023/05/a.jpg");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&src, b"the original bytes").unwrap();
        fs::write(&dest, b"meddled").unwrap();

        let path = plan_with(vec![turned_entry(src.clone(), dest.clone(), 18)], &dir);
        journal_a_turn(&path, &src, &dest, b"turned");

        let report = verify(
            &path,
            &VerifyOptions {
                compare_hashes: true,
            },
            &AtomicBool::new(false),
            &|_| {},
        )
        .unwrap();

        assert_eq!(report.content_mismatch, 1);
        assert_eq!(report.issues[0].kind, IssueKind::ContentMismatch);
    }

    #[test]
    fn a_turned_copy_with_no_journal_left_is_only_checked_for_being_there() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("a.jpg");
        let dest = dir.path().join("out/2023/05/a.jpg");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&src, b"the original bytes").unwrap();
        fs::write(&dest, b"turned").unwrap();

        let path = plan_with(vec![turned_entry(src, dest, 18)], &dir);

        let report = verify(
            &path,
            &VerifyOptions {
                compare_hashes: true,
            },
            &AtomicBool::new(false),
            &|_| {},
        )
        .unwrap();

        assert_eq!(report.ok, 1);
        assert!(report.issues.is_empty());
    }

    #[test]
    fn counts_a_correctly_copied_file_as_ok() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("a.bin");
        let dest = dir.path().join("out/2023/05/a.bin");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&src, b"hello").unwrap();
        fs::write(&dest, b"hello").unwrap();

        let path = plan_with(vec![entry(src, dest, 5)], &dir);
        let report = verify(
            &path,
            &VerifyOptions {
                compare_hashes: true,
            },
            &AtomicBool::new(false),
            &|_| {},
        )
        .unwrap();

        assert_eq!(report.ok, 1);
        assert!(report.issues.is_empty());
        assert_eq!(report.source_bytes, 5);
        assert_eq!(report.destination_bytes, 5);
    }

    #[test]
    fn reports_a_missing_destination() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("a.bin");
        fs::write(&src, b"hello").unwrap();

        let path = plan_with(
            vec![entry(src, dir.path().join("out/2023/05/a.bin"), 5)],
            &dir,
        );
        let report = verify(
            &path,
            &VerifyOptions::default(),
            &AtomicBool::new(false),
            &|_| {},
        )
        .unwrap();

        assert_eq!(report.destination_missing, 1);
        assert_eq!(report.issues[0].kind, IssueKind::DestinationMissing);
    }

    #[test]
    fn accepts_a_match_found_under_a_duplicate_name() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("a.bin");
        let dest = dir.path().join("out/2023/05/a.bin");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&src, b"hello").unwrap();
        fs::write(&dest, b"other").unwrap();
        fs::write(dir.path().join("out/2023/05/a_dup_1.bin"), b"hello").unwrap();

        let path = plan_with(vec![entry(src, dest, 5)], &dir);
        let report = verify(
            &path,
            &VerifyOptions {
                compare_hashes: true,
            },
            &AtomicBool::new(false),
            &|_| {},
        )
        .unwrap();

        assert_eq!(report.ok, 1);
        assert_eq!(report.duplicate_files, 1);
        assert_eq!(report.destination_bytes, 10);
    }

    #[test]
    fn counts_a_shared_destination_only_once() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out/2023/05/a.bin");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&dest, b"hello").unwrap();

        let mut entries = Vec::new();
        for name in ["one.bin", "two.bin"] {
            let src = dir.path().join(name);
            fs::write(&src, b"hello").unwrap();
            entries.push(entry(src, dest.clone(), 5));
        }

        let path = plan_with(entries, &dir);
        let report = verify(
            &path,
            &VerifyOptions {
                compare_hashes: true,
            },
            &AtomicBool::new(false),
            &|_| {},
        )
        .unwrap();

        assert_eq!(report.ok, 2);
        assert_eq!(report.source_bytes, 10);
        assert_eq!(report.destination_bytes, 5);
    }

    #[test]
    fn reports_a_content_mismatch_when_hashes_differ() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("a.bin");
        let dest = dir.path().join("out/2023/05/a.bin");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&src, b"hello").unwrap();
        fs::write(&dest, b"world").unwrap();

        let path = plan_with(vec![entry(src, dest, 5)], &dir);
        let report = verify(
            &path,
            &VerifyOptions {
                compare_hashes: true,
            },
            &AtomicBool::new(false),
            &|_| {},
        )
        .unwrap();

        assert_eq!(report.content_mismatch, 1);
        assert_eq!(report.issues[0].kind, IssueKind::ContentMismatch);
    }

    #[test]
    fn reports_a_vanished_source() {
        let dir = tempdir().unwrap();
        let path = plan_with(
            vec![entry(
                dir.path().join("gone.bin"),
                dir.path().join("out/2023/05/gone.bin"),
                5,
            )],
            &dir,
        );
        let report = verify(
            &path,
            &VerifyOptions::default(),
            &AtomicBool::new(false),
            &|_| {},
        )
        .unwrap();

        assert_eq!(report.source_missing, 1);
        assert_eq!(report.issues[0].kind, IssueKind::SourceMissing);
    }
}
