use crate::error::{Error, Result};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const MAX_TAGS: usize = 6;
pub const TAG_CONFIDENCE: f32 = 0.01;
pub const HIT_FLOOR: f32 = 0.02;

const PROJECTION: &str = "projection";
const REPROJECT_CHUNK: usize = 512;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Spot {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub score: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub spot: Spot,
    pub vector: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Portrait {
    pub digest: String,
    pub ord: usize,
    pub spot: Spot,
    pub vector: Vec<f32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Sighting {
    pub tags: Vec<String>,
    #[serde(default)]
    pub vector: Vec<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faces: Option<Vec<Spot>>,
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

pub fn library_path(data_dir: &Path) -> PathBuf {
    data_dir.join("sightings.db")
}

pub fn store_path(plan_path: &Path) -> PathBuf {
    plan_path.with_extension("tags.db")
}

pub fn sidecar_path(plan_path: &Path) -> PathBuf {
    plan_path.with_extension("tags.json")
}

pub type Listed = (PathBuf, Vec<String>, Option<f32>);
type Handed = (String, String, Option<f64>, Option<Vec<u8>>);

pub fn digest(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(file).ok()?;
    Some(hasher.finalize().to_hex().to_string())
}

fn stat(path: &Path) -> Option<(i64, i64)> {
    let facts = std::fs::metadata(path).ok()?;
    let modified = facts
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or(0);
    Some((facts.len() as i64, modified))
}

pub fn projection_stamp(vocabulary: &[&str]) -> String {
    let mut hasher = blake3::Hasher::new();
    for word in vocabulary {
        hasher.update(word.as_bytes());
        hasher.update(b"\0");
    }
    hasher.update(&MAX_TAGS.to_le_bytes());
    hasher.update(&TAG_CONFIDENCE.to_le_bytes());
    hasher.finalize().to_hex()[..16].to_string()
}

#[derive(Debug, Clone, Default)]
pub struct Known {
    looked: HashMap<String, Option<String>>,
    rated: HashSet<String>,
    faced: HashSet<String>,
}

impl Known {
    pub fn looked(&self, digest: &str) -> bool {
        self.looked.contains_key(digest)
    }

    pub fn looked_with(&self, digest: &str, model: &str) -> bool {
        match self.looked.get(digest) {
            Some(Some(held)) => held == model,
            Some(None) => false,
            None => false,
        }
    }

    pub fn rated(&self, digest: &str) -> bool {
        self.rated.contains(digest)
    }

    pub fn faced(&self, digest: &str) -> bool {
        self.faced.contains(digest)
    }

    pub fn saw_faces(&mut self, digest: &str) {
        self.faced.insert(digest.to_string());
    }

    pub fn saw(&mut self, digest: &str) {
        self.looked.insert(digest.to_string(), None);
    }

    pub fn saw_with(&mut self, digest: &str, model: &str) {
        self.looked
            .insert(digest.to_string(), Some(model.to_string()));
    }

    pub fn len(&self) -> usize {
        self.looked.len()
    }

    pub fn is_empty(&self) -> bool {
        self.looked.is_empty()
    }
}

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
                digest  TEXT PRIMARY KEY,
                tags    TEXT NOT NULL,
                quality REAL,
                vector  BLOB,
                model   TEXT,
                faces   TEXT
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS worn (
                digest TEXT NOT NULL,
                tag    TEXT NOT NULL,
                PRIMARY KEY (digest, tag)
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS file (
                path     TEXT PRIMARY KEY,
                size     INTEGER NOT NULL,
                modified INTEGER NOT NULL,
                digest   TEXT NOT NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS inherited (
                path    TEXT PRIMARY KEY,
                tags    TEXT NOT NULL,
                quality REAL,
                vector  BLOB,
                model   TEXT,
                faces   TEXT
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS face (
                digest TEXT NOT NULL,
                ord    INTEGER NOT NULL,
                x      REAL NOT NULL,
                y      REAL NOT NULL,
                width  REAL NOT NULL,
                height REAL NOT NULL,
                score  REAL NOT NULL,
                vector BLOB,
                label  TEXT,
                PRIMARY KEY (digest, ord)
            )",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS face_by_label ON face (label, digest)",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS meta (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS worn_by_tag ON worn (tag, digest)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS sighting_by_quality ON sighting (quality)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS file_by_digest ON file (digest)",
            [],
        )?;

        let store = Self { conn };
        for table in ["sighting", "inherited"] {
            store.add_text_column(table, "model")?;
            store.add_text_column(table, "faces")?;
        }
        store.hang_up_missing_tags()?;
        Ok(store)
    }

    fn add_text_column(&self, table: &str, column: &str) -> Result<()> {
        let held: bool = self
            .conn
            .prepare(&format!("PRAGMA table_info({table})"))?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<String>>>()?
            .iter()
            .any(|name| name == column);
        if !held {
            self.conn
                .execute(&format!("ALTER TABLE {table} ADD COLUMN {column} TEXT"), [])?;
        }
        Ok(())
    }

    pub fn library(data_dir: &Path) -> Result<Self> {
        Self::open(&library_path(data_dir))
    }

    pub fn identify(&self, path: &Path) -> Result<Option<String>> {
        let Some((size, modified)) = stat(path) else {
            return Ok(None);
        };

        let at = key(path);
        let held: Option<(i64, i64, String)> = self
            .conn
            .query_row(
                "SELECT size, modified, digest FROM file WHERE path = ?1",
                [&at],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;

        if let Some((was_size, was_modified, digest)) = held {
            if was_size == size && was_modified == modified {
                return Ok(Some(digest));
            }
        }

        let Some(digest) = digest(path) else {
            return Ok(None);
        };
        self.conn.execute(
            "INSERT INTO file (path, size, modified, digest) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET
                 size = excluded.size,
                 modified = excluded.modified,
                 digest = excluded.digest",
            rusqlite::params![at, size, modified, digest],
        )?;
        Ok(Some(digest))
    }

    pub fn inherit(&self, plan_path: &Path) -> Result<usize> {
        let legacy = store_path(plan_path);
        let mut taken = self.take_in(&sidecar_path(plan_path))?;

        if legacy.exists() {
            let read = || -> rusqlite::Result<Vec<Handed>> {
                let old = Connection::open(&legacy)?;
                let mut query =
                    old.prepare("SELECT source, tags, quality, vector FROM sighting")?;
                let rows = query.query_map([], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                })?;
                let mut held = Vec::new();
                for row in rows {
                    held.push(row?);
                }
                Ok(held)
            };

            let Ok(listed) = read() else {
                return Ok(taken);
            };

            self.conn.execute("BEGIN IMMEDIATE", [])?;
            {
                let mut keep = self.conn.prepare_cached(
                    "INSERT OR IGNORE INTO inherited (path, tags, quality, vector)
                     VALUES (?1, ?2, ?3, ?4)",
                )?;
                for (source, tags, quality, vector) in listed {
                    keep.execute(rusqlite::params![source, tags, quality, vector])?;
                    taken += 1;
                }
            }
            self.conn.execute("COMMIT", [])?;
            let _ = std::fs::rename(&legacy, legacy.with_extension("db.inherited"));
        }

        Ok(taken)
    }

    pub fn claim(
        &self,
        path: &Path,
        digest: &str,
        project: &dyn Fn(&[f32]) -> Vec<String>,
    ) -> Result<Option<Sighting>> {
        let at = key(path);
        let held: Option<Sighting> = self
            .conn
            .query_row(
                "SELECT tags, quality, vector, model, faces FROM inherited WHERE path = ?1",
                [&at],
                |row| {
                    Ok(Sighting {
                        tags: from_row_tags(row.get::<_, String>(0)?),
                        quality: row.get::<_, Option<f64>>(1)?.map(|q| q as f32),
                        vector: unpack(row.get::<_, Option<Vec<u8>>>(2)?.unwrap_or_default()),
                        model: row.get::<_, Option<String>>(3)?,
                        faces: from_row_faces(row.get::<_, Option<String>>(4)?),
                    })
                },
            )
            .optional()?;

        let Some(mut sighting) = held else {
            return Ok(None);
        };
        if sighting.vector.is_empty() {
            self.conn
                .execute("DELETE FROM inherited WHERE path = ?1", [&at])?;
            return Ok(None);
        }

        sighting.tags = project(&sighting.vector);
        self.set_many(std::iter::once((digest.to_string(), sighting.clone())))?;
        self.conn
            .execute("DELETE FROM inherited WHERE path = ?1", [&at])?;
        Ok(Some(sighting))
    }

    pub fn take_in(&self, sidecar: &Path) -> Result<usize> {
        if !sidecar.exists() {
            return Ok(0);
        }

        let text = std::fs::read_to_string(sidecar).map_err(|e| Error::io(sidecar, e))?;
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        if text.trim().is_empty() {
            return Ok(0);
        }

        let held: Tags = serde_json::from_str(text)?;
        let taken = held.len();

        self.conn.execute("BEGIN IMMEDIATE", [])?;
        {
            let mut keep = self.conn.prepare_cached(
                "INSERT OR IGNORE INTO inherited (path, tags, quality, vector, model, faces)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for (source, sighting) in held.0 {
                keep.execute(rusqlite::params![
                    key(&source),
                    serde_json::to_string(&sighting.tags)?,
                    sighting.quality.map(|q| q as f64),
                    pack(&sighting.vector),
                    sighting.model.as_deref(),
                    to_row_faces(&sighting.faces)?,
                ])?;
            }
        }
        self.conn.execute("COMMIT", [])?;

        let _ = std::fs::rename(sidecar, sidecar.with_extension("json.inherited"));
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

    pub fn holds(&self, digest: &str) -> Result<bool> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM sighting WHERE digest = ?1",
                [digest],
                |row| row.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }

    pub fn known(&self) -> Result<Known> {
        let mut query = self.conn.prepare(
            "SELECT digest, IFNULL(length(vector), 0) > 0, quality IS NOT NULL, model,
                    faces IS NOT NULL
             FROM sighting",
        )?;
        let rows = query.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, bool>(1)?,
                row.get::<_, bool>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, bool>(4)?,
            ))
        })?;

        let mut known = Known::default();
        for row in rows {
            let (digest, has_vector, has_quality, model, has_faces) = row?;
            if has_vector {
                known.looked.insert(digest.clone(), model);
            }
            if has_quality {
                known.rated.insert(digest.clone());
            }
            if has_faces {
                known.faced.insert(digest);
            }
        }
        Ok(known)
    }

    pub fn stamp_of(&self, key: &str) -> Result<Option<String>> {
        self.conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()
            .map_err(Error::from)
    }

    pub fn set_stamp(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    pub fn projection_matches(&self, stamp: &str) -> Result<bool> {
        Ok(self.stamp_of(PROJECTION)?.as_deref() == Some(stamp))
    }

    pub fn mark_projection(&self, stamp: &str) -> Result<()> {
        self.set_stamp(PROJECTION, stamp)
    }

    pub fn reproject(&self, stamp: &str, project: &dyn Fn(&[f32]) -> Vec<String>) -> Result<usize> {
        let mut done = 0usize;
        let mut offset = 0usize;

        loop {
            let batch: Vec<(String, Vec<f32>)> = {
                let mut query = self.conn.prepare(
                    "SELECT digest, vector FROM sighting
                     WHERE vector IS NOT NULL AND length(vector) > 0
                     ORDER BY digest LIMIT ?1 OFFSET ?2",
                )?;
                let rows = query.query_map(
                    rusqlite::params![REPROJECT_CHUNK as i64, offset as i64],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            unpack(row.get::<_, Option<Vec<u8>>>(1)?.unwrap_or_default()),
                        ))
                    },
                )?;

                let mut got = Vec::new();
                for row in rows {
                    got.push(row?);
                }
                got
            };

            if batch.is_empty() {
                break;
            }
            offset += batch.len();

            self.conn.execute("BEGIN IMMEDIATE", [])?;
            {
                let mut retag = self
                    .conn
                    .prepare_cached("UPDATE sighting SET tags = ?2 WHERE digest = ?1")?;
                let mut forget = self
                    .conn
                    .prepare_cached("DELETE FROM worn WHERE digest = ?1")?;
                let mut wear = self
                    .conn
                    .prepare_cached("INSERT OR IGNORE INTO worn (digest, tag) VALUES (?1, ?2)")?;

                for (digest, vector) in &batch {
                    let tags = project(vector);
                    retag.execute(rusqlite::params![digest, serde_json::to_string(&tags)?])?;
                    forget.execute([digest])?;
                    for tag in &tags {
                        wear.execute(rusqlite::params![digest, tag])?;
                    }
                    done += 1;
                }
            }
            self.conn.execute("COMMIT", [])?;
        }

        self.set_stamp(PROJECTION, stamp)?;
        Ok(done)
    }

    pub fn set_quality_many(
        &self,
        scores: impl Iterator<Item = (String, Option<f32>)>,
    ) -> Result<usize> {
        let mut written = 0usize;
        self.conn.execute("BEGIN IMMEDIATE", [])?;
        {
            let mut rate = self
                .conn
                .prepare_cached("UPDATE sighting SET quality = ?2 WHERE digest = ?1")?;
            for (digest, score) in scores {
                rate.execute(rusqlite::params![digest, score.map(|q| q as f64)])?;
                written += 1;
            }
        }
        self.conn.execute("COMMIT", [])?;
        Ok(written)
    }

    pub fn set_faces_many(
        &self,
        found: impl Iterator<Item = (String, Vec<Found>)>,
    ) -> Result<usize> {
        let mut written = 0usize;
        self.conn.execute("BEGIN IMMEDIATE", [])?;
        {
            let mut mark = self.conn.prepare_cached(
                "INSERT INTO sighting (digest, tags, faces) VALUES (?1, '[]', ?2)
                 ON CONFLICT(digest) DO UPDATE SET faces = excluded.faces",
            )?;
            let mut clear = self
                .conn
                .prepare_cached("DELETE FROM face WHERE digest = ?1")?;
            let mut keep = self.conn.prepare_cached(
                "INSERT INTO face (digest, ord, x, y, width, height, score, vector, label)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;

            for (digest, faces) in found {
                let spots: Vec<Spot> = faces.iter().map(|face| face.spot.clone()).collect();
                mark.execute(rusqlite::params![digest, serde_json::to_string(&spots)?])?;
                clear.execute([&digest])?;
                for (ord, face) in faces.iter().enumerate() {
                    keep.execute(rusqlite::params![
                        digest,
                        ord as i64,
                        face.spot.x as f64,
                        face.spot.y as f64,
                        face.spot.width as f64,
                        face.spot.height as f64,
                        face.spot.score as f64,
                        pack(&face.vector),
                        face.spot.label.as_deref(),
                    ])?;
                }
                written += 1;
            }
        }
        self.conn.execute("COMMIT", [])?;
        Ok(written)
    }

    pub fn forget_faces(&self, digests: &[String]) -> Result<usize> {
        let mut forgotten = 0usize;
        self.conn.execute("BEGIN IMMEDIATE", [])?;
        {
            let mut clear = self
                .conn
                .prepare_cached("DELETE FROM face WHERE digest = ?1")?;
            let mut unmark = self.conn.prepare_cached(
                "UPDATE sighting SET faces = NULL WHERE digest = ?1 AND faces IS NOT NULL",
            )?;
            for digest in digests {
                clear.execute([digest])?;
                forgotten += unmark.execute([digest])?;
            }
        }
        self.conn.execute("COMMIT", [])?;
        Ok(forgotten)
    }

    pub fn name_face(&self, digest: &str, ord: usize, label: Option<&str>) -> Result<()> {
        self.conn.execute("BEGIN IMMEDIATE", [])?;
        self.conn.execute(
            "UPDATE face SET label = ?3 WHERE digest = ?1 AND ord = ?2",
            rusqlite::params![digest, ord as i64, label],
        )?;
        self.restate(digest)?;
        self.conn.execute("COMMIT", [])?;
        Ok(())
    }

    fn restate(&self, digest: &str) -> Result<()> {
        let spots: Vec<Spot> = self
            .portraits_of(digest)?
            .into_iter()
            .map(|held| held.spot)
            .collect();
        self.conn.execute(
            "UPDATE sighting SET faces = ?2 WHERE digest = ?1",
            rusqlite::params![digest, serde_json::to_string(&spots)?],
        )?;
        Ok(())
    }

    pub fn portraits_of(&self, digest: &str) -> Result<Vec<Portrait>> {
        let mut query = self.conn.prepare_cached(
            "SELECT ord, x, y, width, height, score, vector, label
             FROM face WHERE digest = ?1 ORDER BY ord",
        )?;
        let rows = query.query_map([digest], |row| {
            Ok(Portrait {
                digest: digest.to_string(),
                ord: row.get::<_, i64>(0)? as usize,
                spot: Spot {
                    x: row.get::<_, f64>(1)? as f32,
                    y: row.get::<_, f64>(2)? as f32,
                    width: row.get::<_, f64>(3)? as f32,
                    height: row.get::<_, f64>(4)? as f32,
                    score: row.get::<_, f64>(5)? as f32,
                    label: row.get::<_, Option<String>>(7)?,
                },
                vector: unpack(row.get::<_, Option<Vec<u8>>>(6)?.unwrap_or_default()),
            })
        })?;

        let mut held = Vec::new();
        for row in rows {
            held.push(row?);
        }
        Ok(held)
    }

    pub fn every_portrait(&self) -> Result<Vec<Portrait>> {
        let mut query = self
            .conn
            .prepare("SELECT digest, ord, x, y, width, height, score, vector, label FROM face")?;
        let rows = query.query_map([], |row| {
            Ok(Portrait {
                digest: row.get::<_, String>(0)?,
                ord: row.get::<_, i64>(1)? as usize,
                spot: Spot {
                    x: row.get::<_, f64>(2)? as f32,
                    y: row.get::<_, f64>(3)? as f32,
                    width: row.get::<_, f64>(4)? as f32,
                    height: row.get::<_, f64>(5)? as f32,
                    score: row.get::<_, f64>(6)? as f32,
                    label: row.get::<_, Option<String>>(8)?,
                },
                vector: unpack(row.get::<_, Option<Vec<u8>>>(7)?.unwrap_or_default()),
            })
        })?;

        let mut held = Vec::new();
        for row in rows {
            held.push(row?);
        }
        Ok(held)
    }

    pub fn names(&self) -> Result<Vec<(String, usize)>> {
        let mut query = self.conn.prepare(
            "SELECT label, COUNT(*) FROM face
             WHERE label IS NOT NULL AND label <> ''
             GROUP BY label ORDER BY COUNT(*) DESC, label",
        )?;
        let rows = query.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;

        let mut held = Vec::new();
        for row in rows {
            let (label, count) = row?;
            held.push((label, count as usize));
        }
        Ok(held)
    }

    pub fn name_everything_like(&self, vector: &[f32], label: &str, floor: f32) -> Result<usize> {
        let alike: Vec<(String, usize)> = self
            .every_portrait()?
            .into_iter()
            .filter(|held| held.spot.label.is_none() && !held.vector.is_empty())
            .filter(|held| cosine(&held.vector, vector) >= floor)
            .map(|held| (held.digest, held.ord))
            .collect();

        let touched = alike.len();
        for (digest, ord) in alike {
            self.name_face(&digest, ord, Some(label))?;
        }
        Ok(touched)
    }

    pub fn faces_of(&self, digest: &str) -> Result<Option<Vec<Spot>>> {
        let held: Option<Option<String>> = self
            .conn
            .query_row(
                "SELECT faces FROM sighting WHERE digest = ?1",
                [digest],
                |row| row.get(0),
            )
            .optional()?;
        Ok(held.and_then(from_row_faces))
    }

    pub fn get(&self, digest: &str) -> Result<Option<Sighting>> {
        self.conn
            .query_row(
                "SELECT tags, quality, vector, model, faces FROM sighting WHERE digest = ?1",
                [digest],
                |row| {
                    Ok(Sighting {
                        tags: from_row_tags(row.get::<_, String>(0)?),
                        quality: row.get::<_, Option<f64>>(1)?.map(|q| q as f32),
                        vector: unpack(row.get::<_, Option<Vec<u8>>>(2)?.unwrap_or_default()),
                        model: row.get::<_, Option<String>>(3)?,
                        faces: from_row_faces(row.get::<_, Option<String>>(4)?),
                    })
                },
            )
            .optional()
            .map_err(Error::from)
    }

    pub fn at(&self, source: &Path) -> Result<Option<Sighting>> {
        let Some(digest) = self.digest_of(source)? else {
            return Ok(None);
        };
        self.get(&digest)
    }

    pub fn digest_of(&self, source: &Path) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT digest FROM file WHERE path = ?1",
                [key(source)],
                |row| row.get(0),
            )
            .optional()
            .map_err(Error::from)
    }

    pub fn set_many(&self, sightings: impl Iterator<Item = (String, Sighting)>) -> Result<usize> {
        let mut written = 0usize;
        self.conn.execute("BEGIN IMMEDIATE", [])?;
        {
            let mut insert = self.conn.prepare_cached(
                "INSERT INTO sighting (digest, tags, quality, vector, model, faces)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(digest) DO UPDATE SET
                     tags = excluded.tags,
                     quality = excluded.quality,
                     vector = excluded.vector,
                     model = excluded.model,
                     faces = COALESCE(excluded.faces, sighting.faces)",
            )?;

            let mut forget = self
                .conn
                .prepare_cached("DELETE FROM worn WHERE digest = ?1")?;
            let mut wear = self
                .conn
                .prepare_cached("INSERT OR IGNORE INTO worn (digest, tag) VALUES (?1, ?2)")?;

            for (digest, sighting) in sightings {
                insert.execute(rusqlite::params![
                    digest,
                    serde_json::to_string(&sighting.tags)?,
                    sighting.quality.map(|q| q as f64),
                    pack(&sighting.vector),
                    sighting.model.as_deref(),
                    to_row_faces(&sighting.faces)?,
                ])?;
                forget.execute([&digest])?;
                for tag in &sighting.tags {
                    wear.execute(rusqlite::params![&digest, tag])?;
                }
                written += 1;
            }
        }
        self.conn.execute("COMMIT", [])?;
        Ok(written)
    }

    pub fn forget(&self, sources: &[PathBuf]) -> Result<usize> {
        self.scope(sources)?;
        self.conn.execute("BEGIN IMMEDIATE", [])?;
        let gone = self.conn.execute(
            "DELETE FROM sighting WHERE digest IN (SELECT digest FROM scope)",
            [],
        )?;
        self.conn.execute(
            "DELETE FROM worn WHERE digest IN (SELECT digest FROM scope)",
            [],
        )?;
        self.conn.execute("COMMIT", [])?;
        Ok(gone)
    }

    pub fn forget_missing(&self) -> Result<usize> {
        let paths: Vec<String> = {
            let mut query = self.conn.prepare("SELECT path FROM file")?;
            let rows = query.query_map([], |row| row.get::<_, String>(0))?;
            let mut held = Vec::new();
            for row in rows {
                held.push(row?);
            }
            held
        };

        let mut gone = 0usize;
        self.conn.execute("BEGIN IMMEDIATE", [])?;
        {
            let mut drop = self
                .conn
                .prepare_cached("DELETE FROM file WHERE path = ?1")?;
            for path in &paths {
                if !Path::new(path).exists() {
                    drop.execute([path])?;
                    gone += 1;
                }
            }
        }
        self.conn.execute("COMMIT", [])?;
        Ok(gone)
    }

    fn scope(&self, sources: &[PathBuf]) -> Result<()> {
        self.conn.execute(
            "CREATE TEMP TABLE IF NOT EXISTS scope (digest TEXT PRIMARY KEY)",
            [],
        )?;
        self.conn.execute("BEGIN IMMEDIATE", [])?;
        self.conn.execute("DELETE FROM scope", [])?;
        {
            let mut insert = self.conn.prepare_cached(
                "INSERT OR IGNORE INTO scope (digest) SELECT digest FROM file WHERE path = ?1",
            )?;
            for source in sources {
                insert.execute([key(source)])?;
            }
        }
        self.conn.execute("COMMIT", [])?;
        Ok(())
    }

    fn hang_up_missing_tags(&self) -> Result<usize> {
        let pending: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sighting WHERE digest NOT IN (SELECT digest FROM worn)
             AND tags <> '[]'",
            [],
            |row| row.get(0),
        )?;
        if pending == 0 {
            return Ok(0);
        }

        let listed: Vec<(String, Vec<String>)> = {
            let mut query = self.conn.prepare("SELECT digest, tags FROM sighting")?;
            let rows = query.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    from_row_tags(row.get::<_, String>(1)?),
                ))
            })?;
            let mut held = Vec::new();
            for row in rows {
                held.push(row?);
            }
            held
        };
        self.conn.execute("BEGIN IMMEDIATE", [])?;
        {
            let mut wear = self
                .conn
                .prepare_cached("INSERT OR IGNORE INTO worn (digest, tag) VALUES (?1, ?2)")?;
            for (digest, tags) in &listed {
                for tag in tags {
                    wear.execute(rusqlite::params![digest, tag])?;
                }
            }
        }
        self.conn.execute("COMMIT", [])?;
        Ok(pending as usize)
    }

    pub fn counts(&self) -> Result<Vec<(String, usize)>> {
        let mut query = self
            .conn
            .prepare("SELECT tag, COUNT(*) FROM worn GROUP BY tag ORDER BY COUNT(*) DESC, tag")?;
        let rows = query.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
        })?;

        let mut counted = Vec::new();
        for row in rows {
            let (tag, count) = row?;
            counted.push((tag, count));
        }
        Ok(counted)
    }

    pub fn forget_all(&self) -> Result<usize> {
        self.conn.execute("BEGIN IMMEDIATE", [])?;
        let gone = self.conn.execute("DELETE FROM sighting", [])?;
        self.conn.execute("DELETE FROM worn", [])?;
        self.conn.execute("COMMIT", [])?;
        Ok(gone)
    }

    pub fn listing(&self, sources: &[PathBuf]) -> Result<Vec<Listed>> {
        self.scope(sources)?;
        let mut query = self.conn.prepare(
            "SELECT file.path, sighting.tags, sighting.quality
             FROM sighting
             JOIN file ON file.digest = sighting.digest
             WHERE sighting.digest IN (SELECT digest FROM scope)",
        )?;
        let rows = query.query_map([], |row| {
            Ok((
                PathBuf::from(row.get::<_, String>(0)?),
                from_row_tags(row.get::<_, String>(1)?),
                row.get::<_, Option<f64>>(2)?.map(|q| q as f32),
            ))
        })?;

        let wanted: HashSet<String> = sources.iter().map(|source| key(source)).collect();
        let mut listed = Vec::new();
        for row in rows {
            let (path, tags, quality): Listed = row?;
            if wanted.contains(&key(&path)) {
                listed.push((path, tags, quality));
            }
        }
        Ok(listed)
    }

    pub fn search(
        &self,
        wanted: &[f32],
        words: &str,
        sources: &[PathBuf],
    ) -> Result<Vec<(PathBuf, f32)>> {
        self.scope(sources)?;
        let here: HashSet<String> = sources.iter().map(|source| key(source)).collect();
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

            let mut query = self.conn.prepare(
                "SELECT file.path, COUNT(DISTINCT worn.tag) FROM worn
                 JOIN file ON file.digest = worn.digest
                 WHERE worn.tag LIKE ?1 ESCAPE '#'
                   AND worn.digest IN (SELECT digest FROM scope)
                 GROUP BY file.path",
            )?;

            let mut tally: HashMap<PathBuf, usize> = HashMap::new();
            for needle in &needles {
                let mut rows = query.query([format!("%{}%", like_safe(needle))])?;
                while let Some(row) = rows.next()? {
                    let path = row.get::<_, String>(0)?;
                    if here.contains(&path) {
                        *tally.entry(PathBuf::from(path)).or_insert(0) += 1;
                    }
                }
            }

            for (source, matched) in tally {
                hits.push((source, matched as f32 / needles.len() as f32));
            }
        } else {
            let mut query = self.conn.prepare(
                "SELECT file.path, sighting.vector FROM sighting
                 JOIN file ON file.digest = sighting.digest
                 WHERE sighting.digest IN (SELECT digest FROM scope)",
            )?;
            let mut rows = query.query([])?;
            while let Some(row) = rows.next()? {
                let path = row.get::<_, String>(0)?;
                if !here.contains(&path) {
                    continue;
                }
                let vector = unpack(row.get::<_, Option<Vec<u8>>>(1)?.unwrap_or_default());
                let score = cosine(wanted, &vector);
                if score >= HIT_FLOOR {
                    hits.push((PathBuf::from(path), score));
                }
            }
        }

        hits.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        Ok(hits)
    }
}

fn like_safe(needle: &str) -> String {
    needle
        .replace('#', "##")
        .replace('%', "#%")
        .replace('_', "#_")
}

fn key(source: &Path) -> String {
    source.to_string_lossy().into_owned()
}

fn from_row_faces(raw: Option<String>) -> Option<Vec<Spot>> {
    serde_json::from_str(&raw?).ok()
}

fn to_row_faces(spots: &Option<Vec<Spot>>) -> Result<Option<String>> {
    match spots {
        Some(spots) => Ok(Some(serde_json::to_string(spots)?)),
        None => Ok(None),
    }
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

pub fn confidence(cosine: f32, scale: f32, bias: f32) -> f32 {
    let logit = cosine * scale + bias;
    1.0 / (1.0 + (-logit).exp())
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

    ranked.retain(|(_, score)| *score >= TAG_CONFIDENCE);
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

    use tempfile::{tempdir, TempDir};

    fn sighting(tags: &[&str], vector: Vec<f32>) -> Sighting {
        Sighting {
            tags: tags.iter().map(|t| t.to_string()).collect(),
            vector,
            ..Default::default()
        }
    }

    fn sighting_by(tags: &[&str], vector: Vec<f32>, model: &str) -> Sighting {
        Sighting {
            model: Some(model.to_string()),
            ..sighting(tags, vector)
        }
    }

    fn store() -> Store {
        Store::open(Path::new(":memory:")).unwrap()
    }

    fn shelf() -> (Store, TempDir) {
        (store(), tempdir().unwrap())
    }

    fn picture(dir: &TempDir, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn put(store: &Store, path: &Path, sighting: Sighting) -> String {
        let digest = store.identify(path).unwrap().unwrap();
        store
            .set_many(std::iter::once((digest.clone(), sighting)))
            .unwrap();
        digest
    }

    #[test]
    fn the_library_stands_apart_from_any_one_plan() {
        assert_eq!(
            library_path(Path::new("/data")),
            PathBuf::from("/data/sightings.db")
        );
        assert_eq!(
            store_path(Path::new("/plans/photos.jsonl")),
            PathBuf::from("/plans/photos.tags.db")
        );
        assert_eq!(
            sidecar_path(Path::new("/plans/photos.jsonl")),
            PathBuf::from("/plans/photos.tags.json")
        );
    }

    #[test]
    fn a_fresh_store_has_seen_nothing_yet() {
        let (store, dir) = shelf();
        let path = picture(&dir, "a.jpg", b"a");
        assert!(store.is_empty().unwrap());
        assert!(store.at(&path).unwrap().is_none());
        assert!(!store.holds("nothing").unwrap());
    }

    #[test]
    fn what_was_written_is_what_comes_back() {
        let (store, dir) = shelf();
        let path = picture(&dir, "a.jpg", b"a");
        put(&store, &path, sighting(&["a dog"], vec![0.5, 0.5]));

        let held = store.at(&path).unwrap().unwrap();
        assert_eq!(held.tags, vec!["a dog".to_string()]);
        assert_eq!(held.vector, vec![0.5, 0.5]);
        assert_eq!(held.quality, None);
        assert_eq!(store.len().unwrap(), 1);
    }

    #[test]
    fn a_score_survives_the_round_trip() {
        let (store, dir) = shelf();
        let path = picture(&dir, "b.jpg", b"b");
        let mut seen = sighting(&["a beach"], vec![1.0]);
        seen.quality = Some(6.25);
        put(&store, &path, seen);

        assert_eq!(store.at(&path).unwrap().unwrap().quality, Some(6.25));
    }

    #[test]
    fn the_same_picture_under_a_new_name_is_the_same_picture() {
        let (store, dir) = shelf();
        let first = picture(&dir, "a.jpg", b"the very same bytes");
        put(&store, &first, sighting(&["a dog"], vec![1.0, 0.0]));

        let moved = picture(&dir, "holiday.jpg", b"the very same bytes");
        let digest = store.identify(&moved).unwrap().unwrap();

        assert!(store.known().unwrap().looked(&digest));
        assert_eq!(
            store.at(&moved).unwrap().unwrap().tags,
            vec!["a dog".to_string()]
        );
        assert_eq!(store.len().unwrap(), 1);
    }

    #[test]
    fn a_copy_into_the_sorted_tree_arrives_already_tagged() {
        let (store, dir) = shelf();
        let source = picture(&dir, "a.jpg", b"one picture");
        put(&store, &source, sighting(&["a beach"], vec![0.5]));

        let sorted = dir.path().join("2019").join("07");
        std::fs::create_dir_all(&sorted).unwrap();
        let copied = sorted.join("a.jpg");
        std::fs::copy(&source, &copied).unwrap();

        let digest = store.identify(&copied).unwrap().unwrap();
        assert!(store.known().unwrap().looked(&digest));
        assert_eq!(
            store.at(&copied).unwrap().unwrap().tags,
            vec!["a beach".to_string()]
        );
    }

    #[test]
    fn a_file_that_changed_underneath_is_looked_at_again() {
        let (store, dir) = shelf();
        let path = picture(&dir, "a.jpg", b"before");
        let before = put(&store, &path, sighting(&["a dog"], vec![1.0]));

        std::fs::write(&path, b"after, and a different length").unwrap();
        let after = store.identify(&path).unwrap().unwrap();

        assert_ne!(before, after);
        assert!(!store.known().unwrap().looked(&after));
    }

    #[test]
    fn a_picture_that_is_not_there_cannot_be_named() {
        let (store, dir) = shelf();
        assert!(store
            .identify(&dir.path().join("gone.jpg"))
            .unwrap()
            .is_none());
    }

    #[test]
    fn narrowing_the_sources_does_not_throw_away_what_was_learned() {
        let (store, dir) = shelf();
        let kept = picture(&dir, "a.jpg", b"a");
        let left = picture(&dir, "b.jpg", b"b");
        put(&store, &kept, sighting(&["a dog"], vec![1.0]));
        let dropped = put(&store, &left, sighting(&["a beach"], vec![0.0]));

        assert_eq!(store.listing(std::slice::from_ref(&kept)).unwrap().len(), 1);
        assert!(store.holds(&dropped).unwrap());
        assert_eq!(store.len().unwrap(), 2);
    }

    #[test]
    fn a_picture_with_a_vector_counts_as_looked_at_but_not_as_rated() {
        let (store, dir) = shelf();
        let path = picture(&dir, "a.jpg", b"a");
        let digest = put(&store, &path, sighting(&["a dog"], vec![1.0, 0.0]));

        let known = store.known().unwrap();
        assert!(known.looked(&digest));
        assert!(!known.rated(&digest));
        assert!(!known.looked("nothing"));
    }

    #[test]
    fn a_rating_can_be_added_without_looking_again() {
        let (store, dir) = shelf();
        let path = picture(&dir, "a.jpg", b"a");
        let digest = put(&store, &path, sighting(&["a dog"], vec![1.0, 0.0]));

        assert_eq!(
            store
                .set_quality_many(std::iter::once((digest.clone(), Some(6.5))))
                .unwrap(),
            1
        );

        let held = store.at(&path).unwrap().unwrap();
        assert_eq!(held.quality, Some(6.5));
        assert_eq!(held.tags, vec!["a dog".to_string()]);
        assert_eq!(held.vector, vec![1.0, 0.0]);
        assert!(store.known().unwrap().rated(&digest));
    }

    #[test]
    fn a_picture_seen_but_never_rated_is_still_owed_a_rating() {
        let (store, dir) = shelf();
        let bare = picture(&dir, "a.jpg", b"a");
        let scored_at = picture(&dir, "b.jpg", b"b");
        let unrated = put(&store, &bare, sighting(&["a dog"], vec![1.0]));
        let mut scored = sighting(&["a cat"], vec![0.0]);
        scored.quality = Some(5.0);
        let rated = put(&store, &scored_at, scored);

        let known = store.known().unwrap();
        assert!(known.looked(&unrated) && !known.rated(&unrated));
        assert!(known.looked(&rated) && known.rated(&rated));
    }

    #[test]
    fn new_words_reach_old_pictures_without_the_model() {
        let (store, dir) = shelf();
        let one = picture(&dir, "a.jpg", b"a");
        let two = picture(&dir, "b.jpg", b"b");
        put(&store, &one, sighting(&["a dog"], vec![1.0, 0.0]));
        put(&store, &two, sighting(&["a dog"], vec![0.0, 1.0]));

        let done = store
            .reproject("later", &|vector| {
                if vector[0] > 0.5 {
                    vec!["a wolf".to_string()]
                } else {
                    vec!["a beach".to_string()]
                }
            })
            .unwrap();

        assert_eq!(done, 2);
        assert_eq!(store.at(&one).unwrap().unwrap().tags, ["a wolf"]);
        assert_eq!(store.at(&two).unwrap().unwrap().tags, ["a beach"]);
    }

    #[test]
    fn reprojecting_leaves_the_vectors_and_the_ratings_alone() {
        let (store, dir) = shelf();
        let path = picture(&dir, "a.jpg", b"a");
        let mut scored = sighting(&["a dog"], vec![0.25, 0.75]);
        scored.quality = Some(7.5);
        put(&store, &path, scored);

        store
            .reproject("later", &|_| vec!["a cat".to_string()])
            .unwrap();

        let held = store.at(&path).unwrap().unwrap();
        assert_eq!(held.vector, vec![0.25, 0.75]);
        assert_eq!(held.quality, Some(7.5));
        assert_eq!(held.tags, vec!["a cat".to_string()]);
    }

    #[test]
    fn the_tag_filter_follows_a_reprojection() {
        let (store, dir) = shelf();
        let path = picture(&dir, "a.jpg", b"a");
        put(&store, &path, sighting(&["a dog"], vec![1.0]));

        store
            .reproject("later", &|_| vec!["a wolf".to_string()])
            .unwrap();

        assert_eq!(store.counts().unwrap(), vec![("a wolf".to_string(), 1)]);
    }

    #[test]
    fn a_picture_with_no_vector_is_left_out_of_a_reprojection() {
        let (store, dir) = shelf();
        let path = picture(&dir, "a.jpg", b"a");
        put(&store, &path, sighting(&["a dog"], vec![]));

        assert_eq!(
            store
                .reproject("later", &|_| vec!["a wolf".to_string()])
                .unwrap(),
            0
        );
        assert_eq!(store.at(&path).unwrap().unwrap().tags, ["a dog"]);
    }

    #[test]
    fn the_words_are_reprojected_once_and_then_left_alone() {
        let store = store();
        let stamp = projection_stamp(&["a dog", "a cat"]);
        assert!(!store.projection_matches(&stamp).unwrap());

        store.reproject(&stamp, &|_| Vec::new()).unwrap();
        assert!(store.projection_matches(&stamp).unwrap());
        assert!(!store
            .projection_matches(&projection_stamp(&["a dog", "a cat", "a wolf"]))
            .unwrap());
    }

    #[test]
    fn the_same_words_always_stamp_the_same_way() {
        assert_eq!(
            projection_stamp(&["a dog", "a cat"]),
            projection_stamp(&["a dog", "a cat"])
        );
        assert_ne!(
            projection_stamp(&["a dog", "a cat"]),
            projection_stamp(&["a cat", "a dog"])
        );
    }

    #[test]
    fn looking_again_at_a_chosen_few_leaves_the_rest_standing() {
        let (store, dir) = shelf();
        let one = picture(&dir, "a.jpg", b"a");
        let two = picture(&dir, "b.jpg", b"b");
        put(&store, &one, sighting(&["a dog"], vec![1.0]));
        let spared = put(&store, &two, sighting(&["a beach"], vec![0.0]));

        assert_eq!(store.forget(std::slice::from_ref(&one)).unwrap(), 1);
        assert!(store.at(&one).unwrap().is_none());
        assert!(store.holds(&spared).unwrap());
        assert_eq!(store.counts().unwrap(), vec![("a beach".to_string(), 1)]);
    }

    #[test]
    fn a_path_that_no_longer_exists_is_let_go_of() {
        let (store, dir) = shelf();
        let path = picture(&dir, "a.jpg", b"a");
        let digest = put(&store, &path, sighting(&["a dog"], vec![1.0]));

        std::fs::remove_file(&path).unwrap();
        assert_eq!(store.forget_missing().unwrap(), 1);

        assert!(store.digest_of(&path).unwrap().is_none());
        assert!(store.holds(&digest).unwrap());
    }

    #[test]
    fn the_listing_answers_for_the_files_it_was_asked_about() {
        let (store, dir) = shelf();
        let path = picture(&dir, "a.jpg", b"a");
        let other = picture(&dir, "b.jpg", b"b");
        let mut seen = sighting(&["a dog"], vec![0.1, 0.2, 0.3]);
        seen.quality = Some(5.5);
        put(&store, &path, seen);
        put(&store, &other, sighting(&["a beach"], vec![0.0]));

        let listed = store.listing(std::slice::from_ref(&path)).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].0, path);
        assert_eq!(listed[0].1, vec!["a dog".to_string()]);
        assert_eq!(listed[0].2, Some(5.5));
    }

    #[test]
    fn a_sidecar_from_the_old_days_is_taken_in_once() {
        let (store, dir) = shelf();
        let path = picture(&dir, "a.jpg", b"a");
        let sidecar = dir.path().join("photos.tags.json");
        std::fs::write(
            &sidecar,
            format!(
                r#"{{{:?}:{{"tags":["a dog"],"vector":[0.5,0.5],"quality":6.25}}}}"#,
                path.to_string_lossy()
            ),
        )
        .unwrap();

        assert_eq!(store.take_in(&sidecar).unwrap(), 1);
        assert!(!sidecar.exists());
        assert!(sidecar.with_extension("json.inherited").exists());

        let digest = store.identify(&path).unwrap().unwrap();
        let taken = store
            .claim(&path, &digest, &|_| vec!["a wolf".to_string()])
            .unwrap()
            .unwrap();
        assert_eq!(taken.quality, Some(6.25));
        assert_eq!(taken.vector, vec![0.5, 0.5]);
        assert_eq!(store.at(&path).unwrap().unwrap().tags, ["a wolf"]);

        assert_eq!(store.take_in(&sidecar).unwrap(), 0);
    }

    #[test]
    fn a_plan_of_its_own_hands_its_sightings_to_the_library() {
        let (store, dir) = shelf();
        let path = picture(&dir, "a.jpg", b"a");
        let plan = dir.path().join("photos.jsonl");

        let old = Connection::open(store_path(&plan)).unwrap();
        old.execute(
            "CREATE TABLE sighting (source TEXT PRIMARY KEY, tags TEXT NOT NULL,
             quality REAL, vector BLOB)",
            [],
        )
        .unwrap();
        old.execute(
            "INSERT INTO sighting (source, tags, quality, vector) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![key(&path), r#"["a dog"]"#, 6.0f64, pack(&[0.25, 0.75])],
        )
        .unwrap();
        drop(old);

        assert_eq!(store.inherit(&plan).unwrap(), 1);
        assert!(store_path(&plan).with_extension("db.inherited").exists());

        let digest = store.identify(&path).unwrap().unwrap();
        let taken = store
            .claim(&path, &digest, &|_| vec!["a wolf".to_string()])
            .unwrap()
            .unwrap();
        assert_eq!(taken.vector, vec![0.25, 0.75]);
        assert_eq!(taken.quality, Some(6.0));
        assert!(store.known().unwrap().looked(&digest));
    }

    #[test]
    fn a_plan_store_that_cannot_be_read_does_not_stop_the_looking() {
        let (store, dir) = shelf();
        let plan = dir.path().join("photos.jsonl");
        std::fs::write(store_path(&plan), b"this was never a database").unwrap();

        assert_eq!(store.inherit(&plan).unwrap(), 0);
        assert!(store_path(&plan).exists());
    }

    #[test]
    fn there_is_nothing_to_claim_twice() {
        let (store, dir) = shelf();
        let path = picture(&dir, "a.jpg", b"a");
        let digest = store.identify(&path).unwrap().unwrap();

        assert!(store
            .claim(&path, &digest, &|_| Vec::new())
            .unwrap()
            .is_none());
    }

    #[test]
    fn a_damaged_sidecar_is_reported_rather_than_silently_emptied() {
        let (store, dir) = shelf();
        let sidecar = dir.path().join("photos.tags.json");
        std::fs::write(&sidecar, "{ this is not json").unwrap();

        assert!(store.take_in(&sidecar).is_err());
    }

    #[test]
    fn forgetting_everything_leaves_the_store_ready_to_look_again() {
        let (store, dir) = shelf();
        let one = picture(&dir, "a.jpg", b"a");
        let two = picture(&dir, "b.jpg", b"b");
        let digest = put(&store, &one, sighting(&["a dog"], vec![1.0]));
        put(&store, &two, sighting(&["a beach"], vec![0.0]));

        assert_eq!(store.forget_all().unwrap(), 2);
        assert!(store.is_empty().unwrap());
        assert!(store.counts().unwrap().is_empty());
        assert!(!store.holds(&digest).unwrap());

        put(&store, &one, sighting(&["a wolf"], vec![1.0]));
        assert_eq!(store.len().unwrap(), 1);
    }

    #[test]
    fn the_tags_are_counted_by_the_store_itself() {
        let (store, dir) = shelf();
        let one = picture(&dir, "a.jpg", b"a");
        let two = picture(&dir, "b.jpg", b"b");
        put(&store, &one, sighting(&["a dog", "a forest"], vec![1.0]));
        put(&store, &two, sighting(&["a dog"], vec![0.0]));

        assert_eq!(
            store.counts().unwrap(),
            vec![("a dog".to_string(), 2), ("a forest".to_string(), 1)]
        );
    }

    #[test]
    fn seeing_a_picture_again_takes_its_old_tags_off_the_shelf() {
        let (store, dir) = shelf();
        let path = picture(&dir, "a.jpg", b"a");
        put(&store, &path, sighting(&["a dog"], vec![1.0]));
        put(&store, &path, sighting(&["a wolf"], vec![1.0]));

        assert_eq!(store.counts().unwrap(), vec![("a wolf".to_string(), 1)]);
        assert!(store.search(&[], "dog", &[path]).unwrap().is_empty());
    }

    #[test]
    fn the_search_answers_only_for_the_files_it_was_asked_about() {
        let (store, dir) = shelf();
        let here = picture(&dir, "a.jpg", b"a");
        let elsewhere = picture(&dir, "b.jpg", b"b");
        put(&store, &here, sighting(&["a dog"], vec![1.0, 0.0]));
        put(&store, &elsewhere, sighting(&["a dog"], vec![1.0, 0.0]));

        let hits = store
            .search(&[], "dog", std::slice::from_ref(&here))
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, here);

        let close = store
            .search(&[1.0, 0.0], "", std::slice::from_ref(&here))
            .unwrap();
        assert_eq!(close.len(), 1);
        assert_eq!(close[0].0, here);
    }

    #[test]
    fn a_wildcard_in_the_question_is_not_a_wildcard() {
        let (store, dir) = shelf();
        let one = picture(&dir, "a.jpg", b"a");
        let two = picture(&dir, "b.jpg", b"b");
        put(&store, &one, sighting(&["a dog"], vec![1.0]));
        put(&store, &two, sighting(&["100% wool"], vec![0.0]));

        let scope = [one.clone(), two.clone()];
        let hits = store.search(&[], "%", &scope).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, two);
        assert_eq!(store.search(&[], "100%", &scope).unwrap().len(), 1);
    }

    #[test]
    fn a_store_written_before_the_shelf_existed_is_hung_up_on_opening() {
        let (store, dir) = shelf();
        let path = picture(&dir, "a.jpg", b"a");
        put(&store, &path, sighting(&["a dog"], vec![1.0]));
        store.conn.execute("DELETE FROM worn", []).unwrap();

        assert_eq!(store.hang_up_missing_tags().unwrap(), 1);
        assert_eq!(store.counts().unwrap(), vec![("a dog".to_string(), 1)]);
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
            top_tags(&vocabulary, &[0.9, 0.8, 0.0001]),
            vec!["forest", "dog"]
        );
    }

    #[test]
    fn a_tag_the_model_is_unsure_of_is_left_off() {
        let vocabulary = ["forest", "dog"];
        assert!(top_tags(&vocabulary, &[TAG_CONFIDENCE / 2.0, 0.0]).is_empty());
        assert_eq!(
            top_tags(&vocabulary, &[TAG_CONFIDENCE, 0.0]),
            vec!["forest"]
        );
    }

    #[test]
    fn a_confident_reading_is_kept_however_sure_the_best_one_was() {
        let vocabulary = ["forest", "dog"];
        assert_eq!(
            top_tags(&vocabulary, &[0.99, 0.05]),
            vec!["forest", "dog"],
            "a second answer is not dropped merely for being weaker"
        );
    }

    #[test]
    fn the_calibration_turns_a_similarity_into_a_probability() {
        assert!((confidence(0.0, 100.0, 0.0) - 0.5).abs() < 1e-6);
        assert!(confidence(1.0, 100.0, -10.0) > 0.99);
        assert!(confidence(-1.0, 100.0, -10.0) < 0.001);

        let low = confidence(0.02, 110.0, -12.0);
        let high = confidence(0.05, 110.0, -12.0);
        assert!(high > low, "{high} should beat {low}");
        assert!((0.0..=1.0).contains(&low));
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
        let (store, dir) = shelf();
        let far = picture(&dir, "far.jpg", b"far");
        let near = picture(&dir, "near.jpg", b"near");
        put(&store, &far, sighting(&[], vec![0.0, 1.0]));
        put(&store, &near, sighting(&[], vec![1.0, 0.0]));

        let hits = store.search(&[1.0, 0.0], "", &[far, near.clone()]).unwrap();
        assert_eq!(hits[0].0, near);
    }

    #[test]
    fn a_picture_can_be_the_question_instead_of_a_word() {
        let (store, dir) = shelf();
        let asked = picture(&dir, "asked.jpg", b"asked");
        let alike = picture(&dir, "alike.jpg", b"alike");
        let other = picture(&dir, "other.jpg", b"other");
        put(&store, &asked, sighting(&[], vec![1.0, 0.0]));
        put(&store, &alike, sighting(&[], vec![0.96, 0.28]));
        put(&store, &other, sighting(&[], vec![0.0, 1.0]));

        let held = store.at(&asked).unwrap().expect("the picture was stored");
        assert!(!held.vector.is_empty());

        let scope = [asked.clone(), alike.clone(), other];
        let hits = store.search(&held.vector, "", &scope).unwrap();
        let ranked: Vec<&PathBuf> = hits.iter().map(|(path, _)| path).collect();

        assert_eq!(
            ranked.first(),
            Some(&&asked),
            "a picture is its own best match"
        );
        assert_eq!(ranked.get(1), Some(&&alike));
    }

    #[test]
    fn a_picture_with_no_vector_cannot_be_the_question() {
        let (store, dir) = shelf();
        let bare = picture(&dir, "bare.jpg", b"bare");
        put(&store, &bare, sighting(&["a dog"], Vec::new()));

        let held = store.at(&bare).unwrap().expect("the picture was stored");
        assert!(held.vector.is_empty());
    }

    #[test]
    fn a_picture_of_something_else_entirely_is_left_out() {
        let (store, dir) = shelf();
        let other = picture(&dir, "other.jpg", b"other");
        put(&store, &other, sighting(&[], vec![-1.0, 0.0]));
        assert!(store.search(&[1.0, 0.0], "", &[other]).unwrap().is_empty());
    }

    #[test]
    fn without_a_model_the_words_are_matched_against_the_tags() {
        let (store, dir) = shelf();
        let both = picture(&dir, "both.jpg", b"both");
        let one = picture(&dir, "one.jpg", b"one");
        let none = picture(&dir, "none.jpg", b"none");
        put(&store, &both, sighting(&["forest", "dog"], vec![1.0]));
        put(&store, &one, sighting(&["forest"], vec![0.5]));
        put(&store, &none, sighting(&["city"], vec![0.0]));

        let hits = store
            .search(&[], "forest and dog", &[both.clone(), one, none])
            .unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].0, both);
        assert!(hits[0].1 > hits[1].1);
    }

    #[test]
    fn the_word_and_is_not_something_to_look_for() {
        let (store, dir) = shelf();
        let path = picture(&dir, "a.jpg", b"a");
        put(&store, &path, sighting(&["sandy beach"], vec![1.0]));
        assert!(store.search(&[], "and", &[path]).unwrap().is_empty());
    }

    #[test]
    fn an_empty_question_gets_an_empty_answer() {
        let (store, dir) = shelf();
        let path = picture(&dir, "a.jpg", b"a");
        put(&store, &path, sighting(&["dog"], vec![1.0]));
        assert!(store.search(&[], "   ", &[path]).unwrap().is_empty());
    }
    #[test]
    fn a_vector_remembers_which_model_and_revision_made_it() {
        let (store, dir) = shelf();
        let path = picture(&dir, "a.jpg", b"one picture");
        let digest = store.identify(&path).unwrap().unwrap();
        store
            .set_many(std::iter::once((
                digest.clone(),
                sighting_by(&["a dog"], vec![1.0, 0.0], "siglip@abc123def456"),
            )))
            .unwrap();

        let held = store.get(&digest).unwrap().unwrap();
        assert_eq!(held.model.as_deref(), Some("siglip@abc123def456"));
    }

    #[test]
    fn a_vector_from_another_model_does_not_count_as_looked_at() {
        let (store, dir) = shelf();
        let path = picture(&dir, "a.jpg", b"one picture");
        let digest = store.identify(&path).unwrap().unwrap();
        store
            .set_many(std::iter::once((
                digest.clone(),
                sighting_by(&["a dog"], vec![1.0, 0.0], "siglip@abc123def456"),
            )))
            .unwrap();

        let known = store.known().unwrap();
        assert!(known.looked(&digest));
        assert!(known.looked_with(&digest, "siglip@abc123def456"));
        assert!(!known.looked_with(&digest, "siglip@999999999999"));
        assert!(!known.looked_with(&digest, "something-else@abc123def456"));
    }

    #[test]
    fn a_vector_from_before_the_model_was_recorded_is_looked_at_again() {
        let (store, dir) = shelf();
        let path = picture(&dir, "a.jpg", b"one picture");
        let digest = store.identify(&path).unwrap().unwrap();
        put(&store, &path, sighting(&["a dog"], vec![1.0, 0.0]));

        let known = store.known().unwrap();
        assert!(known.looked(&digest));
        assert!(!known.looked_with(&digest, "siglip@abc123def456"));
    }

    #[test]
    fn a_library_written_before_the_model_column_still_opens() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("old.db");
        {
            let old = Connection::open(&path).unwrap();
            old.execute(
                "CREATE TABLE sighting (digest TEXT PRIMARY KEY, tags TEXT NOT NULL, quality REAL, vector BLOB)",
                [],
            )
            .unwrap();
            old.execute(
                "INSERT INTO sighting (digest, tags, quality, vector) VALUES ('abc', '[\"a dog\"]', NULL, NULL)",
                [],
            )
            .unwrap();
        }

        let store = Store::open(&path).unwrap();
        let held = store.get("abc").unwrap().unwrap();
        assert_eq!(held.tags, vec!["a dog".to_string()]);
        assert_eq!(held.model, None);
    }

    fn spot(x: f32, y: f32) -> Spot {
        Spot {
            x,
            y,
            width: 0.1,
            height: 0.1,
            score: 0.9,
            label: None,
        }
    }

    fn found(x: f32, y: f32) -> Found {
        Found {
            spot: spot(x, y),
            vector: vec![x, y, 1.0 - x],
        }
    }

    #[test]
    fn a_picture_nobody_looked_at_has_no_answer_about_faces() {
        let store = store();
        store
            .set_many(std::iter::once((
                "abc".into(),
                sighting(&["a dog"], vec![]),
            )))
            .unwrap();
        assert_eq!(store.faces_of("abc").unwrap(), None);
        assert!(!store.known().unwrap().faced("abc"));
    }

    #[test]
    fn a_picture_looked_at_with_nothing_in_it_is_not_the_same_as_unlooked() {
        let store = store();
        store
            .set_faces_many(std::iter::once(("abc".into(), vec![])))
            .unwrap();
        assert_eq!(store.faces_of("abc").unwrap(), Some(vec![]));
        assert!(
            store.known().unwrap().faced("abc"),
            "an empty answer is still an answer"
        );
    }

    #[test]
    fn faces_are_found_without_the_picture_ever_being_tagged() {
        let store = store();
        store
            .set_faces_many(std::iter::once(("abc".into(), vec![found(0.1, 0.2)])))
            .unwrap();
        let held = store.get("abc").unwrap().expect("a row should exist");
        assert_eq!(held.faces, Some(vec![spot(0.1, 0.2)]));
        assert!(held.tags.is_empty(), "no tags were ever asked for");
    }

    #[test]
    fn tagging_a_picture_later_does_not_lose_the_faces() {
        let store = store();
        store
            .set_faces_many(std::iter::once(("abc".into(), vec![found(0.3, 0.4)])))
            .unwrap();
        store
            .set_many(std::iter::once((
                "abc".into(),
                sighting(&["a dog"], vec![1.0, 0.0]),
            )))
            .unwrap();

        let held = store.get("abc").unwrap().unwrap();
        assert_eq!(held.tags, vec!["a dog".to_string()]);
        assert_eq!(
            held.faces,
            Some(vec![spot(0.3, 0.4)]),
            "the tagging pass wrote over the faces"
        );
    }

    #[test]
    fn looking_again_replaces_what_was_found_before() {
        let store = store();
        store
            .set_faces_many(std::iter::once(("abc".into(), vec![found(0.1, 0.1)])))
            .unwrap();
        store
            .set_faces_many(std::iter::once((
                "abc".into(),
                vec![found(0.5, 0.5), found(0.6, 0.6)],
            )))
            .unwrap();
        assert_eq!(store.faces_of("abc").unwrap().unwrap().len(), 2);
    }

    #[test]
    fn faces_ride_along_in_the_sidecar() {
        let (store, dir) = shelf();
        let plan = dir.path().join("plan.jsonl");
        let source = dir.path().join("picture.jpg");

        let mut held = Tags::default();
        held.set(
            source.clone(),
            Sighting {
                faces: Some(vec![spot(0.2, 0.2)]),
                ..sighting(&["a dog"], vec![1.0, 0.0])
            },
        );
        std::fs::write(sidecar_path(&plan), serde_json::to_string(&held).unwrap()).unwrap();

        store.inherit(&plan).unwrap();
        let claimed = store
            .claim(&source, "abc", &|_| vec!["a dog".to_string()])
            .unwrap()
            .expect("the sidecar entry should be claimed");
        assert_eq!(claimed.faces, Some(vec![spot(0.2, 0.2)]));
        assert_eq!(store.faces_of("abc").unwrap(), Some(vec![spot(0.2, 0.2)]));
    }

    #[test]
    fn a_store_made_before_faces_existed_still_opens() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("old.db");
        {
            let old = Connection::open(&path).unwrap();
            old.execute(
                "CREATE TABLE sighting (digest TEXT PRIMARY KEY, tags TEXT NOT NULL, quality REAL, vector BLOB)",
                [],
            )
            .unwrap();
            old.execute(
                "INSERT INTO sighting (digest, tags) VALUES ('abc', '[\"a dog\"]')",
                [],
            )
            .unwrap();
        }

        let store = Store::open(&path).unwrap();
        assert_eq!(store.faces_of("abc").unwrap(), None);
        store
            .set_faces_many(std::iter::once(("abc".into(), vec![found(0.1, 0.1)])))
            .unwrap();
        assert_eq!(store.faces_of("abc").unwrap().unwrap().len(), 1);
    }

    fn facing(vector: Vec<f32>) -> Found {
        Found {
            spot: spot(0.1, 0.1),
            vector,
        }
    }

    #[test]
    fn a_face_keeps_its_vector_alongside_its_box() {
        let store = store();
        store
            .set_faces_many(std::iter::once((
                "abc".into(),
                vec![facing(vec![1.0, 0.0]), facing(vec![0.0, 1.0])],
            )))
            .unwrap();

        let held = store.portraits_of("abc").unwrap();
        assert_eq!(held.len(), 2);
        assert_eq!(held[0].vector, vec![1.0, 0.0]);
        assert_eq!(held[1].ord, 1);
        assert!(held[0].spot.label.is_none());
    }

    #[test]
    fn naming_a_face_shows_up_in_what_the_picture_reports() {
        let store = store();
        store
            .set_faces_many(std::iter::once((
                "abc".into(),
                vec![facing(vec![1.0, 0.0])],
            )))
            .unwrap();
        store.name_face("abc", 0, Some("Anna")).unwrap();

        assert_eq!(
            store.portraits_of("abc").unwrap()[0].spot.label.as_deref(),
            Some("Anna")
        );
        let drawn = store.faces_of("abc").unwrap().unwrap();
        assert_eq!(
            drawn[0].label.as_deref(),
            Some("Anna"),
            "the drawing blob did not follow the name"
        );
    }

    #[test]
    fn a_name_can_be_taken_off_again() {
        let store = store();
        store
            .set_faces_many(std::iter::once((
                "abc".into(),
                vec![facing(vec![1.0, 0.0])],
            )))
            .unwrap();
        store.name_face("abc", 0, Some("Anna")).unwrap();
        store.name_face("abc", 0, None).unwrap();
        assert!(store.portraits_of("abc").unwrap()[0].spot.label.is_none());
        assert!(store.names().unwrap().is_empty());
    }

    #[test]
    fn the_names_are_counted_by_how_often_they_appear() {
        let store = store();
        for (digest, vector) in [
            ("a", vec![1.0, 0.0]),
            ("b", vec![1.0, 0.0]),
            ("c", vec![0.0, 1.0]),
        ] {
            store
                .set_faces_many(std::iter::once((digest.into(), vec![facing(vector)])))
                .unwrap();
        }
        store.name_face("a", 0, Some("Anna")).unwrap();
        store.name_face("b", 0, Some("Anna")).unwrap();
        store.name_face("c", 0, Some("Bo")).unwrap();

        assert_eq!(
            store.names().unwrap(),
            vec![("Anna".to_string(), 2), ("Bo".to_string(), 1)]
        );
    }

    #[test]
    fn naming_everything_alike_reaches_the_close_ones_only() {
        let store = store();
        store
            .set_faces_many(
                [
                    ("near".to_string(), vec![facing(vec![0.99, 0.14])]),
                    ("far".to_string(), vec![facing(vec![0.0, 1.0])]),
                ]
                .into_iter(),
            )
            .unwrap();

        let touched = store
            .name_everything_like(&[1.0, 0.0], "Anna", 0.9)
            .unwrap();
        assert_eq!(touched, 1, "only the close face should be named");
        assert_eq!(
            store.portraits_of("near").unwrap()[0].spot.label.as_deref(),
            Some("Anna")
        );
        assert!(store.portraits_of("far").unwrap()[0].spot.label.is_none());
    }

    #[test]
    fn naming_everything_alike_never_writes_over_a_name_already_there() {
        let store = store();
        store
            .set_faces_many(std::iter::once((
                "abc".into(),
                vec![facing(vec![1.0, 0.0])],
            )))
            .unwrap();
        store.name_face("abc", 0, Some("Bo")).unwrap();

        let touched = store
            .name_everything_like(&[1.0, 0.0], "Anna", 0.5)
            .unwrap();
        assert_eq!(touched, 0, "a face already named must be left alone");
        assert_eq!(
            store.portraits_of("abc").unwrap()[0].spot.label.as_deref(),
            Some("Bo")
        );
    }

    #[test]
    fn looking_again_at_a_picture_clears_the_faces_it_had() {
        let store = store();
        store
            .set_faces_many(std::iter::once((
                "abc".into(),
                vec![facing(vec![1.0, 0.0]), facing(vec![0.0, 1.0])],
            )))
            .unwrap();
        store
            .set_faces_many(std::iter::once((
                "abc".into(),
                vec![facing(vec![1.0, 0.0])],
            )))
            .unwrap();
        assert_eq!(
            store.portraits_of("abc").unwrap().len(),
            1,
            "a stale row survived"
        );
    }

    #[test]
    fn forgetting_faces_leaves_the_picture_waiting_to_be_looked_at_again() {
        let store = store();
        store
            .set_faces_many(
                vec![
                    ("abc".to_string(), vec![facing(vec![1.0, 0.0])]),
                    ("def".to_string(), vec![facing(vec![0.0, 1.0])]),
                ]
                .into_iter(),
            )
            .unwrap();

        assert_eq!(store.forget_faces(&["abc".to_string()]).unwrap(), 1);

        assert!(store.portraits_of("abc").unwrap().is_empty());
        assert!(!store.known().unwrap().faced("abc"));
        assert_eq!(store.portraits_of("def").unwrap().len(), 1);
        assert!(store.known().unwrap().faced("def"));
    }

    #[test]
    fn forgetting_a_picture_that_was_never_looked_at_changes_nothing() {
        let store = store();
        assert_eq!(store.forget_faces(&["nobody".to_string()]).unwrap(), 0);
    }
}
