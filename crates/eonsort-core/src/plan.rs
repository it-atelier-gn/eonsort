use crate::error::{Error, Result};
use crate::model::{PlanEntry, PlanHeader, PlanRecord, SkippedEntry, PLAN_VERSION};
use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

const FLUSH_EVERY: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub header: PlanHeader,
    pub entries: Vec<PlanEntry>,
    pub skipped: Vec<SkippedEntry>,
}

impl Plan {
    pub fn total_bytes(&self) -> u64 {
        self.entries.iter().map(|e| e.size).sum()
    }

    pub fn analysed_sources(&self) -> HashSet<PathBuf> {
        self.entries
            .iter()
            .map(|e| e.source.clone())
            .chain(self.skipped.iter().map(|s| s.source.clone()))
            .collect()
    }
}

pub fn default_plan_name(sources: &[PathBuf], destination: &Path) -> String {
    let mut hasher = blake3::Hasher::new();
    for source in sources {
        hasher.update(source.to_string_lossy().as_bytes());
        hasher.update(b"\0");
    }
    hasher.update(b"->");
    hasher.update(destination.to_string_lossy().as_bytes());
    format!("plan-{}.jsonl", &hasher.finalize().to_hex()[..8])
}

pub fn read_plan(path: &Path) -> Result<Plan> {
    let file = File::open(path).map_err(|e| Error::io(path, e))?;
    let mut header = None;
    let mut entries = Vec::new();
    let mut skipped = Vec::new();

    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|e| Error::io(path, e))?;
        if line.trim().is_empty() {
            continue;
        }
        let record: PlanRecord = serde_json::from_str(&line).map_err(|e| Error::MalformedPlan {
            line: index + 1,
            message: e.to_string(),
        })?;
        match record {
            PlanRecord::Header(h) => {
                if h.version > PLAN_VERSION {
                    return Err(Error::UnsupportedPlanVersion(h.version));
                }
                header = Some(h);
            }
            PlanRecord::Entry(e) => entries.push(e),
            PlanRecord::Skipped(s) => skipped.push(s),
        }
    }

    Ok(Plan {
        header: header.ok_or(Error::MissingPlanHeader)?,
        entries,
        skipped,
    })
}

/// Append-only writer. Records land on disk as they are produced so an
/// interrupted scan can be resumed instead of restarted.
pub struct PlanWriter {
    path: PathBuf,
    file: BufWriter<File>,
    unflushed: usize,
}

impl PlanWriter {
    pub fn create(path: &Path, header: &PlanHeader) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
            }
        }
        let file = File::create(path).map_err(|e| Error::io(path, e))?;
        let mut writer = Self {
            path: path.to_path_buf(),
            file: BufWriter::new(file),
            unflushed: 0,
        };
        writer.write(&PlanRecord::Header(header.clone()))?;
        Ok(writer)
    }

    pub fn append(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .append(true)
            .open(path)
            .map_err(|e| Error::io(path, e))?;
        Ok(Self {
            path: path.to_path_buf(),
            file: BufWriter::new(file),
            unflushed: 0,
        })
    }

    pub fn write(&mut self, record: &PlanRecord) -> Result<()> {
        let line = serde_json::to_string(record)?;
        writeln!(self.file, "{line}").map_err(|e| Error::io(&self.path, e))?;
        self.unflushed += 1;
        if self.unflushed >= FLUSH_EVERY {
            self.flush()?;
        }
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        self.file.flush().map_err(|e| Error::io(&self.path, e))?;
        self.unflushed = 0;
        Ok(())
    }
}

impl Drop for PlanWriter {
    fn drop(&mut self) {
        let _ = self.file.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DEFAULT_FOLDER_PATTERN;
    use crate::providers::{DetectOptions, Provider};
    use chrono::NaiveDate;
    use tempfile::tempdir;

    fn header() -> PlanHeader {
        PlanHeader {
            version: PLAN_VERSION,
            created_at: NaiveDate::from_ymd_opt(2024, 1, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            sources: vec![PathBuf::from("/src")],
            destination: PathBuf::from("/out"),
            folder_pattern: DEFAULT_FOLDER_PATTERN.to_string(),
            detect: DetectOptions::default(),
        }
    }

    fn entry(name: &str) -> PlanEntry {
        PlanEntry {
            source: PathBuf::from(format!("/src/{name}")),
            destination: PathBuf::from(format!("/out/2023/05/{name}")),
            taken: NaiveDate::from_ymd_opt(2023, 5, 6)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            provider: Provider::Filename,
            provider_info: Some(name.to_string()),
            size: 10,
        }
    }

    #[test]
    fn round_trips_a_plan() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("plan.jsonl");

        let mut writer = PlanWriter::create(&path, &header()).unwrap();
        writer.write(&PlanRecord::Entry(entry("a.jpg"))).unwrap();
        writer
            .write(&PlanRecord::Skipped(SkippedEntry {
                source: PathBuf::from("/src/b.bin"),
                reason: "no date".into(),
            }))
            .unwrap();
        writer.flush().unwrap();
        drop(writer);

        let plan = read_plan(&path).unwrap();
        assert_eq!(plan.header, header());
        assert_eq!(plan.entries, vec![entry("a.jpg")]);
        assert_eq!(plan.skipped.len(), 1);
        assert_eq!(plan.total_bytes(), 10);
        assert_eq!(plan.analysed_sources().len(), 2);
    }

    #[test]
    fn appends_without_rewriting_the_header() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("plan.jsonl");

        let mut writer = PlanWriter::create(&path, &header()).unwrap();
        writer.write(&PlanRecord::Entry(entry("a.jpg"))).unwrap();
        drop(writer);

        let mut writer = PlanWriter::append(&path).unwrap();
        writer.write(&PlanRecord::Entry(entry("b.jpg"))).unwrap();
        drop(writer);

        let plan = read_plan(&path).unwrap();
        assert_eq!(plan.entries.len(), 2);
    }

    #[test]
    fn reports_the_line_of_a_malformed_record() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("plan.jsonl");
        std::fs::write(
            &path,
            format!(
                "{}\n{{ nope\n",
                serde_json::to_string(&PlanRecord::Header(header())).unwrap()
            ),
        )
        .unwrap();

        match read_plan(&path) {
            Err(Error::MalformedPlan { line, .. }) => assert_eq!(line, 2),
            other => panic!("expected a malformed-plan error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_a_plan_without_a_header() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("plan.jsonl");
        std::fs::write(
            &path,
            serde_json::to_string(&PlanRecord::Entry(entry("a.jpg"))).unwrap(),
        )
        .unwrap();

        assert!(matches!(read_plan(&path), Err(Error::MissingPlanHeader)));
    }

    #[test]
    fn plan_names_are_stable_per_source_and_destination() {
        let sources = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        let dest = Path::new("/out");
        assert_eq!(
            default_plan_name(&sources, dest),
            default_plan_name(&sources, dest)
        );
        assert_ne!(
            default_plan_name(&sources, dest),
            default_plan_name(&sources, Path::new("/other"))
        );
    }
}
