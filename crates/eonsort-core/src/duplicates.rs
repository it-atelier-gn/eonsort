use crate::error::{Error, Result};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub bytes: u64,
    pub wasted: u64,
    pub sources: Vec<PathBuf>,
}

pub fn exact(files: &[(PathBuf, u64)], cancel: &AtomicBool) -> Result<Vec<DuplicateGroup>> {
    let mut by_size: HashMap<u64, Vec<&PathBuf>> = HashMap::new();
    for (source, size) in files.iter().filter(|(_, size)| *size > 0) {
        by_size.entry(*size).or_default().push(source);
    }

    let candidates: Vec<(u64, Vec<&PathBuf>)> = by_size
        .into_iter()
        .filter(|(_, sources)| sources.len() > 1)
        .collect();

    let hashed: Vec<Result<Vec<DuplicateGroup>>> = candidates
        .into_par_iter()
        .map(|(size, sources)| {
            if cancel.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            Ok(by_content(size, &sources))
        })
        .collect();

    let mut groups = Vec::new();
    for found in hashed {
        groups.extend(found?);
    }

    groups.sort_by(|a, b| {
        b.wasted
            .cmp(&a.wasted)
            .then_with(|| a.sources.first().cmp(&b.sources.first()))
    });
    Ok(groups)
}

fn by_content(size: u64, sources: &[&PathBuf]) -> Vec<DuplicateGroup> {
    let mut by_hash: HashMap<String, Vec<PathBuf>> = HashMap::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for source in sources {
        if !seen.insert(same_file(source)) {
            continue;
        }
        let Some(digest) = hash(source) else { continue };
        by_hash.entry(digest).or_default().push((*source).clone());
    }

    by_hash
        .into_values()
        .filter(|sources| sources.len() > 1)
        .map(|mut sources| {
            sources.sort();
            DuplicateGroup {
                wasted: size * (sources.len() as u64 - 1),
                bytes: size,
                sources,
            }
        })
        .collect()
}

fn hash(path: &Path) -> Option<String> {
    crate::tags::digest(path)
}

pub fn same_file(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Removable,
    Vanished,
    Changed,
    TheKeeperItself,
}

impl Verdict {
    pub fn describe(&self) -> &'static str {
        match self {
            Verdict::Removable => "an identical copy is still there",
            Verdict::Vanished => "it is no longer on disk",
            Verdict::Changed => "it no longer holds the same bytes as the copy that is kept",
            Verdict::TheKeeperItself => "it is the copy that is kept, reached by another path",
        }
    }
}

pub fn look_again(keeper: &Path, extra: &Path) -> Verdict {
    if same_file(keeper) == same_file(extra) {
        return Verdict::TheKeeperItself;
    }

    let (Ok(kept), Ok(going)) = (keeper.metadata(), extra.metadata()) else {
        return Verdict::Vanished;
    };
    if !kept.is_file() || !going.is_file() {
        return Verdict::Vanished;
    }
    if kept.len() != going.len() {
        return Verdict::Changed;
    }

    let (Some(one), Some(two)) = (hash(keeper), hash(extra)) else {
        return Verdict::Vanished;
    };
    if one == two {
        Verdict::Removable
    } else {
        Verdict::Changed
    }
}

pub fn wasted(groups: &[DuplicateGroup]) -> u64 {
    groups.iter().map(|group| group.wasted).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn files(dir: &Path, named: &[(&str, &[u8])]) -> Vec<(PathBuf, u64)> {
        named
            .iter()
            .map(|(name, body)| {
                let path = dir.join(name);
                fs::write(&path, body).unwrap();
                (path, body.len() as u64)
            })
            .collect()
    }

    #[test]
    fn finds_files_that_hold_the_same_bytes() {
        let dir = tempdir().unwrap();
        let listed = files(
            dir.path(),
            &[
                ("a.jpg", b"same picture"),
                ("b.jpg", b"same picture"),
                ("c.jpg", b"another one!"),
            ],
        );

        let groups = exact(&listed, &AtomicBool::new(false)).unwrap();

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].sources.len(), 2);
        assert_eq!(groups[0].bytes, 12);
        assert_eq!(groups[0].wasted, 12);
        assert_eq!(wasted(&groups), 12);
    }

    #[test]
    fn files_of_the_same_length_are_not_taken_for_each_other() {
        let dir = tempdir().unwrap();
        let listed = files(dir.path(), &[("a.jpg", b"aaaaa"), ("b.jpg", b"bbbbb")]);

        assert!(exact(&listed, &AtomicBool::new(false)).unwrap().is_empty());
    }

    #[test]
    fn empty_files_are_not_reported_as_copies_of_each_other() {
        let dir = tempdir().unwrap();
        let listed = files(dir.path(), &[("a.jpg", b""), ("b.jpg", b"")]);

        assert!(exact(&listed, &AtomicBool::new(false)).unwrap().is_empty());
    }

    #[test]
    fn the_heaviest_group_is_reported_first() {
        let dir = tempdir().unwrap();
        let listed = files(
            dir.path(),
            &[
                ("small1.jpg", b"tiny"),
                ("small2.jpg", b"tiny"),
                ("big1.jpg", b"a much larger picture"),
                ("big2.jpg", b"a much larger picture"),
                ("big3.jpg", b"a much larger picture"),
            ],
        );

        let groups = exact(&listed, &AtomicBool::new(false)).unwrap();

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].sources.len(), 3);
        assert_eq!(groups[0].wasted, 42);
        assert_eq!(groups[1].wasted, 4);
    }

    #[test]
    fn a_cancelled_search_stops_rather_than_reporting_half_of_it() {
        let dir = tempdir().unwrap();
        let listed = files(dir.path(), &[("a.jpg", b"same"), ("b.jpg", b"same")]);

        let cancel = AtomicBool::new(true);
        assert!(matches!(exact(&listed, &cancel), Err(Error::Cancelled)));
    }

    #[test]
    fn one_file_listed_twice_is_not_a_copy_of_itself() {
        let dir = tempdir().unwrap();
        let listed = files(dir.path(), &[("a.jpg", b"same picture")]);
        let twice = vec![listed[0].clone(), listed[0].clone()];

        assert!(exact(&twice, &AtomicBool::new(false)).unwrap().is_empty());
    }

    #[test]
    fn one_path_reached_two_ways_is_not_a_copy_of_itself() {
        let dir = tempdir().unwrap();
        let deep = dir.path().join("2023");
        fs::create_dir_all(&deep).unwrap();
        let listed = files(&deep, &[("a.jpg", b"same picture")]);
        let roundabout = dir.path().join(".").join("2023").join("a.jpg");
        let twice = vec![listed[0].clone(), (roundabout, 12)];

        assert!(exact(&twice, &AtomicBool::new(false)).unwrap().is_empty());
    }

    #[test]
    fn a_real_copy_beside_a_path_listed_twice_still_reports_one_extra() {
        let dir = tempdir().unwrap();
        let listed = files(
            dir.path(),
            &[("a.jpg", b"same picture"), ("b.jpg", b"same picture")],
        );
        let listing = vec![listed[0].clone(), listed[0].clone(), listed[1].clone()];

        let groups = exact(&listing, &AtomicBool::new(false)).unwrap();

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].sources.len(), 2);
        assert_eq!(groups[0].wasted, 12);
    }

    #[test]
    fn a_copy_still_holding_the_same_bytes_may_go() {
        let dir = tempdir().unwrap();
        let listed = files(
            dir.path(),
            &[("a.jpg", b"same picture"), ("b.jpg", b"same picture")],
        );

        assert_eq!(look_again(&listed[0].0, &listed[1].0), Verdict::Removable);
    }

    #[test]
    fn a_copy_rewritten_since_the_search_stays() {
        let dir = tempdir().unwrap();
        let listed = files(
            dir.path(),
            &[("a.jpg", b"same picture"), ("b.jpg", b"same picture")],
        );
        fs::write(&listed[1].0, b"different!!!").unwrap();

        assert_eq!(look_again(&listed[0].0, &listed[1].0), Verdict::Changed);
    }

    #[test]
    fn a_copy_of_another_length_stays_without_reading_it() {
        let dir = tempdir().unwrap();
        let listed = files(
            dir.path(),
            &[("a.jpg", b"same picture"), ("b.jpg", b"same picture")],
        );
        fs::write(&listed[1].0, b"longer than it was").unwrap();

        assert_eq!(look_again(&listed[0].0, &listed[1].0), Verdict::Changed);
    }

    #[test]
    fn a_keeper_that_went_away_takes_the_whole_group_with_it() {
        let dir = tempdir().unwrap();
        let listed = files(
            dir.path(),
            &[("a.jpg", b"same picture"), ("b.jpg", b"same picture")],
        );
        fs::remove_file(&listed[0].0).unwrap();

        assert_eq!(look_again(&listed[0].0, &listed[1].0), Verdict::Vanished);
    }

    #[test]
    fn the_keeper_reached_by_another_path_is_never_removable() {
        let dir = tempdir().unwrap();
        let listed = files(dir.path(), &[("a.jpg", b"same picture")]);
        let roundabout = dir.path().join(".").join("a.jpg");

        assert_eq!(
            look_again(&listed[0].0, &roundabout),
            Verdict::TheKeeperItself
        );
    }

    #[test]
    fn a_file_that_cannot_be_read_is_skipped_rather_than_fatal() {
        let dir = tempdir().unwrap();
        let mut listed = files(dir.path(), &[("a.jpg", b"same"), ("b.jpg", b"same")]);
        listed.push((dir.path().join("gone.jpg"), 4));

        let groups = exact(&listed, &AtomicBool::new(false)).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].sources.len(), 2);
    }
}
