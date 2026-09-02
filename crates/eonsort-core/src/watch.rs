use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const DEFAULT_INTERVAL_SECONDS: u64 = 60;
pub const DEFAULT_SETTLE_SECONDS: u64 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchOptions {
    pub interval: Duration,
    pub settle: Duration,
}

impl Default for WatchOptions {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(DEFAULT_INTERVAL_SECONDS),
            settle: Duration::from_secs(DEFAULT_SETTLE_SECONDS),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Seen {
    size: u64,
    at: SystemTime,
}

#[derive(Debug, Clone, Default)]
pub struct Pending {
    waiting: HashMap<PathBuf, Seen>,
    done: HashMap<PathBuf, u64>,
}

impl Pending {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }

    pub fn settled(&self) -> usize {
        self.done.len()
    }

    pub fn forget(&mut self, source: &Path) {
        self.waiting.remove(source);
        self.done.remove(source);
    }

    pub fn offer(&mut self, source: &Path, size: u64, now: SystemTime, settle: Duration) -> bool {
        if self.done.get(source) == Some(&size) {
            return false;
        }

        match self.waiting.get(source).copied() {
            Some(previous) if previous.size == size => {
                let waited = now
                    .duration_since(previous.at)
                    .unwrap_or(Duration::from_secs(0));
                if waited < settle {
                    return false;
                }
                self.waiting.remove(source);
                self.done.insert(source.to_path_buf(), size);
                true
            }
            _ => {
                self.waiting
                    .insert(source.to_path_buf(), Seen { size, at: now });
                false
            }
        }
    }

    pub fn ready(
        &mut self,
        found: &[(PathBuf, u64)],
        now: SystemTime,
        settle: Duration,
    ) -> Vec<PathBuf> {
        let mut ready: Vec<PathBuf> = found
            .iter()
            .filter(|(source, size)| self.offer(source, *size, now, settle))
            .map(|(source, _)| source.clone())
            .collect();

        let present: std::collections::HashSet<&PathBuf> =
            found.iter().map(|(source, _)| source).collect();
        self.waiting.retain(|source, _| present.contains(source));

        ready.sort();
        ready
    }
}

pub fn sweep(sources: &[PathBuf], follow_symlinks: bool) -> Vec<(PathBuf, u64)> {
    let mut found = Vec::new();
    for root in sources {
        for entry in walkdir::WalkDir::new(root)
            .follow_links(follow_symlinks)
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            found.push((entry.path().to_path_buf(), meta.len()));
        }
    }
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn at(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000 + seconds)
    }

    fn settle() -> Duration {
        Duration::from_secs(10)
    }

    fn file(name: &str, size: u64) -> (PathBuf, u64) {
        (PathBuf::from(format!("/watched/{name}")), size)
    }

    #[test]
    fn a_file_is_not_taken_the_moment_it_appears() {
        let mut pending = Pending::new();
        let found = vec![file("a.jpg", 100)];

        assert!(pending.ready(&found, at(0), settle()).is_empty());
        assert_eq!(pending.waiting(), 1);
    }

    #[test]
    fn a_file_that_has_stopped_growing_is_taken_once_it_has_settled() {
        let mut pending = Pending::new();
        let found = vec![file("a.jpg", 100)];

        assert!(pending.ready(&found, at(0), settle()).is_empty());
        assert!(pending.ready(&found, at(5), settle()).is_empty());

        let ready = pending.ready(&found, at(11), settle());
        assert_eq!(ready, vec![PathBuf::from("/watched/a.jpg")]);
        assert_eq!(pending.settled(), 1);
    }

    #[test]
    fn a_file_still_being_written_keeps_its_clock_reset() {
        let mut pending = Pending::new();

        assert!(pending
            .ready(&[file("a.jpg", 100)], at(0), settle())
            .is_empty());
        assert!(pending
            .ready(&[file("a.jpg", 900)], at(9), settle())
            .is_empty());
        assert!(pending
            .ready(&[file("a.jpg", 900)], at(12), settle())
            .is_empty());

        let ready = pending.ready(&[file("a.jpg", 900)], at(20), settle());
        assert_eq!(ready.len(), 1);
    }

    #[test]
    fn a_file_already_taken_is_not_taken_again() {
        let mut pending = Pending::new();
        let found = vec![file("a.jpg", 100)];

        pending.ready(&found, at(0), settle());
        assert_eq!(pending.ready(&found, at(11), settle()).len(), 1);
        assert!(pending.ready(&found, at(30), settle()).is_empty());
        assert!(pending.ready(&found, at(90), settle()).is_empty());
    }

    #[test]
    fn a_file_that_changed_after_it_was_taken_comes_round_again() {
        let mut pending = Pending::new();

        pending.ready(&[file("a.jpg", 100)], at(0), settle());
        assert_eq!(
            pending.ready(&[file("a.jpg", 100)], at(11), settle()).len(),
            1
        );

        assert!(pending
            .ready(&[file("a.jpg", 250)], at(20), settle())
            .is_empty());
        assert_eq!(
            pending.ready(&[file("a.jpg", 250)], at(40), settle()).len(),
            1
        );
    }

    #[test]
    fn a_file_that_vanished_before_settling_is_forgotten() {
        let mut pending = Pending::new();

        pending.ready(&[file("a.jpg", 100)], at(0), settle());
        assert_eq!(pending.waiting(), 1);

        pending.ready(&[], at(5), settle());
        assert_eq!(pending.waiting(), 0);
    }

    #[test]
    fn several_files_come_back_in_a_settled_order() {
        let mut pending = Pending::new();
        let found = vec![file("c.jpg", 1), file("a.jpg", 1), file("b.jpg", 1)];

        pending.ready(&found, at(0), settle());
        let ready = pending.ready(&found, at(11), settle());

        assert_eq!(
            ready,
            vec![
                PathBuf::from("/watched/a.jpg"),
                PathBuf::from("/watched/b.jpg"),
                PathBuf::from("/watched/c.jpg"),
            ]
        );
    }

    #[test]
    fn forgetting_a_file_lets_it_be_taken_afresh() {
        let mut pending = Pending::new();
        let found = vec![file("a.jpg", 100)];

        pending.ready(&found, at(0), settle());
        pending.ready(&found, at(11), settle());
        assert!(pending.ready(&found, at(20), settle()).is_empty());

        pending.forget(Path::new("/watched/a.jpg"));
        pending.ready(&found, at(30), settle());
        assert_eq!(pending.ready(&found, at(45), settle()).len(), 1);
    }

    #[test]
    fn a_clock_that_went_backwards_does_not_release_a_file_early() {
        let mut pending = Pending::new();
        let found = vec![file("a.jpg", 100)];

        pending.ready(&found, at(100), settle());
        assert!(pending.ready(&found, at(50), settle()).is_empty());
    }

    #[test]
    fn a_sweep_finds_every_file_under_the_sources() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("2019").join("07");
        fs::create_dir_all(&nested).unwrap();
        fs::write(dir.path().join("a.jpg"), b"aa").unwrap();
        fs::write(nested.join("b.jpg"), b"bbbb").unwrap();

        let found = sweep(&[dir.path().to_path_buf()], false);

        assert_eq!(found.len(), 2);
        assert!(found
            .iter()
            .any(|(p, size)| p.ends_with("a.jpg") && *size == 2));
        assert!(found
            .iter()
            .any(|(p, size)| p.ends_with("b.jpg") && *size == 4));
    }

    #[test]
    fn a_sweep_of_nothing_finds_nothing() {
        let dir = tempdir().unwrap();
        assert!(sweep(&[dir.path().to_path_buf()], false).is_empty());
        assert!(sweep(&[dir.path().join("missing")], false).is_empty());
    }

    #[test]
    fn a_sweep_and_the_tracker_work_together_on_real_files() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.jpg");
        fs::write(&path, b"first").unwrap();

        let mut pending = Pending::new();
        let sources = vec![dir.path().to_path_buf()];

        assert!(pending
            .ready(&sweep(&sources, false), at(0), settle())
            .is_empty());

        fs::write(&path, b"grown a little").unwrap();
        assert!(pending
            .ready(&sweep(&sources, false), at(11), settle())
            .is_empty());

        let ready = pending.ready(&sweep(&sources, false), at(30), settle());
        assert_eq!(ready.len(), 1);
        assert!(ready[0].ends_with("a.jpg"));
    }

    #[test]
    fn the_defaults_are_a_sensible_place_to_start() {
        let options = WatchOptions::default();
        assert_eq!(options.interval.as_secs(), DEFAULT_INTERVAL_SECONDS);
        assert_eq!(options.settle.as_secs(), DEFAULT_SETTLE_SECONDS);
        assert!(options.settle < options.interval);
    }
}
