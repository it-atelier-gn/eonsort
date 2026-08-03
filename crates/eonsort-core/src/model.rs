use crate::error::{Error, Result};
use crate::providers::{DetectOptions, Provider};
use chrono::format::StrftimeItems;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PLAN_VERSION: u32 = 1;
pub const DEFAULT_FOLDER_PATTERN: &str = "%Y/%m";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanHeader {
    pub version: u32,
    pub created_at: NaiveDateTime,
    pub sources: Vec<PathBuf>,
    pub destination: PathBuf,
    pub folder_pattern: String,
    pub detect: DetectOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanEntry {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub taken: NaiveDateTime,
    pub provider: Provider,
    pub provider_info: Option<String>,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedEntry {
    pub source: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlanRecord {
    Header(PlanHeader),
    Entry(PlanEntry),
    Skipped(SkippedEntry),
}

pub fn validate_folder_pattern(pattern: &str) -> Result<()> {
    if pattern.trim().is_empty() {
        return Err(Error::InvalidFolderPattern(pattern.to_string()));
    }
    StrftimeItems::new(pattern)
        .parse()
        .map(|_| ())
        .map_err(|_| Error::InvalidFolderPattern(pattern.to_string()))
}

pub fn destination_for(
    source: &Path,
    taken: NaiveDateTime,
    destination_root: &Path,
    folder_pattern: &str,
) -> Result<PathBuf> {
    let items = StrftimeItems::new(folder_pattern)
        .parse()
        .map_err(|_| Error::InvalidFolderPattern(folder_pattern.to_string()))?;
    let folder = taken.format_with_items(items.as_slice().iter()).to_string();
    let name = source
        .file_name()
        .ok_or_else(|| Error::InvalidSourcePath(source.to_path_buf()))?;

    let mut path = destination_root.to_path_buf();
    for part in folder.split(['/', '\\']).filter(|p| !p.is_empty()) {
        path.push(part);
    }
    path.push(name);
    Ok(path)
}

/// Adds a `_dup_N` marker before the extension, matching the layout used for
/// same-name files whose contents differ.
pub fn duplicate_variant(destination: &Path, index: usize) -> PathBuf {
    let stem = destination
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = match destination.extension() {
        Some(ext) => format!("{stem}_dup_{index}.{}", ext.to_string_lossy()),
        None => format!("{stem}_dup_{index}"),
    };
    destination.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn taken() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2023, 5, 6)
            .unwrap()
            .and_hms_opt(1, 2, 3)
            .unwrap()
    }

    #[test]
    fn builds_year_month_destination() {
        let dest = destination_for(
            Path::new("/src/holiday.jpg"),
            taken(),
            Path::new("/out"),
            DEFAULT_FOLDER_PATTERN,
        )
        .unwrap();
        assert_eq!(
            dest,
            Path::new("/out")
                .join("2023")
                .join("05")
                .join("holiday.jpg")
        );
    }

    #[test]
    fn honours_a_custom_folder_pattern() {
        let dest = destination_for(
            Path::new("/src/holiday.jpg"),
            taken(),
            Path::new("/out"),
            "%Y/%Y-%m-%d",
        )
        .unwrap();
        assert_eq!(
            dest,
            Path::new("/out")
                .join("2023")
                .join("2023-05-06")
                .join("holiday.jpg")
        );
    }

    #[test]
    fn rejects_an_unparseable_folder_pattern() {
        assert!(validate_folder_pattern("%Q").is_err());
        assert!(validate_folder_pattern("").is_err());
        assert!(validate_folder_pattern("%Y/%m").is_ok());
    }

    #[test]
    fn duplicate_variant_keeps_the_extension() {
        assert_eq!(
            duplicate_variant(Path::new("/out/2023/05/a.jpg"), 2),
            PathBuf::from("/out/2023/05/a_dup_2.jpg")
        );
        assert_eq!(
            duplicate_variant(Path::new("/out/2023/05/README"), 1),
            PathBuf::from("/out/2023/05/README_dup_1")
        );
    }
}
