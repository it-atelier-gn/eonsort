use crate::ai::{cosine, Client};
use crate::error::{Error, Result};
use crate::model::PlanEntry;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 8] = b"EONSRCH1";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Index {
    pub vectors: HashMap<PathBuf, Vec<f32>>,
}

impl Index {
    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    pub fn rank(&self, query: &[f32], limit: usize) -> Vec<(PathBuf, f32)> {
        let mut scored: Vec<(PathBuf, f32)> = self
            .vectors
            .iter()
            .map(|(path, vector)| (path.clone(), cosine(query, vector)))
            .filter(|(_, score)| *score > 0.0)
            .collect();

        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        scored.truncate(limit);
        scored
    }
}

pub fn index_path(plan_path: &Path) -> PathBuf {
    plan_path.with_extension("search.bin")
}

pub fn describe(entry: &PlanEntry) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(caption) = &entry.caption {
        parts.push(caption.clone());
    }
    if let Some(subject) = &entry.subject {
        parts.push(subject.clone());
    }
    if !entry.tags.is_empty() {
        parts.push(entry.tags.join(", "));
    }
    if let Some(name) = entry.source.file_name().and_then(|n| n.to_str()) {
        parts.push(name.replace(['_', '-', '.'], " "));
    }
    parts.push(entry.taken.format("%B %Y").to_string());

    parts.join(". ")
}

pub fn build(
    client: &Client,
    entries: &[PlanEntry],
    cancel: &std::sync::atomic::AtomicBool,
    on_progress: &dyn Fn(usize, usize),
) -> Result<Index> {
    let mut index = Index::default();
    let total = entries.len();

    for (done, entry) in entries.iter().enumerate() {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let vector = client.embed(&describe(entry))?;
        if !vector.is_empty() {
            index.vectors.insert(entry.source.clone(), vector);
        }
        on_progress(done + 1, total);
    }

    Ok(index)
}

pub fn write(path: &Path, index: &Index) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
    }

    let file = std::fs::File::create(path).map_err(|e| Error::io(path, e))?;
    let mut out = std::io::BufWriter::new(file);
    let io = |e: std::io::Error| Error::io(path, e);

    out.write_all(MAGIC).map_err(io)?;
    out.write_all(&(index.vectors.len() as u64).to_le_bytes())
        .map_err(io)?;

    let mut ordered: Vec<(&PathBuf, &Vec<f32>)> = index.vectors.iter().collect();
    ordered.sort_by(|a, b| a.0.cmp(b.0));

    for (source, vector) in ordered {
        let raw = source.to_string_lossy();
        let bytes = raw.as_bytes();
        out.write_all(&(bytes.len() as u32).to_le_bytes())
            .map_err(io)?;
        out.write_all(bytes).map_err(io)?;
        out.write_all(&(vector.len() as u32).to_le_bytes())
            .map_err(io)?;
        for value in vector {
            out.write_all(&value.to_le_bytes()).map_err(io)?;
        }
    }

    out.flush().map_err(io)
}

pub fn read(path: &Path) -> Result<Index> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Index::default()),
        Err(e) => return Err(Error::io(path, e)),
    };
    let mut input = std::io::BufReader::new(file);
    let io = |e: std::io::Error| Error::io(path, e);

    let mut magic = [0u8; 8];
    input.read_exact(&mut magic).map_err(io)?;
    if &magic != MAGIC {
        return Err(Error::Ai(format!(
            "{} is not a search index this version understands",
            path.display()
        )));
    }

    let mut count = [0u8; 8];
    input.read_exact(&mut count).map_err(io)?;
    let count = u64::from_le_bytes(count) as usize;

    let mut index = Index::default();
    for _ in 0..count {
        let mut length = [0u8; 4];
        input.read_exact(&mut length).map_err(io)?;
        let mut raw = vec![0u8; u32::from_le_bytes(length) as usize];
        input.read_exact(&mut raw).map_err(io)?;
        let source = PathBuf::from(String::from_utf8_lossy(&raw).into_owned());

        input.read_exact(&mut length).map_err(io)?;
        let dimensions = u32::from_le_bytes(length) as usize;
        let mut vector = Vec::with_capacity(dimensions);
        let mut value = [0u8; 4];
        for _ in 0..dimensions {
            input.read_exact(&mut value).map_err(io)?;
            vector.push(f32::from_le_bytes(value));
        }
        index.vectors.insert(source, vector);
    }

    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use tempfile::tempdir;

    fn entry(name: &str) -> PlanEntry {
        PlanEntry {
            source: PathBuf::from(format!("/src/{name}")),
            taken: NaiveDate::from_ymd_opt(2019, 7, 4)
                .unwrap()
                .and_hms_opt(10, 0, 0)
                .unwrap(),
            ..PlanEntry::default()
        }
    }

    #[test]
    fn describes_a_file_the_model_has_looked_at() {
        let mut item = entry("IMG_0001.jpg");
        item.caption = Some("A dog on a beach.".into());
        item.subject = Some("dog".into());
        item.tags = vec!["beach".into(), "sand".into()];

        let text = describe(&item);
        assert!(text.contains("A dog on a beach."));
        assert!(text.contains("beach, sand"));
        assert!(text.contains("July 2019"));
    }

    #[test]
    fn still_describes_a_file_the_model_never_saw() {
        let text = describe(&entry("summer_holiday_croatia.jpg"));
        assert!(text.contains("summer holiday croatia"));
        assert!(text.contains("July 2019"));
    }

    #[test]
    fn ranks_by_similarity_and_drops_non_matches() {
        let mut index = Index::default();
        index.vectors.insert(PathBuf::from("/a"), vec![1.0, 0.0]);
        index.vectors.insert(PathBuf::from("/b"), vec![0.7, 0.7]);
        index.vectors.insert(PathBuf::from("/c"), vec![-1.0, 0.0]);

        let hits = index.rank(&[1.0, 0.0], 10);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].0, PathBuf::from("/a"));
        assert_eq!(hits[1].0, PathBuf::from("/b"));
    }

    #[test]
    fn honours_the_result_limit() {
        let mut index = Index::default();
        for i in 0..10 {
            index
                .vectors
                .insert(PathBuf::from(format!("/{i}")), vec![1.0, 0.0]);
        }
        assert_eq!(index.rank(&[1.0, 0.0], 3).len(), 3);
    }

    #[test]
    fn survives_a_round_trip_to_disk() {
        let dir = tempdir().unwrap();
        let path = index_path(&dir.path().join("plan.jsonl"));
        assert!(path.ends_with("plan.search.bin"));

        let mut index = Index::default();
        index
            .vectors
            .insert(PathBuf::from("/src/a b.jpg"), vec![0.5, -0.25, 1.0]);
        index.vectors.insert(PathBuf::from("/src/ü.jpg"), vec![1.0]);

        write(&path, &index).unwrap();
        assert_eq!(read(&path).unwrap(), index);
    }

    #[test]
    fn a_missing_index_reads_as_empty() {
        let dir = tempdir().unwrap();
        let index = read(&index_path(&dir.path().join("plan.jsonl"))).unwrap();
        assert!(index.is_empty());
    }

    #[test]
    fn refuses_a_file_that_is_not_an_index() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("plan.search.bin");
        std::fs::write(&path, b"not an index at all").unwrap();
        assert!(read(&path).is_err());
    }
}
