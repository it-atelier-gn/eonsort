use crate::dateparse::parse_date;
use crate::error::{Error, Result};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

pub const READ_IMAGE: &str = "\
Look at this image and answer only with JSON, no prose and no code fence.

{\"date\": \"<a date you can literally SEE printed, written, or displayed in the image, \
as YYYY-MM-DD or YYYY-MM-DD HH:MM:SS, or null>\",
 \"date_confidence\": \"<high|low>\",
 \"date_source\": \"<a few words naming where in the image you read it, or null>\",
 \"subject\": \"<one or two words for the main subject, lowercase>\",
 \"tags\": [\"<up to five short lowercase keywords>\"],
 \"caption\": \"<one plain sentence describing the image>\"}

Rules for the date: only report a date you can actually read in the picture — on a receipt, \
an invoice, a document, a newspaper, a screen, a clock, or a date stamp burnt into the photo. \
Never guess a date from the style, the clothing, the film grain, or the subject matter. \
If no date is legibly visible, the date must be null.";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reading {
    pub taken: Option<NaiveDateTime>,
    pub date_confident: bool,
    pub date_source: Option<String>,
    pub subject: Option<String>,
    pub tags: Vec<String>,
    pub caption: Option<String>,
}

#[derive(Deserialize)]
struct Raw {
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    date_confidence: Option<String>,
    #[serde(default)]
    date_source: Option<String>,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    caption: Option<String>,
}

pub fn parse_reading(answer: &str) -> Result<Reading> {
    let body = extract_object(answer)
        .ok_or_else(|| Error::Ai(format!("the model did not answer with JSON: {answer}")))?;
    let raw: Raw = serde_json::from_str(body)
        .map_err(|e| Error::Ai(format!("could not read the model's answer: {e}")))?;

    let confident = raw
        .date_confidence
        .as_deref()
        .map(|c| c.trim().eq_ignore_ascii_case("high"))
        .unwrap_or(false);

    let taken = raw
        .date
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty() && !d.eq_ignore_ascii_case("null"))
        .and_then(parse_date);

    Ok(Reading {
        taken,
        date_confident: confident,
        date_source: clean(raw.date_source),
        subject: clean(raw.subject).map(|s| s.to_lowercase()),
        tags: raw
            .tags
            .into_iter()
            .filter_map(|t| clean(Some(t)))
            .map(|t| t.to_lowercase())
            .take(5)
            .collect(),
        caption: clean(raw.caption),
    })
}

fn clean(value: Option<String>) -> Option<String> {
    let text = value?;
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("null") {
        return None;
    }
    Some(trimmed.to_string())
}

fn extract_object(answer: &str) -> Option<&str> {
    let start = answer.find('{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (offset, character) in answer[start..].char_indices() {
        if in_string {
            match character {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match character {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&answer[start..start + offset + character.len_utf8()]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn reads_a_clean_answer() {
        let reading = parse_reading(
            r#"{"date":"2019-07-04","date_confidence":"high","date_source":"receipt header",
                "subject":"receipt","tags":["paper","invoice"],"caption":"A printed receipt."}"#,
        )
        .unwrap();

        assert_eq!(
            reading.taken,
            Some(
                NaiveDate::from_ymd_opt(2019, 7, 4)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
            )
        );
        assert!(reading.date_confident);
        assert_eq!(reading.subject.as_deref(), Some("receipt"));
        assert_eq!(reading.tags, vec!["paper", "invoice"]);
    }

    #[test]
    fn digs_the_json_out_of_a_chatty_answer() {
        let reading = parse_reading(
            "Sure! Here is the JSON:\n```json\n{\"date\": null, \"subject\": \"Beach\"}\n```\nHope that helps.",
        )
        .unwrap();
        assert!(reading.taken.is_none());
        assert_eq!(reading.subject.as_deref(), Some("beach"));
    }

    #[test]
    fn handles_braces_inside_strings() {
        let reading =
            parse_reading(r#"{"caption": "a sign reading {open}", "subject": "sign"}"#).unwrap();
        assert_eq!(reading.caption.as_deref(), Some("a sign reading {open}"));
    }

    #[test]
    fn treats_a_null_string_as_no_date() {
        for body in [
            r#"{"date": null}"#,
            r#"{"date": "null"}"#,
            r#"{"date": "  "}"#,
            r#"{}"#,
        ] {
            assert!(parse_reading(body).unwrap().taken.is_none(), "{body}");
        }
    }

    #[test]
    fn rejects_a_date_the_shared_parser_will_not_accept() {
        let reading = parse_reading(r#"{"date": "sometime in the nineties"}"#).unwrap();
        assert!(reading.taken.is_none());
    }

    #[test]
    fn low_confidence_is_reported_rather_than_dropped() {
        let reading = parse_reading(r#"{"date":"2019-07-04","date_confidence":"low"}"#).unwrap();
        assert!(reading.taken.is_some());
        assert!(!reading.date_confident);
    }

    #[test]
    fn caps_the_tag_list() {
        let reading = parse_reading(r#"{"tags":["a","b","c","d","e","f","g"]}"#).unwrap();
        assert_eq!(reading.tags.len(), 5);
    }

    #[test]
    fn an_answer_with_no_json_is_an_error() {
        assert!(parse_reading("I cannot see any date in this image.").is_err());
        assert!(parse_reading("{\"date\": \"2019-07-04\"").is_err());
    }
}
