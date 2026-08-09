use crate::error::{Error, Result};
use crate::providers::{DetectOptions, Detection, Provider};
use crate::rotate::Transform;
use crate::suspect::Flag;
use chrono::format::StrftimeItems;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PLAN_VERSION: u32 = 4;
pub const SUBJECT_TOKEN: &str = "{subject}";
pub const UNKNOWN_SUBJECT: &str = "unsorted";
pub const DEFAULT_FOLDER_PATTERN: &str = "%Y/%m";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanHeader {
    pub version: u32,
    pub created_at: NaiveDateTime,
    pub sources: Vec<PathBuf>,
    #[serde(default)]
    pub destination: Option<PathBuf>,
    pub folder_pattern: String,
    pub detect: DetectOptions,
}

impl PlanHeader {
    pub fn root(&self) -> &Path {
        destination_root(self.destination.as_deref())
    }
}

pub fn destination_root(destination: Option<&Path>) -> &Path {
    destination.unwrap_or(Path::new(""))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanEntry {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub taken: NaiveDateTime,
    pub provider: Provider,
    pub provider_info: Option<String>,
    pub size: u64,
    #[serde(default)]
    pub candidates: Vec<Detection>,
    #[serde(default)]
    pub flags: Vec<Flag>,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub caption: Option<String>,
    #[serde(default)]
    pub orientation: u16,
    #[serde(default)]
    pub rotate: Transform,
    #[serde(default)]
    pub reencode: bool,
}

impl Default for PlanEntry {
    fn default() -> Self {
        Self {
            source: PathBuf::new(),
            destination: PathBuf::new(),
            taken: NaiveDateTime::default(),
            provider: Provider::Filesystem,
            provider_info: None,
            size: 0,
            candidates: Vec::new(),
            flags: Vec::new(),
            subject: None,
            tags: Vec::new(),
            caption: None,
            orientation: 0,
            rotate: Transform::None,
            reencode: false,
        }
    }
}

impl PlanEntry {
    pub fn candidate(&self, provider: Provider) -> Option<&Detection> {
        self.candidates.iter().find(|c| c.provider == provider)
    }

    pub fn filesystem_time(&self) -> Option<NaiveDateTime> {
        self.candidate(Provider::Filesystem).map(|c| c.taken)
    }
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
    StrftimeItems::new(&pattern.replace(SUBJECT_TOKEN, UNKNOWN_SUBJECT))
        .parse()
        .map(|_| ())
        .map_err(|_| Error::InvalidFolderPattern(pattern.to_string()))
}

pub fn pattern_needs_subject(pattern: &str) -> bool {
    pattern.contains(SUBJECT_TOKEN)
}

pub fn folder_segment(subject: &str) -> String {
    let cleaned: String = subject
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                ' '
            }
        })
        .collect();

    let joined = cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .trim_matches(['-', '.'])
        .to_string();

    if joined.is_empty() {
        UNKNOWN_SUBJECT.to_string()
    } else {
        joined.chars().take(48).collect()
    }
}

pub fn destination_for(
    source: &Path,
    taken: NaiveDateTime,
    destination_root: &Path,
    folder_pattern: &str,
) -> Result<PathBuf> {
    destination_with_subject(source, taken, None, destination_root, folder_pattern)
}

pub fn destination_with_subject(
    source: &Path,
    taken: NaiveDateTime,
    subject: Option<&str>,
    destination_root: &Path,
    folder_pattern: &str,
) -> Result<PathBuf> {
    let resolved = folder_pattern.replace(
        SUBJECT_TOKEN,
        &subject.map(folder_segment).unwrap_or_else(|| {
            if pattern_needs_subject(folder_pattern) {
                UNKNOWN_SUBJECT.to_string()
            } else {
                String::new()
            }
        }),
    );

    let items = StrftimeItems::new(&resolved)
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
    fn puts_the_subject_into_the_folder_when_the_pattern_asks_for_it() {
        let dest = destination_with_subject(
            Path::new("/src/holiday.jpg"),
            taken(),
            Some("Beach Sunset"),
            Path::new("/out"),
            "%Y/{subject}",
        )
        .unwrap();
        assert_eq!(
            dest,
            Path::new("/out")
                .join("2023")
                .join("beach-sunset")
                .join("holiday.jpg")
        );
    }

    #[test]
    fn falls_back_to_a_named_folder_when_the_model_had_no_subject() {
        let dest = destination_with_subject(
            Path::new("/src/holiday.jpg"),
            taken(),
            None,
            Path::new("/out"),
            "%Y/{subject}",
        )
        .unwrap();
        assert!(dest.ends_with("2023/unsorted/holiday.jpg"));
    }

    #[test]
    fn a_subject_never_escapes_its_folder() {
        for hostile in [
            "../../etc",
            "C:\\Windows",
            "a/b",
            "...",
            "  ",
            "!!!",
            "con:/nul",
        ] {
            let segment = folder_segment(hostile);
            assert!(
                !segment.contains(['/', '\\', ':']),
                "{hostile} -> {segment}"
            );
            assert!(!segment.is_empty(), "{hostile}");
            assert_ne!(segment, "..", "{hostile}");
            assert!(!segment.starts_with('.'), "{hostile} -> {segment}");
        }
    }

    #[test]
    fn subject_segments_are_lowercased_hyphenated_and_bounded() {
        assert_eq!(folder_segment("Beach Sunset"), "beach-sunset");
        assert_eq!(folder_segment("  spaced   out  "), "spaced-out");
        assert_eq!(folder_segment("Grandma's 80th!"), "grandma-s-80th");
        assert_eq!(folder_segment(&"x".repeat(200)).len(), 48);
    }

    #[test]
    fn a_pattern_without_the_token_ignores_the_subject_entirely() {
        let with = destination_with_subject(
            Path::new("/src/a.jpg"),
            taken(),
            Some("beach"),
            Path::new("/out"),
            DEFAULT_FOLDER_PATTERN,
        )
        .unwrap();
        let without = destination_for(
            Path::new("/src/a.jpg"),
            taken(),
            Path::new("/out"),
            DEFAULT_FOLDER_PATTERN,
        )
        .unwrap();
        assert_eq!(with, without);
        assert!(!pattern_needs_subject(DEFAULT_FOLDER_PATTERN));
        assert!(pattern_needs_subject("%Y/{subject}"));
    }

    #[test]
    fn validates_a_pattern_that_contains_the_subject_token() {
        assert!(validate_folder_pattern("%Y/{subject}").is_ok());
        assert!(validate_folder_pattern("{subject}").is_ok());
        assert!(validate_folder_pattern("%Q/{subject}").is_err());
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
