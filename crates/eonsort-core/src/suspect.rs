use crate::providers::{Detection, Provider};
use chrono::{Datelike, Duration, NaiveDateTime, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

pub const EPOCH_WINDOW_SECONDS: i64 = 60;
pub const RESET_WINDOW_SECONDS: i64 = 86_399;
pub const RUN_GAP_DAYS: i64 = 30;
pub const SPREAD_DAYS: i64 = 180;
pub const WRITE_TOLERANCE_HOURS: i64 = 48;
pub const CONSENSUS_HOURS: i64 = 24;
pub const NEIGHBOUR_DRIFT_DAYS: i64 = 730;
pub const IDENTICAL_CLUSTER_MIN: usize = 5;
pub const SEQUENCE_MIN: usize = 5;
pub const NEIGHBOUR_MIN: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "flag", rename_all = "snake_case")]
pub enum Flag {
    CameraEpoch,
    FutureDate,
    TakenAfterFileWrite,
    ProviderSpread { days: i64 },
    ClockResetRun { anchor: NaiveDateTime, files: usize },
    IdenticalTimestampCluster { files: usize },
    SequenceOutlier,
    FarFromNeighbours { years: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Hard,
    Soft,
}

impl Flag {
    pub fn severity(&self) -> Severity {
        match self {
            Flag::ProviderSpread { .. } => Severity::Soft,
            _ => Severity::Hard,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Flag::CameraEpoch => "sits on a camera factory-reset date".to_string(),
            Flag::FutureDate => "lies in the future".to_string(),
            Flag::TakenAfterFileWrite => "is later than the file was written".to_string(),
            Flag::ProviderSpread { days } => format!("sources disagree by {days} days"),
            Flag::ClockResetRun { anchor, files } => format!(
                "part of a {files}-file run counting up from {}",
                anchor.format("%Y-%m-%d %H:%M")
            ),
            Flag::IdenticalTimestampCluster { files } => {
                format!("shared to the second with {files} files")
            }
            Flag::SequenceOutlier => "breaks the camera counter order of its folder".to_string(),
            Flag::FarFromNeighbours { years } => {
                format!("{years} years away from the rest of its folder")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

impl Confidence {
    pub fn label(self) -> &'static str {
        match self {
            Confidence::Low => "low",
            Confidence::Medium => "medium",
            Confidence::High => "high",
        }
    }
}

pub struct EntryFacts<'a> {
    pub source: &'a Path,
    pub taken: NaiveDateTime,
    pub provider: Provider,
    pub filesystem: Option<NaiveDateTime>,
}

pub fn is_camera_epoch(taken: NaiveDateTime) -> bool {
    at_year_start(taken, EPOCH_WINDOW_SECONDS)
}

fn is_reset_anchor(taken: NaiveDateTime) -> bool {
    at_year_start(taken, RESET_WINDOW_SECONDS)
}

fn at_year_start(taken: NaiveDateTime, window: i64) -> bool {
    taken.month() == 1 && taken.day() == 1 && i64::from(taken.num_seconds_from_midnight()) <= window
}

pub fn date_flags(
    taken: NaiveDateTime,
    filesystem_latest: Option<NaiveDateTime>,
    now: NaiveDateTime,
) -> Vec<Flag> {
    let mut flags = Vec::new();
    if is_camera_epoch(taken) {
        flags.push(Flag::CameraEpoch);
    }
    if taken > now {
        flags.push(Flag::FutureDate);
    }
    if let Some(latest) = filesystem_latest {
        if taken - latest > Duration::hours(WRITE_TOLERANCE_HOURS) {
            flags.push(Flag::TakenAfterFileWrite);
        }
    }
    flags
}

pub fn spread(candidates: &[Detection]) -> Option<Flag> {
    let min = candidates.iter().map(|c| c.taken).min()?;
    let max = candidates.iter().map(|c| c.taken).max()?;
    let days = (max - min).num_days();
    (days > SPREAD_DAYS).then_some(Flag::ProviderSpread { days })
}

pub fn entry_flags(
    chosen: NaiveDateTime,
    candidates: &[Detection],
    filesystem_latest: Option<NaiveDateTime>,
    now: NaiveDateTime,
) -> Vec<Flag> {
    let mut flags = date_flags(chosen, filesystem_latest, now);
    flags.extend(spread(candidates));
    flags
}

pub fn confidence(candidates: &[Detection], flags: &[Flag]) -> Confidence {
    if flags.iter().any(|f| f.severity() == Severity::Hard) {
        return Confidence::Low;
    }
    if flags.is_empty() && corroborated(candidates) {
        return Confidence::High;
    }
    Confidence::Medium
}

fn corroborated(candidates: &[Detection]) -> bool {
    candidates.iter().enumerate().any(|(i, a)| {
        candidates
            .iter()
            .skip(i + 1)
            .any(|b| (a.taken - b.taken).num_hours().abs() < CONSENSUS_HOURS)
    })
}

pub fn cross_file_flags(entries: &[EntryFacts<'_>]) -> Vec<Vec<Flag>> {
    let mut out = vec![Vec::new(); entries.len()];
    let mut groups: HashMap<&Path, Vec<usize>> = HashMap::new();
    for (index, entry) in entries.iter().enumerate() {
        let parent = entry.source.parent().unwrap_or_else(|| Path::new(""));
        groups.entry(parent).or_default().push(index);
    }

    for indices in groups.values() {
        identical_clusters(entries, indices, &mut out);
        clock_reset_run(entries, indices, &mut out);
        far_from_neighbours(entries, indices, &mut out);
        sequence_outliers(entries, indices, &mut out);
    }
    out
}

fn identical_clusters(entries: &[EntryFacts<'_>], indices: &[usize], out: &mut [Vec<Flag>]) {
    let mut buckets: HashMap<NaiveDateTime, Vec<usize>> = HashMap::new();
    for &index in indices {
        buckets.entry(entries[index].taken).or_default().push(index);
    }
    for members in buckets.values() {
        if members.len() < IDENTICAL_CLUSTER_MIN {
            continue;
        }
        for &index in members {
            out[index].push(Flag::IdenticalTimestampCluster {
                files: members.len(),
            });
        }
    }
}

fn clock_reset_run(entries: &[EntryFacts<'_>], indices: &[usize], out: &mut [Vec<Flag>]) {
    let mut device: Vec<usize> = indices
        .iter()
        .copied()
        .filter(|&i| entries[i].provider != Provider::Filesystem)
        .collect();
    if device.len() < 2 {
        return;
    }
    device.sort_by_key(|&i| entries[i].taken);

    let anchor = entries[device[0]].taken;
    if !is_reset_anchor(anchor) {
        return;
    }

    let mut run = vec![device[0]];
    for &index in &device[1..] {
        let previous = entries[*run.last().unwrap()].taken;
        if (entries[index].taken - previous).num_days() > RUN_GAP_DAYS {
            break;
        }
        run.push(index);
    }
    if run.len() < 2 {
        return;
    }

    let latest = entries[*run.last().unwrap()].taken;
    let mut written: Vec<NaiveDateTime> =
        run.iter().filter_map(|&i| entries[i].filesystem).collect();
    if written.is_empty() {
        return;
    }
    written.sort();
    let reference = written[written.len() / 2];
    if (reference - latest).num_days() <= SPREAD_DAYS {
        return;
    }

    for &index in &run {
        out[index].push(Flag::ClockResetRun {
            anchor,
            files: run.len(),
        });
    }
}

fn far_from_neighbours(entries: &[EntryFacts<'_>], indices: &[usize], out: &mut [Vec<Flag>]) {
    if indices.len() < NEIGHBOUR_MIN {
        return;
    }
    let mut taken: Vec<NaiveDateTime> = indices.iter().map(|&i| entries[i].taken).collect();
    taken.sort();
    let median = taken[taken.len() / 2];

    for &index in indices {
        let drift = (entries[index].taken - median).num_days().abs();
        if drift < NEIGHBOUR_DRIFT_DAYS {
            continue;
        }
        let Some(written) = entries[index].filesystem else {
            continue;
        };
        if (written - median).num_days().abs() < NEIGHBOUR_DRIFT_DAYS {
            out[index].push(Flag::FarFromNeighbours { years: drift / 365 });
        }
    }
}

fn sequence_outliers(entries: &[EntryFacts<'_>], indices: &[usize], out: &mut [Vec<Flag>]) {
    let mut numbered: Vec<(u64, usize)> = indices
        .iter()
        .filter(|&&i| entries[i].provider != Provider::Filename)
        .filter_map(|&i| counter(entries[i].source).map(|n| (n, i)))
        .collect();
    if numbered.len() < SEQUENCE_MIN {
        return;
    }
    numbered.sort();

    let by_counter: Vec<usize> = numbered.iter().map(|&(_, i)| i).collect();
    let mut by_date = by_counter.clone();
    by_date.sort_by_key(|&i| entries[i].taken);

    let date_rank: HashMap<usize, usize> = by_date
        .iter()
        .enumerate()
        .map(|(rank, &index)| (index, rank))
        .collect();

    let tolerance = by_counter.len() / 2;
    for (rank, &index) in by_counter.iter().enumerate() {
        if date_rank[&index].abs_diff(rank) > tolerance {
            out[index].push(Flag::SequenceOutlier);
        }
    }
}

fn counter(path: &Path) -> Option<u64> {
    let stem = path.file_stem()?.to_str()?;
    let tail: Vec<char> = stem
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if tail.len() < 3 {
        return None;
    }
    tail.iter().rev().collect::<String>().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::path::PathBuf;

    fn at(y: i32, m: u32, d: u32, hh: u32, mm: u32, ss: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(hh, mm, ss)
            .unwrap()
    }

    fn detection(provider: Provider, taken: NaiveDateTime) -> Detection {
        Detection {
            provider,
            info: None,
            taken,
        }
    }

    #[test]
    fn recognises_the_common_camera_reset_dates() {
        for year in [1970, 1980, 2000, 2002, 2003, 2004, 2007, 2010, 2015, 2016] {
            assert!(
                is_camera_epoch(at(year, 1, 1, 0, 0, 0)),
                "{year}-01-01 00:00:00 should look like a reset"
            );
        }
        assert!(is_camera_epoch(at(2003, 1, 1, 0, 0, 42)));
    }

    #[test]
    fn a_real_new_year_photo_is_not_a_reset_date() {
        assert!(!is_camera_epoch(at(2019, 1, 1, 14, 32, 10)));
        assert!(!is_camera_epoch(at(2019, 1, 1, 0, 2, 0)));
        assert!(!is_camera_epoch(at(2019, 7, 4, 0, 0, 0)));
    }

    #[test]
    fn flags_a_date_in_the_future() {
        let now = at(2026, 8, 6, 12, 0, 0);
        assert!(date_flags(at(2030, 1, 5, 9, 0, 0), None, now).contains(&Flag::FutureDate));
        assert!(!date_flags(at(2020, 1, 5, 9, 0, 0), None, now).contains(&Flag::FutureDate));
    }

    #[test]
    fn flags_a_date_later_than_the_file_was_written() {
        let now = at(2026, 8, 6, 12, 0, 0);
        let written = at(2019, 7, 4, 10, 0, 0);
        let flags = date_flags(at(2019, 8, 1, 10, 0, 0), Some(written), now);
        assert!(flags.contains(&Flag::TakenAfterFileWrite));
    }

    #[test]
    fn tolerates_timezone_sized_differences_against_the_write_time() {
        let now = at(2026, 8, 6, 12, 0, 0);
        let written = at(2019, 7, 4, 10, 0, 0);
        let flags = date_flags(at(2019, 7, 5, 20, 0, 0), Some(written), now);
        assert!(!flags.contains(&Flag::TakenAfterFileWrite));
    }

    #[test]
    fn reports_the_spread_between_disagreeing_providers() {
        let candidates = vec![
            detection(Provider::Exif, at(2003, 1, 1, 0, 0, 0)),
            detection(Provider::Filesystem, at(2019, 7, 4, 10, 0, 0)),
        ];
        assert!(matches!(
            spread(&candidates),
            Some(Flag::ProviderSpread { days }) if days > 5000
        ));

        let close = vec![
            detection(Provider::Exif, at(2019, 7, 4, 10, 0, 0)),
            detection(Provider::Filesystem, at(2019, 7, 4, 11, 0, 0)),
        ];
        assert!(spread(&close).is_none());
    }

    #[test]
    fn confidence_is_low_for_any_hard_flag() {
        let candidates = vec![detection(Provider::Exif, at(2003, 1, 1, 0, 0, 0))];
        assert_eq!(
            confidence(&candidates, &[Flag::CameraEpoch]),
            Confidence::Low
        );
    }

    #[test]
    fn confidence_is_high_when_two_providers_corroborate() {
        let candidates = vec![
            detection(Provider::Exif, at(2019, 7, 4, 10, 0, 0)),
            detection(Provider::Filename, at(2019, 7, 4, 10, 0, 0)),
        ];
        assert_eq!(confidence(&candidates, &[]), Confidence::High);
    }

    #[test]
    fn confidence_is_medium_for_a_lone_provider_or_a_wide_spread() {
        let lone = vec![detection(Provider::Filesystem, at(2019, 7, 4, 10, 0, 0))];
        assert_eq!(confidence(&lone, &[]), Confidence::Medium);

        let wide = vec![
            detection(Provider::Exif, at(2019, 7, 4, 10, 0, 0)),
            detection(Provider::Filesystem, at(2023, 1, 1, 0, 0, 0)),
        ];
        let flags = vec![Flag::ProviderSpread { days: 1277 }];
        assert_eq!(confidence(&wide, &flags), Confidence::Medium);
    }

    fn facts<'a>(
        paths: &'a [PathBuf],
        takens: &[NaiveDateTime],
        provider: Provider,
        written: Option<NaiveDateTime>,
    ) -> Vec<EntryFacts<'a>> {
        paths
            .iter()
            .zip(takens)
            .map(|(source, &taken)| EntryFacts {
                source,
                taken,
                provider,
                filesystem: written,
            })
            .collect()
    }

    #[test]
    fn flags_a_whole_run_counting_up_from_a_reset_date() {
        let paths: Vec<PathBuf> = (0..6)
            .map(|i| PathBuf::from(format!("/trip/IMG_{:04}.jpg", 100 + i)))
            .collect();
        let takens: Vec<NaiveDateTime> = (0..6).map(|i| at(2003, 1, 1, 9, i * 3, 0)).collect();
        let entries = facts(
            &paths,
            &takens,
            Provider::Exif,
            Some(at(2019, 7, 4, 10, 0, 0)),
        );

        let flags = cross_file_flags(&entries);
        for per_entry in &flags {
            assert!(per_entry
                .iter()
                .any(|f| matches!(f, Flag::ClockResetRun { files: 6, .. })));
        }
    }

    #[test]
    fn a_new_year_folder_whose_files_were_written_then_is_left_alone() {
        let paths: Vec<PathBuf> = (0..6)
            .map(|i| PathBuf::from(format!("/party/IMG_{:04}.jpg", 100 + i)))
            .collect();
        let takens: Vec<NaiveDateTime> = (0..6).map(|i| at(2019, 1, 1, 0, i * 3, 0)).collect();
        let entries = facts(
            &paths,
            &takens,
            Provider::Exif,
            Some(at(2019, 1, 1, 2, 0, 0)),
        );

        let flags = cross_file_flags(&entries);
        assert!(flags
            .iter()
            .all(|f| !f.iter().any(|x| matches!(x, Flag::ClockResetRun { .. }))));
    }

    #[test]
    fn a_correctly_dated_file_in_the_folder_does_not_hide_the_run() {
        let mut paths: Vec<PathBuf> = (0..6)
            .map(|i| PathBuf::from(format!("/trip/IMG_{:04}.jpg", 100 + i)))
            .collect();
        let mut takens: Vec<NaiveDateTime> = (0..6).map(|i| at(2003, 1, 1, 9, i * 7, 0)).collect();
        paths.push(PathBuf::from("/trip/IMG_0200.jpg"));
        takens.push(at(2019, 7, 4, 10, 11, 12));

        let entries = facts(
            &paths,
            &takens,
            Provider::Filename,
            Some(at(2019, 7, 4, 18, 30, 0)),
        );

        let flags = cross_file_flags(&entries);
        for stuck in &flags[..6] {
            assert!(stuck
                .iter()
                .any(|f| matches!(f, Flag::ClockResetRun { files: 6, .. })));
        }
        assert!(!flags[6]
            .iter()
            .any(|f| matches!(f, Flag::ClockResetRun { .. })));
    }

    #[test]
    fn a_correctly_dated_folder_is_left_alone() {
        let paths: Vec<PathBuf> = (0..6)
            .map(|i| PathBuf::from(format!("/trip/IMG_{:04}.jpg", 100 + i)))
            .collect();
        let takens: Vec<NaiveDateTime> = (0..6).map(|i| at(2019, 7, 4, 10, i * 3, 0)).collect();
        let entries = facts(
            &paths,
            &takens,
            Provider::Exif,
            Some(at(2019, 7, 4, 10, 0, 0)),
        );

        assert!(cross_file_flags(&entries).iter().all(|f| f.is_empty()));
    }

    #[test]
    fn flags_files_frozen_on_one_identical_timestamp() {
        let paths: Vec<PathBuf> = (0..5)
            .map(|i| PathBuf::from(format!("/dump/a{i}.jpg")))
            .collect();
        let takens = vec![at(2015, 1, 1, 0, 0, 0); 5];
        let entries = facts(&paths, &takens, Provider::Media, None);

        let flags = cross_file_flags(&entries);
        assert!(flags.iter().all(|f| f
            .iter()
            .any(|x| matches!(x, Flag::IdenticalTimestampCluster { files: 5 }))));
    }

    #[test]
    fn flags_a_single_file_stranded_far_from_its_folder() {
        let paths: Vec<PathBuf> = (0..6)
            .map(|i| PathBuf::from(format!("/trip/scan{i}.tif")))
            .collect();
        let mut takens: Vec<NaiveDateTime> = (0..6).map(|i| at(2019, 7, 4, 10, i * 3, 0)).collect();
        takens[3] = at(2004, 6, 1, 8, 0, 0);

        let mut entries = facts(&paths, &takens, Provider::Exif, None);
        for entry in entries.iter_mut() {
            entry.filesystem = Some(at(2019, 7, 4, 10, 0, 0));
        }

        let flags = cross_file_flags(&entries);
        assert!(flags[3]
            .iter()
            .any(|f| matches!(f, Flag::FarFromNeighbours { .. })));
        assert!(flags[0]
            .iter()
            .all(|f| !matches!(f, Flag::FarFromNeighbours { .. })));
    }

    #[test]
    fn flags_a_file_that_breaks_the_camera_counter_order() {
        let paths: Vec<PathBuf> = (0..7)
            .map(|i| PathBuf::from(format!("/cam/DSC_{:04}.jpg", 200 + i)))
            .collect();
        let mut takens: Vec<NaiveDateTime> = (0..7).map(|i| at(2019, 7, 4, 10, i * 5, 0)).collect();
        takens[0] = at(2019, 7, 4, 23, 0, 0);

        let entries = facts(&paths, &takens, Provider::Exif, None);
        let flags = cross_file_flags(&entries);
        assert!(flags[0].contains(&Flag::SequenceOutlier));
        assert!(!flags[3].contains(&Flag::SequenceOutlier));
    }

    #[test]
    fn does_not_read_a_timestamp_in_a_file_name_as_a_camera_counter() {
        let paths: Vec<PathBuf> = [
            "/cam/IMG_20190704_101112.jpg",
            "/cam/IMG_20190704_183000.jpg",
            "/cam/IMG_20190704_183700.jpg",
            "/cam/IMG_20190704_184400.jpg",
            "/cam/IMG_20190704_185100.jpg",
            "/cam/IMG_20190704_185800.jpg",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();
        let takens = vec![
            at(2019, 7, 4, 10, 11, 12),
            at(2019, 7, 4, 18, 30, 0),
            at(2019, 7, 4, 18, 37, 0),
            at(2019, 7, 4, 18, 44, 0),
            at(2019, 7, 4, 18, 51, 0),
            at(2019, 7, 4, 18, 58, 0),
        ];

        let entries = facts(&paths, &takens, Provider::Filename, None);
        let flags = cross_file_flags(&entries);
        assert!(flags.iter().all(|f| !f.contains(&Flag::SequenceOutlier)));
    }

    #[test]
    fn reads_a_trailing_camera_counter() {
        assert_eq!(counter(Path::new("/cam/DSC_0123.jpg")), Some(123));
        assert_eq!(
            counter(Path::new("/cam/IMG_20230506_101112.jpg")),
            Some(101112)
        );
        assert_eq!(counter(Path::new("/cam/holiday.jpg")), None);
        assert_eq!(counter(Path::new("/cam/a12.jpg")), None);
    }
}
