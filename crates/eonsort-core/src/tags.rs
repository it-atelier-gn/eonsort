use crate::error::{Error, Result};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const MAX_TAGS: usize = 6;
pub const TAG_FLOOR: f32 = 0.14;
pub const HIT_FLOOR: f32 = 0.02;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sighting {
    pub tags: Vec<String>,
    #[serde(default)]
    pub vector: Vec<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<f32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Tags(pub HashMap<PathBuf, Sighting>);

impl Tags {
    pub fn get(&self, source: &Path) -> Option<&Sighting> {
        self.0.get(source)
    }

    pub fn set(&mut self, source: PathBuf, sighting: Sighting) {
        self.0.insert(source, sighting);
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub fn store_path(plan_path: &Path) -> PathBuf {
    plan_path.with_extension("tags.db")
}

pub fn sidecar_path(plan_path: &Path) -> PathBuf {
    plan_path.with_extension("tags.json")
}

pub type Listed = (PathBuf, Vec<String>, Option<f32>);

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }

        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS sighting (
                source  TEXT PRIMARY KEY,
                tags    TEXT NOT NULL,
                quality REAL,
                vector  BLOB
            )",
            [],
        )?;

        Ok(Self { conn })
    }

    pub fn beside(plan_path: &Path) -> Result<Self> {
        let store = Self::open(&store_path(plan_path))?;
        store.take_in(&sidecar_path(plan_path))?;
        Ok(store)
    }

    pub fn take_in(&self, sidecar: &Path) -> Result<usize> {
        if !sidecar.exists() || self.len()? > 0 {
            return Ok(0);
        }

        let text = std::fs::read_to_string(sidecar).map_err(|e| Error::io(sidecar, e))?;
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        if text.trim().is_empty() {
            return Ok(0);
        }

        let held: Tags = serde_json::from_str(text)?;
        let taken = held.len();
        self.set_many(held.0.into_iter())?;
        let _ = std::fs::rename(sidecar, sidecar.with_extension("json.imported"));
        Ok(taken)
    }

    pub fn len(&self) -> Result<usize> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM sighting", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }

    pub fn holds(&self, source: &Path) -> Result<bool> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM sighting WHERE source = ?1",
                [key(source)],
                |row| row.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }

    pub fn get(&self, source: &Path) -> Result<Option<Sighting>> {
        self.conn
            .query_row(
                "SELECT tags, quality, vector FROM sighting WHERE source = ?1",
                [key(source)],
                |row| {
                    Ok(Sighting {
                        tags: from_row_tags(row.get::<_, String>(0)?),
                        quality: row.get::<_, Option<f64>>(1)?.map(|q| q as f32),
                        vector: unpack(row.get::<_, Option<Vec<u8>>>(2)?.unwrap_or_default()),
                    })
                },
            )
            .optional()
            .map_err(Error::from)
    }

    pub fn set_many(
        &self,
        sightings: impl Iterator<Item = (PathBuf, Sighting)>,
    ) -> Result<usize> {
        let mut written = 0usize;
        self.conn.execute("BEGIN IMMEDIATE", [])?;
        {
            let mut insert = self.conn.prepare_cached(
                "INSERT INTO sighting (source, tags, quality, vector)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(source) DO UPDATE SET
                     tags = excluded.tags,
                     quality = excluded.quality,
                     vector = excluded.vector",
            )?;

            for (source, sighting) in sightings {
                insert.execute(rusqlite::params![
                    key(&source),
                    serde_json::to_string(&sighting.tags)?,
                    sighting.quality.map(|q| q as f64),
                    pack(&sighting.vector),
                ])?;
                written += 1;
            }
        }
        self.conn.execute("COMMIT", [])?;
        Ok(written)
    }

    pub fn keep_only(&self, wanted: &[PathBuf]) -> Result<usize> {
        self.conn.execute("BEGIN IMMEDIATE", [])?;
        self.conn
            .execute("CREATE TEMP TABLE IF NOT EXISTS wanted (source TEXT PRIMARY KEY)", [])?;
        self.conn.execute("DELETE FROM wanted", [])?;
        {
            let mut insert = self
                .conn
                .prepare_cached("INSERT OR IGNORE INTO wanted (source) VALUES (?1)")?;
            for source in wanted {
                insert.execute([key(source)])?;
            }
        }
        let gone = self.conn.execute(
            "DELETE FROM sighting WHERE source NOT IN (SELECT source FROM wanted)",
            [],
        )?;
        self.conn.execute("COMMIT", [])?;
        Ok(gone)
    }

    pub fn listing(&self) -> Result<Vec<Listed>> {
        let mut query = self
            .conn
            .prepare("SELECT source, tags, quality FROM sighting")?;
        let rows = query.query_map([], |row| {
            Ok((
                PathBuf::from(row.get::<_, String>(0)?),
                from_row_tags(row.get::<_, String>(1)?),
                row.get::<_, Option<f64>>(2)?.map(|q| q as f32),
            ))
        })?;

        let mut listed = Vec::new();
        for row in rows {
            listed.push(row?);
        }
        Ok(listed)
    }

    pub fn search(&self, wanted: &[f32], words: &str) -> Result<Vec<(PathBuf, f32)>> {
        let mut hits: Vec<(PathBuf, f32)> = Vec::new();

        if wanted.is_empty() {
            let needles: Vec<String> = words
                .split_whitespace()
                .map(|w| w.to_lowercase())
                .filter(|w| !w.is_empty() && w != "and" && w != "or")
                .collect();
            if needles.is_empty() {
                return Ok(Vec::new());
            }

            let mut query = self.conn.prepare("SELECT source, tags FROM sighting")?;
            let mut rows = query.query([])?;
            while let Some(row) = rows.next()? {
                let source = PathBuf::from(row.get::<_, String>(0)?);
                let tags = from_row_tags(row.get::<_, String>(1)?);
                let matched = needles
                    .iter()
                    .filter(|needle| tags.iter().any(|tag| tag.contains(needle.as_str())))
                    .count();
                if matched > 0 {
                    hits.push((source, matched as f32 / needles.len() as f32));
                }
            }
        } else {
            let mut query = self.conn.prepare("SELECT source, vector FROM sighting")?;
            let mut rows = query.query([])?;
            while let Some(row) = rows.next()? {
                let vector = unpack(row.get::<_, Option<Vec<u8>>>(1)?.unwrap_or_default());
                let score = cosine(wanted, &vector);
                if score >= HIT_FLOOR {
                    hits.push((PathBuf::from(row.get::<_, String>(0)?), score));
                }
            }
        }

        hits.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        Ok(hits)
    }
}

fn key(source: &Path) -> String {
    source.to_string_lossy().into_owned()
}

fn from_row_tags(raw: String) -> Vec<String> {
    serde_json::from_str(&raw).unwrap_or_default()
}

fn pack(vector: &[f32]) -> Vec<u8> {
    let mut packed = Vec::with_capacity(vector.len() * 4);
    for value in vector {
        packed.extend_from_slice(&value.to_le_bytes());
    }
    packed
}

fn unpack(packed: Vec<u8>) -> Vec<f32> {
    packed
        .chunks_exact(4)
        .map(|four| f32::from_le_bytes([four[0], four[1], four[2], four[3]]))
        .collect()
}

pub fn normalise(vector: &mut [f32]) {
    let length = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
    if length > 0.0 && length.is_finite() {
        for value in vector.iter_mut() {
            *value /= length;
        }
    }
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

pub fn top_tags(vocabulary: &[&str], scores: &[f32]) -> Vec<String> {
    let mut ranked: Vec<(usize, f32)> = scores
        .iter()
        .copied()
        .enumerate()
        .filter(|(index, score)| *index < vocabulary.len() && score.is_finite())
        .collect();

    let best = ranked
        .iter()
        .map(|(_, score)| *score)
        .fold(f32::NEG_INFINITY, f32::max);
    if !best.is_finite() {
        return Vec::new();
    }

    ranked.retain(|(_, score)| *score >= best * TAG_FLOOR.max(0.0) && *score > 0.0);
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    ranked.truncate(MAX_TAGS);
    ranked
        .into_iter()
        .map(|(index, _)| vocabulary[index].to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sighting(tags: &[&str], vector: Vec<f32>) -> Sighting {
        Sighting {
            tags: tags.iter().map(|t| t.to_string()).collect(),
            vector,
            quality: None,
        }
    }

    fn store() -> Store {
        Store::open(Path::new(":memory:")).unwrap()
    }

    fn put(store: &Store, source: &str, sighting: Sighting) {
        store
            .set_many(std::iter::once((PathBuf::from(source), sighting)))
            .unwrap();
    }

    #[test]
    fn the_store_sits_beside_the_plan() {
        let path = store_path(Path::new("/plans/photos.jsonl"));
        assert_eq!(path, PathBuf::from("/plans/photos.tags.db"));
        assert_eq!(
            sidecar_path(Path::new("/plans/photos.jsonl")),
            PathBuf::from("/plans/photos.tags.json")
        );
    }

    #[test]
    fn a_fresh_store_has_seen_nothing_yet() {
        let store = store();
        assert!(store.is_empty().unwrap());
        assert!(store.get(Path::new("/a.jpg")).unwrap().is_none());
        assert!(!store.holds(Path::new("/a.jpg")).unwrap());
    }

    #[test]
    fn what_was_written_is_what_comes_back() {
        let store = store();
        put(&store, "/a.jpg", sighting(&["a dog"], vec![0.5, 0.5]));

        let held = store.get(Path::new("/a.jpg")).unwrap().unwrap();
        assert_eq!(held.tags, vec!["a dog".to_string()]);
        assert_eq!(held.vector, vec![0.5, 0.5]);
        assert_eq!(held.quality, None);
        assert_eq!(store.len().unwrap(), 1);
    }

    #[test]
    fn a_score_survives_the_round_trip() {
        let store = store();
        let mut seen = sighting(&["a beach"], vec![1.0]);
        seen.quality = Some(6.25);
        put(&store, "/b.jpg", seen);

        assert_eq!(
            store.get(Path::new("/b.jpg")).unwrap().unwrap().quality,
            Some(6.25)
        );
    }

    #[test]
    fn seeing_a_picture_again_replaces_what_was_known() {
        let store = store();
        put(&store, "/a.jpg", sighting(&["a dog"], vec![1.0, 0.0]));
        put(&store, "/a.jpg", sighting(&["a wolf"], vec![0.0, 1.0]));

        let held = store.get(Path::new("/a.jpg")).unwrap().unwrap();
        assert_eq!(held.tags, vec!["a wolf".to_string()]);
        assert_eq!(held.vector, vec![0.0, 1.0]);
        assert_eq!(store.len().unwrap(), 1);
    }

    #[test]
    fn a_batch_goes_in_as_one_piece_of_work() {
        let store = store();
        let batch = vec![
            (PathBuf::from("/a.jpg"), sighting(&["a dog"], vec![1.0])),
            (PathBuf::from("/b.jpg"), sighting(&["a beach"], vec![0.0])),
        ];
        assert_eq!(store.set_many(batch.into_iter()).unwrap(), 2);
        assert_eq!(store.len().unwrap(), 2);
    }

    #[test]
    fn forgetting_files_that_left_the_plan() {
        let store = store();
        put(&store, "/a.jpg", sighting(&["a dog"], vec![]));
        put(&store, "/b.jpg", sighting(&["a beach"], vec![]));

        assert_eq!(store.keep_only(&[PathBuf::from("/a.jpg")]).unwrap(), 1);
        assert!(store.holds(Path::new("/a.jpg")).unwrap());
        assert!(!store.holds(Path::new("/b.jpg")).unwrap());
    }

    #[test]
    fn keeping_nothing_empties_the_store() {
        let store = store();
        put(&store, "/a.jpg", sighting(&["a dog"], vec![]));
        assert_eq!(store.keep_only(&[]).unwrap(), 1);
        assert!(store.is_empty().unwrap());
    }

    #[test]
    fn the_listing_leaves_the_vectors_where_they_are() {
        let store = store();
        let mut seen = sighting(&["a dog"], vec![0.1, 0.2, 0.3]);
        seen.quality = Some(5.5);
        put(&store, "/a.jpg", seen);

        let listed = store.listing().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].0, PathBuf::from("/a.jpg"));
        assert_eq!(listed[0].1, vec!["a dog".to_string()]);
        assert_eq!(listed[0].2, Some(5.5));
    }

    #[test]
    fn a_sidecar_from_the_old_days_is_taken_in_once() {
        let dir = std::env::temp_dir().join(format!("eonsort-tags-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sidecar = dir.join("photos.tags.json");
        std::fs::write(
            &sidecar,
            r#"{"/photos/a.jpg":{"tags":["a dog"],"vector":[0.5,0.5],"quality":6.25}}"#,
        )
        .unwrap();

        let store = store();
        assert_eq!(store.take_in(&sidecar).unwrap(), 1);

        let held = store.get(Path::new("/photos/a.jpg")).unwrap().unwrap();
        assert_eq!(held.tags, vec!["a dog".to_string()]);
        assert_eq!(held.quality, Some(6.25));
        assert!(!sidecar.exists());
        assert!(sidecar.with_extension("json.imported").exists());

        assert_eq!(store.take_in(&sidecar).unwrap(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_store_that_already_knows_things_leaves_a_sidecar_alone() {
        let dir = std::env::temp_dir().join(format!("eonsort-tags-held-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sidecar = dir.join("photos.tags.json");
        std::fs::write(&sidecar, r#"{"/photos/a.jpg":{"tags":["a dog"]}}"#).unwrap();

        let store = store();
        put(&store, "/b.jpg", sighting(&["a beach"], vec![]));
        assert_eq!(store.take_in(&sidecar).unwrap(), 0);
        assert!(sidecar.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_damaged_sidecar_is_reported_rather_than_silently_emptied() {
        let dir = std::env::temp_dir().join(format!("eonsort-tags-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sidecar = dir.join("photos.tags.json");
        std::fs::write(&sidecar, "{ this is not json").unwrap();

        let store = store();
        assert!(store.take_in(&sidecar).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_normalised_vector_has_unit_length() {
        let mut vector = vec![3.0, 4.0];
        normalise(&mut vector);
        assert!((cosine(&vector, &vector) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn normalising_nothing_does_not_divide_by_zero() {
        let mut vector = vec![0.0, 0.0];
        normalise(&mut vector);
        assert_eq!(vector, vec![0.0, 0.0]);
    }

    #[test]
    fn vectors_of_different_lengths_do_not_match() {
        assert_eq!(cosine(&[1.0, 0.0], &[1.0]), 0.0);
        assert_eq!(cosine(&[], &[]), 0.0);
    }

    #[test]
    fn a_vector_survives_being_packed_into_bytes() {
        let vector = vec![0.25, -1.5, 3.75];
        assert_eq!(unpack(pack(&vector)), vector);
        assert!(unpack(Vec::new()).is_empty());
    }

    #[test]
    fn the_strongest_tags_come_back_in_order() {
        let vocabulary = ["forest", "dog", "city"];
        assert_eq!(
            top_tags(&vocabulary, &[0.9, 0.8, 0.01]),
            vec!["forest", "dog"]
        );
    }

    #[test]
    fn a_picture_of_nothing_in_the_vocabulary_gets_no_tags() {
        let vocabulary = ["forest"];
        assert!(top_tags(&vocabulary, &[]).is_empty());
        assert!(top_tags(&vocabulary, &[-0.5]).is_empty());
    }

    #[test]
    fn no_more_than_a_handful_of_tags_per_picture() {
        let vocabulary = ["a", "b", "c", "d", "e", "f", "g", "h"];
        let scores = vec![0.9f32; vocabulary.len()];
        assert_eq!(top_tags(&vocabulary, &scores).len(), MAX_TAGS);
    }

    #[test]
    fn scores_that_are_not_numbers_are_ignored() {
        let vocabulary = ["forest", "dog"];
        assert_eq!(top_tags(&vocabulary, &[f32::NAN, 0.8]), vec!["dog"]);
    }

    #[test]
    fn more_scores_than_words_does_not_run_off_the_end() {
        let vocabulary = ["forest"];
        assert_eq!(top_tags(&vocabulary, &[0.9, 0.8, 0.7]), vec!["forest"]);
    }

    #[test]
    fn searching_by_meaning_ranks_the_closest_picture_first() {
        let store = store();
        put(&store, "/far.jpg", sighting(&[], vec![0.0, 1.0]));
        put(&store, "/near.jpg", sighting(&[], vec![1.0, 0.0]));

        let hits = store.search(&[1.0, 0.0], "").unwrap();
        assert_eq!(hits[0].0, PathBuf::from("/near.jpg"));
    }

    #[test]
    fn a_picture_of_something_else_entirely_is_left_out() {
        let store = store();
        put(&store, "/other.jpg", sighting(&[], vec![-1.0, 0.0]));
        assert!(store.search(&[1.0, 0.0], "").unwrap().is_empty());
    }

    #[test]
    fn without_a_model_the_words_are_matched_against_the_tags() {
        let store = store();
        put(&store, "/both.jpg", sighting(&["forest", "dog"], vec![]));
        put(&store, "/one.jpg", sighting(&["forest"], vec![]));
        put(&store, "/none.jpg", sighting(&["city"], vec![]));

        let hits = store.search(&[], "forest and dog").unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].0, PathBuf::from("/both.jpg"));
        assert!(hits[0].1 > hits[1].1);
    }

    #[test]
    fn the_word_and_is_not_something_to_look_for() {
        let store = store();
        put(&store, "/a.jpg", sighting(&["sandy beach"], vec![]));
        assert!(store.search(&[], "and").unwrap().is_empty());
    }

    #[test]
    fn an_empty_question_gets_an_empty_answer() {
        let store = store();
        put(&store, "/a.jpg", sighting(&["dog"], vec![]));
        assert!(store.search(&[], "   ").unwrap().is_empty());
    }
}
