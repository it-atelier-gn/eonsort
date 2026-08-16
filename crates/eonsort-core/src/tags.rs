use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

pub const MAX_TAGS: usize = 6;
pub const TAG_FLOOR: f32 = 0.14;
pub const HIT_FLOOR: f32 = 0.02;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sighting {
    pub tags: Vec<String>,
    #[serde(default)]
    pub vector: Vec<f32>,
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

    pub fn keep_only(&mut self, wanted: &[PathBuf]) {
        self.0.retain(|source, _| wanted.contains(source));
    }
}

pub fn tags_path(plan_path: &Path) -> PathBuf {
    plan_path.with_extension("tags.json")
}

pub fn read(path: &Path) -> Result<Tags> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Tags::default()),
        Err(e) => return Err(Error::io(path, e)),
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    if text.trim().is_empty() {
        return Ok(Tags::default());
    }
    serde_json::from_str(text).map_err(Error::from)
}

pub fn write(path: &Path, tags: &Tags) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let file = File::create(path).map_err(|e| Error::io(path, e))?;
    serde_json::to_writer(BufWriter::new(file), tags).map_err(Error::from)
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

pub fn search(tags: &Tags, wanted: &[f32], words: &str) -> Vec<(PathBuf, f32)> {
    let mut hits: Vec<(PathBuf, f32)> = if wanted.is_empty() {
        let needles: Vec<String> = words
            .split_whitespace()
            .map(|w| w.to_lowercase())
            .filter(|w| !w.is_empty() && w != "and" && w != "or")
            .collect();
        if needles.is_empty() {
            return Vec::new();
        }
        tags.0
            .iter()
            .filter_map(|(source, sighting)| {
                let matched = needles
                    .iter()
                    .filter(|needle| {
                        sighting
                            .tags
                            .iter()
                            .any(|tag| tag.contains(needle.as_str()))
                    })
                    .count();
                (matched > 0).then(|| (source.clone(), matched as f32 / needles.len() as f32))
            })
            .collect()
    } else {
        tags.0
            .iter()
            .filter_map(|(source, sighting)| {
                let score = cosine(wanted, &sighting.vector);
                (score >= HIT_FLOOR).then(|| (source.clone(), score))
            })
            .collect()
    };

    hits.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sighting(tags: &[&str], vector: Vec<f32>) -> Sighting {
        Sighting {
            tags: tags.iter().map(|t| t.to_string()).collect(),
            vector,
        }
    }

    #[test]
    fn the_sidecar_sits_beside_the_plan() {
        let path = tags_path(Path::new("/photos/plan.jsonl"));
        assert!(path.ends_with("plan.tags.json"));
    }

    #[test]
    fn a_missing_sidecar_reads_as_nothing_seen_yet() {
        let path = std::env::temp_dir().join("eonsort-tags-absent.json");
        let _ = std::fs::remove_file(&path);
        assert!(read(&path).unwrap().is_empty());
    }

    #[test]
    fn what_was_written_is_what_comes_back() {
        let path = std::env::temp_dir().join("eonsort-tags-roundtrip.json");
        let mut tags = Tags::default();
        tags.set(
            PathBuf::from("/photos/a.jpg"),
            sighting(&["forest", "dog"], vec![0.6, 0.8]),
        );
        write(&path, &tags).unwrap();

        let back = read(&path).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(
            back.get(Path::new("/photos/a.jpg")).unwrap().tags,
            vec!["forest", "dog"]
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_byte_order_mark_does_not_defeat_it() {
        let path = std::env::temp_dir().join("eonsort-tags-bom.json");
        std::fs::write(&path, "\u{feff}{}").unwrap();
        assert!(read(&path).unwrap().is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_damaged_sidecar_is_reported_rather_than_silently_emptied() {
        let path = std::env::temp_dir().join("eonsort-tags-damaged.json");
        std::fs::write(&path, "{ this is not json").unwrap();
        assert!(read(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn forgetting_files_that_left_the_plan() {
        let mut tags = Tags::default();
        tags.set(PathBuf::from("/a.jpg"), sighting(&["dog"], vec![]));
        tags.set(PathBuf::from("/b.jpg"), sighting(&["cat"], vec![]));
        tags.keep_only(&[PathBuf::from("/a.jpg")]);
        assert_eq!(tags.len(), 1);
        assert!(tags.get(Path::new("/a.jpg")).is_some());
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
    fn the_strongest_tags_come_back_in_order() {
        let vocabulary = ["forest", "dog", "beach", "snow"];
        let tags = top_tags(&vocabulary, &[0.9, 0.7, 0.01, 0.02]);
        assert_eq!(tags, vec!["forest", "dog"]);
    }

    #[test]
    fn a_picture_of_nothing_in_the_vocabulary_gets_no_tags() {
        let vocabulary = ["forest", "dog"];
        assert!(top_tags(&vocabulary, &[0.0, -0.2]).is_empty());
    }

    #[test]
    fn no_more_than_a_handful_of_tags_per_picture() {
        let vocabulary = ["a", "b", "c", "d", "e", "f", "g", "h"];
        let tags = top_tags(&vocabulary, &[0.9; 8]);
        assert_eq!(tags.len(), MAX_TAGS);
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
        let mut tags = Tags::default();
        tags.set(PathBuf::from("/far.jpg"), sighting(&[], vec![0.0, 1.0]));
        tags.set(PathBuf::from("/near.jpg"), sighting(&[], vec![1.0, 0.0]));

        let hits = search(&tags, &[1.0, 0.0], "");
        assert_eq!(hits[0].0, PathBuf::from("/near.jpg"));
    }

    #[test]
    fn a_picture_of_something_else_entirely_is_left_out() {
        let mut tags = Tags::default();
        tags.set(PathBuf::from("/other.jpg"), sighting(&[], vec![-1.0, 0.0]));
        assert!(search(&tags, &[1.0, 0.0], "").is_empty());
    }

    #[test]
    fn without_a_model_the_words_are_matched_against_the_tags() {
        let mut tags = Tags::default();
        tags.set(
            PathBuf::from("/both.jpg"),
            sighting(&["forest", "dog"], vec![]),
        );
        tags.set(PathBuf::from("/one.jpg"), sighting(&["forest"], vec![]));
        tags.set(PathBuf::from("/none.jpg"), sighting(&["city"], vec![]));

        let hits = search(&tags, &[], "forest and dog");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].0, PathBuf::from("/both.jpg"));
        assert!(hits[0].1 > hits[1].1);
    }

    #[test]
    fn the_word_and_is_not_something_to_look_for() {
        let mut tags = Tags::default();
        tags.set(PathBuf::from("/a.jpg"), sighting(&["sandy beach"], vec![]));
        assert!(search(&tags, &[], "and").is_empty());
    }

    #[test]
    fn an_empty_question_gets_an_empty_answer() {
        let mut tags = Tags::default();
        tags.set(PathBuf::from("/a.jpg"), sighting(&["dog"], vec![]));
        assert!(search(&tags, &[], "   ").is_empty());
    }
}
