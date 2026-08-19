use super::{Detection, Provider};
use chrono::{DateTime, Local};
use std::path::{Path, PathBuf};

const SUPPLEMENT: &str = "supplemental-metadata";
const NAME_LIMIT: usize = 51;
const EDITED: [&str; 6] = [
    "-edited",
    "-bearbeitet",
    "-modifié",
    "-editado",
    "-bewerkt",
    "-redigerad",
];

pub fn detect(path: &Path) -> Option<Detection> {
    let sidecar = sidecar(path)?;
    let raw = std::fs::read_to_string(&sidecar).ok()?;
    let taken = taken_from(&raw)?;

    Some(Detection {
        provider: Provider::Takeout,
        info: sidecar
            .file_name()
            .map(|name| name.to_string_lossy().into_owned()),
        taken,
    })
}

pub fn sidecar(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    let name = path.file_name()?.to_str()?;

    candidates(name)
        .into_iter()
        .map(|candidate| parent.join(candidate))
        .find(|candidate| candidate.is_file())
}

fn candidates(name: &str) -> Vec<String> {
    let mut names = vec![name.to_string()];

    if let Some(plain) = without_edited(name) {
        names.push(plain);
    }

    let mut out = Vec::new();
    for name in names {
        if let Some((stem, copy)) = numbered(&name) {
            out.push(format!("{stem}({copy}).json"));
        }
        out.push(format!("{name}.json"));
        out.push(trimmed(&format!("{name}.{SUPPLEMENT}.json")));
    }
    out
}

fn without_edited(name: &str) -> Option<String> {
    let (stem, extension) = name.rsplit_once('.')?;
    let plain = EDITED.iter().find_map(|mark| stem.strip_suffix(mark))?;
    Some(format!("{plain}.{extension}"))
}

fn numbered(name: &str) -> Option<(String, String)> {
    let (head, rest) = name.split_once('(')?;
    let (copy, extension) = rest.split_once(')')?;
    if copy.is_empty() || !copy.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let extension = extension.strip_prefix('.')?;
    Some((format!("{head}.{extension}"), copy.to_string()))
}

fn trimmed(name: &str) -> String {
    if name.len() <= NAME_LIMIT {
        return name.to_string();
    }

    let mut head = name[..name.len() - ".json".len()].to_string();
    while head.len() > NAME_LIMIT - ".json".len() || !head.is_char_boundary(head.len()) {
        head.pop();
    }
    format!("{head}.json")
}

fn taken_from(raw: &str) -> Option<chrono::NaiveDateTime> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let seconds = epoch(&value, "photoTakenTime").or_else(|| epoch(&value, "creationTime"))?;
    DateTime::from_timestamp(seconds, 0).map(|utc| utc.with_timezone(&Local).naive_local())
}

fn epoch(value: &serde_json::Value, field: &str) -> Option<i64> {
    let stamp = value.get(field)?.get("timestamp")?;
    stamp
        .as_str()
        .and_then(|text| text.parse().ok())
        .or_else(|| stamp.as_i64())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::fs;
    use tempfile::tempdir;

    const TAKEN: i64 = 1_557_818_523;

    fn body(seconds: i64) -> String {
        format!(
            r#"{{"title":"IMG_1234.JPG","photoTakenTime":{{"timestamp":"{seconds}","formatted":"whatever"}}}}"#
        )
    }

    fn expected(seconds: i64) -> chrono::NaiveDateTime {
        Local.timestamp_opt(seconds, 0).unwrap().naive_local()
    }

    #[test]
    fn reads_the_plain_sidecar_beside_the_picture() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("IMG_1234.JPG"), b"picture").unwrap();
        fs::write(dir.path().join("IMG_1234.JPG.json"), body(TAKEN)).unwrap();

        let found = detect(&dir.path().join("IMG_1234.JPG")).unwrap();
        assert_eq!(found.provider, Provider::Takeout);
        assert_eq!(found.taken, expected(TAKEN));
    }

    #[test]
    fn reads_the_supplemental_metadata_sidecar() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("IMG_1234.JPG"), b"picture").unwrap();
        fs::write(
            dir.path().join("IMG_1234.JPG.supplemental-metadata.json"),
            body(TAKEN),
        )
        .unwrap();

        assert_eq!(
            detect(&dir.path().join("IMG_1234.JPG")).unwrap().taken,
            expected(TAKEN)
        );
    }

    #[test]
    fn reads_a_sidecar_whose_name_google_cut_short() {
        let dir = tempdir().unwrap();
        let picture = "PXL_20230506_101112345.PORTRAIT.jpg";
        fs::write(dir.path().join(picture), b"picture").unwrap();

        let cut = trimmed(&format!("{picture}.{SUPPLEMENT}.json"));
        assert_eq!(cut.len(), NAME_LIMIT);
        fs::write(dir.path().join(&cut), body(TAKEN)).unwrap();

        assert_eq!(
            detect(&dir.path().join(picture)).unwrap().taken,
            expected(TAKEN)
        );
    }

    #[test]
    fn follows_the_sidecar_of_a_numbered_copy() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("IMG_1234(1).JPG"), b"picture").unwrap();
        fs::write(dir.path().join("IMG_1234.JPG(1).json"), body(TAKEN)).unwrap();

        assert_eq!(
            detect(&dir.path().join("IMG_1234(1).JPG")).unwrap().taken,
            expected(TAKEN)
        );
    }

    #[test]
    fn an_edited_copy_falls_back_to_the_original_sidecar() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("IMG_1234-edited.JPG"), b"picture").unwrap();
        fs::write(dir.path().join("IMG_1234.JPG.json"), body(TAKEN)).unwrap();

        assert_eq!(
            detect(&dir.path().join("IMG_1234-edited.JPG")).unwrap().taken,
            expected(TAKEN)
        );
    }

    #[test]
    fn falls_back_to_the_creation_time_when_no_photo_time_is_recorded() {
        let raw = format!(r#"{{"creationTime":{{"timestamp":"{TAKEN}"}}}}"#);
        assert_eq!(taken_from(&raw).unwrap(), expected(TAKEN));
    }

    #[test]
    fn takes_a_timestamp_written_as_a_number() {
        let raw = format!(r#"{{"photoTakenTime":{{"timestamp":{TAKEN}}}}}"#);
        assert_eq!(taken_from(&raw).unwrap(), expected(TAKEN));
    }

    #[test]
    fn says_nothing_without_a_sidecar() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("IMG_1234.JPG"), b"picture").unwrap();

        assert!(detect(&dir.path().join("IMG_1234.JPG")).is_none());
    }

    #[test]
    fn says_nothing_for_a_sidecar_that_carries_no_time() {
        assert!(taken_from(r#"{"title":"IMG_1234.JPG"}"#).is_none());
        assert!(taken_from("not json at all").is_none());
    }
}
