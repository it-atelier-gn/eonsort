use crate::error::{Error, Result};
use crate::plan::Plan;
use crate::providers::Provider;
use crate::rotate::Transform;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "origin", rename_all = "snake_case")]
pub enum OverrideOrigin {
    Candidate { provider: Provider },
    Manual,
    Shift { seconds: i64 },
}

impl OverrideOrigin {
    pub fn describe(&self) -> String {
        match self {
            OverrideOrigin::Candidate { provider } => format!("taken from {}", provider.label()),
            OverrideOrigin::Manual => "set by hand".to_string(),
            OverrideOrigin::Shift { seconds } => {
                format!("shifted by {}", humanise_shift(*seconds))
            }
        }
    }
}

fn humanise_shift(seconds: i64) -> String {
    let sign = if seconds < 0 { "-" } else { "+" };
    let total = seconds.unsigned_abs();
    let days = total / 86_400;
    let hours = (total % 86_400) / 3_600;
    let minutes = (total % 3_600) / 60;
    format!("{sign}{days}d {hours}h {minutes}m")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateOverride {
    pub taken: NaiveDateTime,
    #[serde(flatten)]
    pub origin: OverrideOrigin,
    pub at: NaiveDateTime,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Overrides(pub HashMap<PathBuf, DateOverride>);

impl Overrides {
    pub fn get(&self, source: &Path) -> Option<&DateOverride> {
        self.0.get(source)
    }

    pub fn set(&mut self, source: PathBuf, value: DateOverride) {
        self.0.insert(source, value);
    }

    pub fn clear(&mut self, source: &Path) -> bool {
        self.0.remove(source).is_some()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub fn overrides_path(plan_path: &Path) -> PathBuf {
    plan_path.with_extension("overrides.json")
}

pub fn read(path: &Path) -> Result<Overrides> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Overrides::default()),
        Err(e) => return Err(Error::io(path, e)),
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    if text.trim().is_empty() {
        return Ok(Overrides::default());
    }
    serde_json::from_str(text).map_err(Error::from)
}

pub fn write(path: &Path, overrides: &Overrides) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
    }
    let file = File::create(path).map_err(|e| Error::io(path, e))?;
    serde_json::to_writer_pretty(BufWriter::new(file), overrides).map_err(Error::from)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RotationOverride {
    pub transform: Transform,
    #[serde(default)]
    pub reencode: bool,
    pub at: NaiveDateTime,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Rotations(pub HashMap<PathBuf, RotationOverride>);

impl Rotations {
    pub fn get(&self, source: &Path) -> Option<&RotationOverride> {
        self.0.get(source)
    }

    pub fn set(&mut self, source: PathBuf, value: RotationOverride) {
        self.0.insert(source, value);
    }

    pub fn clear(&mut self, source: &Path) -> bool {
        self.0.remove(source).is_some()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub fn rotations_path(plan_path: &Path) -> PathBuf {
    plan_path.with_extension("rotations.json")
}

pub fn read_rotations(path: &Path) -> Result<Rotations> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Rotations::default()),
        Err(e) => return Err(Error::io(path, e)),
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    if text.trim().is_empty() {
        return Ok(Rotations::default());
    }
    serde_json::from_str(text).map_err(Error::from)
}

pub fn write_rotations(path: &Path, rotations: &Rotations) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
    }
    let file = File::create(path).map_err(|e| Error::io(path, e))?;
    serde_json::to_writer_pretty(BufWriter::new(file), rotations).map_err(Error::from)
}

pub fn apply_rotations(plan: &mut Plan, rotations: &Rotations) {
    if rotations.is_empty() {
        return;
    }
    for entry in &mut plan.entries {
        if let Some(applied) = rotations.get(&entry.source) {
            entry.rotate = applied.transform;
            entry.reencode = applied.reencode;
        }
    }
}

pub fn excluded_path(plan_path: &Path) -> PathBuf {
    plan_path.with_extension("excluded.json")
}

pub fn read_excluded(path: &Path) -> Result<std::collections::HashSet<PathBuf>> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Default::default()),
        Err(e) => return Err(Error::io(path, e)),
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    if text.trim().is_empty() {
        return Ok(Default::default());
    }
    serde_json::from_str(text).map_err(Error::from)
}

pub fn write_excluded(path: &Path, sources: &std::collections::HashSet<PathBuf>) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
    }
    let mut ordered: Vec<&PathBuf> = sources.iter().collect();
    ordered.sort();
    let file = File::create(path).map_err(|e| Error::io(path, e))?;
    serde_json::to_writer_pretty(BufWriter::new(file), &ordered).map_err(Error::from)
}

pub fn load_plan(plan_path: &Path) -> Result<Plan> {
    let mut plan = crate::plan::read_plan(plan_path)?;
    apply(&mut plan, &read(&overrides_path(plan_path))?)?;
    apply_rotations(&mut plan, &read_rotations(&rotations_path(plan_path))?);

    let excluded = read_excluded(&excluded_path(plan_path))?;
    if !excluded.is_empty() {
        plan.entries
            .retain(|entry| !excluded.contains(&entry.source));
    }
    Ok(plan)
}

pub fn apply(plan: &mut Plan, overrides: &Overrides) -> Result<()> {
    if overrides.is_empty() {
        return Ok(());
    }
    let header = plan.header.clone();

    for entry in &mut plan.entries {
        let Some(applied) = overrides.get(&entry.source) else {
            continue;
        };
        entry.taken = applied.taken;
        entry.destination = header.destination_of(entry, applied.taken)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PlanEntry, PlanHeader, DEFAULT_FOLDER_PATTERN, PLAN_VERSION};
    use crate::providers::DetectOptions;
    use chrono::NaiveDate;
    use tempfile::tempdir;

    fn at(y: i32, m: u32, d: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(1, 2, 3)
            .unwrap()
    }

    fn plan() -> Plan {
        let header = PlanHeader {
            version: PLAN_VERSION,
            created_at: at(2026, 8, 6),
            sources: vec![PathBuf::from("/src")],
            destination: Some(PathBuf::from("/out")),
            folder_pattern: DEFAULT_FOLDER_PATTERN.to_string(),
            name_pattern: crate::naming::DEFAULT_NAME_PATTERN.to_string(),
            detect: DetectOptions::default(),
        };
        Plan {
            header,
            entries: vec![PlanEntry {
                source: PathBuf::from("/src/holiday.jpg"),
                destination: PathBuf::from("/out/2003/01/holiday.jpg"),
                taken: at(2003, 1, 1),
                provider: Provider::Exif,
                provider_info: None,
                size: 10,
                ..PlanEntry::default()
            }],
            skipped: Vec::new(),
        }
    }

    fn manual(taken: NaiveDateTime) -> DateOverride {
        DateOverride {
            taken,
            origin: OverrideOrigin::Manual,
            at: at(2026, 8, 6),
        }
    }

    #[test]
    fn apply_rewrites_both_the_date_and_the_destination() {
        let mut plan = plan();
        let mut overrides = Overrides::default();
        overrides.set(PathBuf::from("/src/holiday.jpg"), manual(at(2019, 7, 4)));

        apply(&mut plan, &overrides).unwrap();

        assert_eq!(plan.entries[0].taken, at(2019, 7, 4));
        assert!(plan.entries[0].destination.ends_with("2019/07/holiday.jpg"));
    }

    #[test]
    fn apply_ignores_paths_that_are_not_in_the_plan() {
        let mut plan = plan();
        let mut overrides = Overrides::default();
        overrides.set(PathBuf::from("/src/missing.jpg"), manual(at(2019, 7, 4)));

        apply(&mut plan, &overrides).unwrap();
        assert_eq!(plan.entries[0].taken, at(2003, 1, 1));
    }

    #[test]
    fn survives_a_round_trip_through_the_sidecar() {
        let dir = tempdir().unwrap();
        let path = overrides_path(&dir.path().join("plan.jsonl"));

        let mut overrides = Overrides::default();
        overrides.set(
            PathBuf::from("/src/holiday.jpg"),
            DateOverride {
                taken: at(2019, 7, 4),
                origin: OverrideOrigin::Candidate {
                    provider: Provider::Filesystem,
                },
                at: at(2026, 8, 6),
            },
        );

        write(&path, &overrides).unwrap();
        assert_eq!(read(&path).unwrap(), overrides);
    }

    #[test]
    fn a_missing_sidecar_reads_as_empty() {
        let dir = tempdir().unwrap();
        let path = overrides_path(&dir.path().join("plan.jsonl"));
        assert!(read(&path).unwrap().is_empty());
    }

    #[test]
    fn reads_a_sidecar_written_with_a_byte_order_mark() {
        let dir = tempdir().unwrap();
        let path = overrides_path(&dir.path().join("plan.jsonl"));
        let body = r#"{"/src/a.jpg":{"taken":"2019-07-04T00:00:00","origin":"manual","at":"2026-08-06T00:00:00"}}"#;
        std::fs::write(&path, format!("\u{feff}{body}")).unwrap();

        let loaded = read(&path).unwrap();
        assert_eq!(loaded.len(), 1);
    }

    #[test]
    fn excluded_files_drop_out_of_a_loaded_plan() {
        use crate::model::PlanRecord;
        use crate::model::{PlanHeader, DEFAULT_FOLDER_PATTERN, PLAN_VERSION};
        use crate::plan::PlanWriter;

        let dir = tempdir().unwrap();
        let plan_path = dir.path().join("plan.jsonl");
        let header = PlanHeader {
            version: PLAN_VERSION,
            created_at: at(2026, 8, 6),
            sources: vec![PathBuf::from("/src")],
            destination: Some(PathBuf::from("/out")),
            folder_pattern: DEFAULT_FOLDER_PATTERN.to_string(),
            name_pattern: crate::naming::DEFAULT_NAME_PATTERN.to_string(),
            detect: DetectOptions::default(),
        };

        let mut writer = PlanWriter::create(&plan_path, &header).unwrap();
        for name in ["keep.jpg", "drop.jpg"] {
            writer
                .write(&PlanRecord::Entry(PlanEntry {
                    source: PathBuf::from(format!("/src/{name}")),
                    destination: PathBuf::from(format!("/out/2019/07/{name}")),
                    taken: at(2019, 7, 4),
                    size: 10,
                    ..PlanEntry::default()
                }))
                .unwrap();
        }
        drop(writer);

        assert_eq!(load_plan(&plan_path).unwrap().entries.len(), 2);

        let mut excluded = std::collections::HashSet::new();
        excluded.insert(PathBuf::from("/src/drop.jpg"));
        write_excluded(&excluded_path(&plan_path), &excluded).unwrap();

        let plan = load_plan(&plan_path).unwrap();
        assert_eq!(plan.entries.len(), 1);
        assert!(plan.entries[0].source.ends_with("keep.jpg"));
    }

    #[test]
    fn the_exclusion_list_sits_beside_the_plan_and_round_trips() {
        let dir = tempdir().unwrap();
        let path = excluded_path(&dir.path().join("plan.jsonl"));
        assert!(path.ends_with("plan.excluded.json"));
        assert!(read_excluded(&path).unwrap().is_empty());

        let mut excluded = std::collections::HashSet::new();
        excluded.insert(PathBuf::from("/src/a.jpg"));
        write_excluded(&path, &excluded).unwrap();
        assert_eq!(read_excluded(&path).unwrap(), excluded);
    }

    #[test]
    fn a_turn_survives_a_round_trip_through_its_own_sidecar() {
        let dir = tempdir().unwrap();
        let path = rotations_path(&dir.path().join("plan.jsonl"));
        assert!(path.ends_with("plan.rotations.json"));
        assert!(read_rotations(&path).unwrap().is_empty());

        let mut rotations = Rotations::default();
        rotations.set(
            PathBuf::from("/src/holiday.jpg"),
            RotationOverride {
                transform: Transform::Rotate270,
                reencode: true,
                at: at(2026, 8, 6),
            },
        );

        write_rotations(&path, &rotations).unwrap();
        assert_eq!(read_rotations(&path).unwrap(), rotations);
    }

    #[test]
    fn a_damaged_rotation_sidecar_is_an_error_rather_than_silently_empty() {
        let dir = tempdir().unwrap();
        let path = rotations_path(&dir.path().join("plan.jsonl"));
        std::fs::write(&path, "{ not json").unwrap();
        assert!(read_rotations(&path).is_err());
    }

    #[test]
    fn reads_a_rotation_sidecar_written_with_a_byte_order_mark() {
        let dir = tempdir().unwrap();
        let path = rotations_path(&dir.path().join("plan.jsonl"));
        let body = r#"{"/src/a.jpg":{"transform":"rotate90","reencode":false,"at":"2026-08-06T00:00:00"}}"#;
        std::fs::write(&path, format!("\u{feff}{body}")).unwrap();

        let loaded = read_rotations(&path).unwrap();
        assert_eq!(
            loaded.get(Path::new("/src/a.jpg")).unwrap().transform,
            Transform::Rotate90
        );
    }

    #[test]
    fn applying_a_turn_leaves_the_destination_where_it_was() {
        let mut plan = plan();
        let before = plan.entries[0].destination.clone();

        let mut rotations = Rotations::default();
        rotations.set(
            PathBuf::from("/src/holiday.jpg"),
            RotationOverride {
                transform: Transform::Rotate180,
                reencode: false,
                at: at(2026, 8, 6),
            },
        );

        apply_rotations(&mut plan, &rotations);

        assert_eq!(plan.entries[0].rotate, Transform::Rotate180);
        assert_eq!(plan.entries[0].destination, before);
    }

    #[test]
    fn a_turn_for_a_file_outside_the_plan_is_ignored() {
        let mut plan = plan();
        let mut rotations = Rotations::default();
        rotations.set(
            PathBuf::from("/src/missing.jpg"),
            RotationOverride {
                transform: Transform::Rotate90,
                reencode: false,
                at: at(2026, 8, 6),
            },
        );

        apply_rotations(&mut plan, &rotations);

        assert_eq!(plan.entries[0].rotate, Transform::None);
    }

    #[test]
    fn a_damaged_sidecar_is_an_error_rather_than_silently_empty() {
        let dir = tempdir().unwrap();
        let path = overrides_path(&dir.path().join("plan.jsonl"));
        std::fs::write(&path, "{ not json").unwrap();
        assert!(read(&path).is_err());
    }

    #[test]
    fn the_sidecar_sits_beside_the_plan() {
        assert_eq!(
            overrides_path(Path::new("/plans/plan-abc.jsonl")),
            PathBuf::from("/plans/plan-abc.overrides.json")
        );
    }

    #[test]
    fn describes_a_shift_in_both_directions() {
        assert_eq!(
            OverrideOrigin::Shift { seconds: 90_000 }.describe(),
            "shifted by +1d 1h 0m"
        );
        assert_eq!(
            OverrideOrigin::Shift { seconds: -3_600 }.describe(),
            "shifted by -0d 1h 0m"
        );
    }
}
