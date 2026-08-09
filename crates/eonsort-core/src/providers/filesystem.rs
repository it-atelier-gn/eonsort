use super::{Detection, Provider};
use chrono::{DateTime, Local, NaiveDateTime};
use std::fs::Metadata;
use std::time::SystemTime;

pub fn detect(meta: &Metadata) -> Option<Detection> {
    let created = meta.created().ok().and_then(to_local);
    let modified = meta.modified().ok().and_then(to_local);
    resolve(created, modified)
}

pub fn latest(meta: &Metadata) -> Option<NaiveDateTime> {
    let created = meta.created().ok().and_then(to_local);
    let modified = meta.modified().ok().and_then(to_local);
    match (created, modified) {
        (Some(c), Some(m)) => Some(c.max(m)),
        (Some(t), None) | (None, Some(t)) => Some(t),
        (None, None) => None,
    }
}

fn resolve(created: Option<NaiveDateTime>, modified: Option<NaiveDateTime>) -> Option<Detection> {
    let (taken, info) = match (created, modified) {
        (Some(c), Some(m)) if m < c => (m, "modified"),
        (Some(c), _) => (c, "created"),
        (None, Some(m)) => (m, "modified"),
        (None, None) => return None,
    };

    Some(Detection {
        provider: Provider::Filesystem,
        info: Some(info.to_string()),
        taken,
    })
}

fn to_local(time: SystemTime) -> Option<NaiveDateTime> {
    Some(DateTime::<Local>::from(time).naive_local())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn reports_a_time_for_a_regular_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.bin");
        fs::write(&path, b"x").unwrap();

        let found = detect(&fs::metadata(&path).unwrap()).unwrap();
        assert_eq!(found.provider, Provider::Filesystem);
        assert!(matches!(
            found.info.as_deref(),
            Some("created") | Some("modified")
        ));
    }

    #[test]
    fn prefers_the_earlier_of_created_and_modified() {
        let created =
            NaiveDateTime::parse_from_str("2020-06-15 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let modified =
            NaiveDateTime::parse_from_str("2000-01-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();

        let found = resolve(Some(created), Some(modified)).unwrap();
        assert_eq!(found.taken, modified);
        assert_eq!(found.info.as_deref(), Some("modified"));
    }

    #[test]
    fn falls_back_to_created_when_not_earlier_than_modified() {
        let created =
            NaiveDateTime::parse_from_str("2020-06-15 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let modified = created;

        let found = resolve(Some(created), Some(modified)).unwrap();
        assert_eq!(found.taken, created);
        assert_eq!(found.info.as_deref(), Some("created"));
    }
}
