use crate::error::{Error, Result};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneObjectFit {
    pub label: String,
    pub bounds: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneFit {
    pub vp: [f32; 2],
    pub rect: [f32; 4],
    pub focal: f32,
    #[serde(default)]
    pub objects: Vec<SceneObjectFit>,
    pub at: NaiveDateTime,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Scenes(pub HashMap<PathBuf, SceneFit>);

impl Scenes {
    pub fn get(&self, source: &Path) -> Option<&SceneFit> {
        self.0.get(source)
    }

    pub fn set(&mut self, source: PathBuf, value: SceneFit) {
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

pub fn scenes_path(plan_path: &Path) -> PathBuf {
    plan_path.with_extension("scenes.json")
}

pub fn read(path: &Path) -> Result<Scenes> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Scenes::default()),
        Err(e) => return Err(Error::io(path, e)),
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    if text.trim().is_empty() {
        return Ok(Scenes::default());
    }
    serde_json::from_str(text).map_err(Error::from)
}

pub fn write(path: &Path, scenes: &Scenes) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
    }
    let file = File::create(path).map_err(|e| Error::io(path, e))?;
    serde_json::to_writer_pretty(BufWriter::new(file), scenes).map_err(Error::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use tempfile::tempdir;

    fn at() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 8, 9)
            .unwrap()
            .and_hms_opt(1, 2, 3)
            .unwrap()
    }

    fn fit() -> SceneFit {
        SceneFit {
            vp: [0.5, 0.5],
            rect: [0.29, 0.29, 0.71, 0.71],
            focal: 1.35,
            objects: Vec::new(),
            at: at(),
        }
    }

    #[test]
    fn the_sidecar_sits_beside_the_plan() {
        assert_eq!(
            scenes_path(Path::new("/plans/plan-abc.jsonl")),
            PathBuf::from("/plans/plan-abc.scenes.json")
        );
    }

    #[test]
    fn a_missing_sidecar_reads_as_nothing_saved() {
        let dir = tempdir().unwrap();
        let path = scenes_path(&dir.path().join("plan.jsonl"));
        assert!(read(&path).unwrap().is_empty());
    }

    #[test]
    fn a_fit_survives_the_round_trip() {
        let dir = tempdir().unwrap();
        let path = scenes_path(&dir.path().join("plan.jsonl"));

        let mut scenes = Scenes::default();
        scenes.set(PathBuf::from("/src/hall.jpg"), fit());
        write(&path, &scenes).unwrap();

        let read_back = read(&path).unwrap();
        assert_eq!(read_back.len(), 1);
        assert_eq!(read_back.get(Path::new("/src/hall.jpg")), Some(&fit()));
    }

    #[test]
    fn objects_ride_along_and_default_when_absent() {
        let dir = tempdir().unwrap();
        let path = scenes_path(&dir.path().join("plan.jsonl"));

        let mut scenes = Scenes::default();
        scenes.set(
            PathBuf::from("/src/street.jpg"),
            SceneFit {
                objects: vec![SceneObjectFit {
                    label: "car".into(),
                    bounds: [0.1, 0.4, 0.3, 0.8],
                }],
                ..fit()
            },
        );
        write(&path, &scenes).unwrap();
        assert_eq!(
            read(&path)
                .unwrap()
                .get(Path::new("/src/street.jpg"))
                .unwrap()
                .objects[0]
                .label,
            "car"
        );

        std::fs::write(
            &path,
            r#"{"/src/bare.jpg":{"vp":[0.5,0.5],"rect":[0.2,0.2,0.8,0.8],"focal":1.35,"at":"2026-08-09T01:02:03"}}"#,
        )
        .unwrap();
        assert!(read(&path)
            .unwrap()
            .get(Path::new("/src/bare.jpg"))
            .unwrap()
            .objects
            .is_empty());
    }

    #[test]
    fn a_byte_order_mark_does_not_stop_it_reading() {
        let dir = tempdir().unwrap();
        let path = scenes_path(&dir.path().join("plan.jsonl"));
        std::fs::write(&path, "\u{feff}{}").unwrap();
        assert!(read(&path).unwrap().is_empty());
    }

    #[test]
    fn a_damaged_sidecar_is_reported_rather_than_silently_emptied() {
        let dir = tempdir().unwrap();
        let path = scenes_path(&dir.path().join("plan.jsonl"));
        std::fs::write(&path, "{ not json").unwrap();
        assert!(read(&path).is_err());
    }

    #[test]
    fn clearing_a_fit_empties_the_store() {
        let mut scenes = Scenes::default();
        scenes.set(PathBuf::from("/src/hall.jpg"), fit());
        assert!(scenes.clear(Path::new("/src/hall.jpg")));
        assert!(!scenes.clear(Path::new("/src/hall.jpg")));
        assert!(scenes.is_empty());
    }
}
