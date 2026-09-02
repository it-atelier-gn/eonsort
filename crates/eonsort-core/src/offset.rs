use crate::model::PlanEntry;
use crate::providers::Provider;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const MIN_FILES: usize = 5;
pub const MIN_SECONDS: i64 = 60;
pub const MERGE_TOLERANCE_SECONDS: i64 = 2;
pub const HOUR_SLACK_SECONDS: i64 = 90;
pub const MAX_ZONE_HOURS: i64 = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum Shape {
    Timezone { hours: i64 },
    Drift,
}

impl Shape {
    fn of(seconds: i64) -> Self {
        let hours = (seconds as f64 / 3_600.0).round() as i64;
        let remainder = (seconds - hours * 3_600).abs();
        if hours != 0 && hours.abs() <= MAX_ZONE_HOURS && remainder <= HOUR_SLACK_SECONDS {
            Shape::Timezone { hours }
        } else {
            Shape::Drift
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offset {
    pub reference: Provider,
    pub seconds: i64,
    pub shape: Shape,
    pub sources: Vec<PathBuf>,
    pub span: Option<(NaiveDateTime, NaiveDateTime)>,
}

impl Offset {
    pub fn files(&self) -> usize {
        self.sources.len()
    }

    pub fn describe(&self) -> String {
        let files = self.files();
        let plural = if files == 1 { "file" } else { "files" };
        let correction = humanise(-self.seconds);
        match self.shape {
            Shape::Timezone { hours } => format!(
                "{files} {plural} read exactly {} from {}, which is the shape of a time zone; correcting them means {correction}",
                whole_hours(hours),
                self.reference.label()
            ),
            Shape::Drift => format!(
                "{files} {plural} run {} ahead of {}; correcting them means {correction}",
                humanise(self.seconds),
                self.reference.label()
            ),
        }
    }
}

fn whole_hours(hours: i64) -> String {
    let magnitude = hours.abs();
    let plural = if magnitude == 1 { "hour" } else { "hours" };
    let side = if hours < 0 { "behind" } else { "ahead of" };
    format!("{magnitude} {plural} {side}")
}

pub fn humanise(seconds: i64) -> String {
    let sign = if seconds < 0 { "-" } else { "+" };
    let total = seconds.unsigned_abs();
    let days = total / 86_400;
    let hours = (total % 86_400) / 3_600;
    let minutes = (total % 3_600) / 60;
    let rest = total % 60;

    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}m"));
    }
    if rest > 0 || parts.is_empty() {
        parts.push(format!("{rest}s"));
    }
    format!("{sign}{}", parts.join(" "))
}

pub fn propose(entries: &[PlanEntry]) -> Vec<Offset> {
    let mut buckets: HashMap<(Provider, i64), Vec<&PlanEntry>> = HashMap::new();

    for entry in entries {
        for candidate in &entry.candidates {
            if candidate.provider == entry.provider {
                continue;
            }
            let seconds = (entry.taken - candidate.taken).num_seconds();
            if seconds.abs() < MIN_SECONDS {
                continue;
            }
            buckets
                .entry((candidate.provider, seconds))
                .or_default()
                .push(entry);
        }
    }

    let mut merged = merge_neighbours(buckets);
    merged.retain(|(_, members)| members.len() >= MIN_FILES);

    let mut offsets: Vec<Offset> = merged
        .into_iter()
        .map(|((reference, seconds), members)| build(reference, seconds, &members))
        .collect();

    offsets.sort_by(|left, right| {
        right
            .files()
            .cmp(&left.files())
            .then(right.seconds.abs().cmp(&left.seconds.abs()))
            .then(left.reference.cmp(&right.reference))
    });
    offsets
}

fn merge_neighbours(
    buckets: HashMap<(Provider, i64), Vec<&PlanEntry>>,
) -> Vec<((Provider, i64), Vec<&PlanEntry>)> {
    let mut by_provider: HashMap<Provider, Vec<(i64, Vec<&PlanEntry>)>> = HashMap::new();
    for ((provider, seconds), members) in buckets {
        by_provider
            .entry(provider)
            .or_default()
            .push((seconds, members));
    }

    let mut out = Vec::new();
    for (provider, mut group) in by_provider {
        group.sort_by_key(|(seconds, _)| *seconds);

        let mut current: Option<(i64, Vec<&PlanEntry>)> = None;
        for (seconds, members) in group {
            match current.as_mut() {
                Some((anchor, collected))
                    if (seconds - *anchor).abs() <= MERGE_TOLERANCE_SECONDS =>
                {
                    collected.extend(members);
                }
                _ => {
                    if let Some(done) = current.take() {
                        out.push(((provider, done.0), done.1));
                    }
                    current = Some((seconds, members));
                }
            }
        }
        if let Some(done) = current {
            out.push(((provider, done.0), done.1));
        }
    }
    out
}

fn build(reference: Provider, seconds: i64, members: &[&PlanEntry]) -> Offset {
    let mut sources: Vec<PathBuf> = members.iter().map(|e| e.source.clone()).collect();
    sources.sort();
    sources.dedup();

    let taken: Vec<NaiveDateTime> = members.iter().map(|e| e.taken).collect();
    let span = taken
        .iter()
        .min()
        .zip(taken.iter().max())
        .map(|(first, last)| (*first, *last));

    Offset {
        reference,
        seconds,
        shape: Shape::of(seconds),
        sources,
        span,
    }
}

pub fn covering<'a>(offsets: &'a [Offset], source: &Path) -> Option<&'a Offset> {
    offsets
        .iter()
        .find(|offset| offset.sources.iter().any(|s| s == source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::Detection;
    use chrono::{Duration, NaiveDate};

    fn at(y: i32, m: u32, d: u32, hh: u32, mm: u32, ss: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(hh, mm, ss)
            .unwrap()
    }

    fn entry(name: &str, chosen: NaiveDateTime, reference: (Provider, NaiveDateTime)) -> PlanEntry {
        PlanEntry {
            source: PathBuf::from(format!("/src/{name}")),
            taken: chosen,
            provider: Provider::Exif,
            candidates: vec![
                Detection {
                    provider: Provider::Exif,
                    info: None,
                    taken: chosen,
                },
                Detection {
                    provider: reference.0,
                    info: None,
                    taken: reference.1,
                },
            ],
            ..PlanEntry::default()
        }
    }

    fn run(count: usize, drift: Duration) -> Vec<PlanEntry> {
        (0..count)
            .map(|index| {
                let truth = at(2019, 7, 4, 10, 0, 0) + Duration::minutes(index as i64);
                entry(
                    &format!("IMG_{index:04}.jpg"),
                    truth + drift,
                    (Provider::Filename, truth),
                )
            })
            .collect()
    }

    #[test]
    fn finds_one_offset_shared_by_a_whole_run() {
        let entries = run(20, Duration::hours(5) + Duration::minutes(32));
        let offsets = propose(&entries);

        assert_eq!(offsets.len(), 1);
        assert_eq!(offsets[0].files(), 20);
        assert_eq!(offsets[0].seconds, 5 * 3_600 + 32 * 60);
        assert_eq!(offsets[0].reference, Provider::Filename);
    }

    #[test]
    fn a_whole_hour_offset_reads_as_a_time_zone() {
        let entries = run(10, Duration::hours(-2));
        let offsets = propose(&entries);

        assert_eq!(offsets[0].shape, Shape::Timezone { hours: -2 });
        assert!(offsets[0].describe().contains("time zone"));
        assert!(offsets[0].describe().contains("2 hours behind"));
    }

    #[test]
    fn an_arbitrary_offset_reads_as_drift() {
        let entries = run(10, Duration::hours(5) + Duration::minutes(32));
        assert_eq!(propose(&entries)[0].shape, Shape::Drift);
    }

    #[test]
    fn a_handful_of_files_is_not_enough_to_call_it_systematic() {
        let entries = run(MIN_FILES - 1, Duration::hours(3));
        assert!(propose(&entries).is_empty());
    }

    #[test]
    fn small_disagreements_are_left_alone() {
        let entries = run(30, Duration::seconds(MIN_SECONDS - 1));
        assert!(propose(&entries).is_empty());
    }

    #[test]
    fn clocks_a_second_or_two_apart_still_count_as_the_same_offset() {
        let base = at(2019, 7, 4, 10, 0, 0);
        let entries: Vec<PlanEntry> = (0..12)
            .map(|index| {
                let wobble = Duration::seconds(i64::from(index % 3));
                entry(
                    &format!("IMG_{index:04}.jpg"),
                    base + Duration::hours(3) + wobble,
                    (Provider::Filename, base),
                )
            })
            .collect();

        let offsets = propose(&entries);
        assert_eq!(offsets.len(), 1);
        assert_eq!(offsets[0].files(), 12);
    }

    #[test]
    fn two_cameras_with_two_different_offsets_stay_apart() {
        let mut entries = run(8, Duration::hours(3));
        entries.extend(
            (0..9)
                .map(|index| {
                    let truth = at(2020, 1, 1, 9, 0, 0) + Duration::minutes(index);
                    entry(
                        &format!("DSC_{index:04}.jpg"),
                        truth - Duration::hours(7),
                        (Provider::Filename, truth),
                    )
                })
                .collect::<Vec<_>>(),
        );

        let offsets = propose(&entries);
        assert_eq!(offsets.len(), 2);
        assert_eq!(offsets[0].files(), 9);
        assert_eq!(offsets[0].seconds, -7 * 3_600);
        assert_eq!(offsets[1].files(), 8);
        assert_eq!(offsets[1].seconds, 3 * 3_600);
    }

    #[test]
    fn the_offset_reports_the_stretch_of_time_it_covers() {
        let entries = run(10, Duration::hours(4));
        let span = propose(&entries)[0].span.unwrap();
        assert_eq!(span.0, at(2019, 7, 4, 14, 0, 0));
        assert_eq!(span.1, at(2019, 7, 4, 14, 9, 0));
    }

    #[test]
    fn the_correction_is_the_opposite_of_the_offset() {
        let entries = run(10, Duration::hours(5));
        let told = propose(&entries)[0].describe();
        assert!(told.contains("-5h"), "{told}");
    }

    #[test]
    fn a_file_can_be_looked_up_by_the_offset_that_covers_it() {
        let entries = run(10, Duration::hours(4));
        let offsets = propose(&entries);

        assert!(covering(&offsets, Path::new("/src/IMG_0003.jpg")).is_some());
        assert!(covering(&offsets, Path::new("/src/nowhere.jpg")).is_none());
    }

    #[test]
    fn the_chosen_provider_is_never_its_own_reference() {
        let entries = run(10, Duration::hours(4));
        assert!(propose(&entries)
            .iter()
            .all(|offset| offset.reference != Provider::Exif));
    }

    #[test]
    fn spells_out_a_shift_in_both_directions() {
        assert_eq!(humanise(90_061), "+1d 1h 1m 1s");
        assert_eq!(humanise(-3_600), "-1h");
        assert_eq!(humanise(0), "+0s");
    }

    #[test]
    fn a_shape_is_only_a_time_zone_when_it_lands_near_the_hour() {
        assert_eq!(Shape::of(3_600), Shape::Timezone { hours: 1 });
        assert_eq!(
            Shape::of(3_600 + HOUR_SLACK_SECONDS),
            Shape::Timezone { hours: 1 }
        );
        assert_eq!(Shape::of(3_600 + HOUR_SLACK_SECONDS + 1), Shape::Drift);
        assert_eq!(Shape::of((MAX_ZONE_HOURS + 1) * 3_600), Shape::Drift);
        assert_eq!(Shape::of(0), Shape::Drift);
    }
}
