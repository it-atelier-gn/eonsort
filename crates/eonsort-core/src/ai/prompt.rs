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

pub const READ_SCENE: &str = "\
Look at this photograph and answer only with JSON, no prose and no code fence.

{\"scene_type\": \"<indoor_room|corridor|street|landscape|portrait|closeup|other>\",
 \"has_perspective\": <true or false>,
 \"vanishing_point\": [<x>, <y>],
 \"back_wall\": [<x0>, <y0>, <x1>, <y1>],
 \"objects\": [{\"label\": \"<one or two lowercase words>\",
               \"box\": [<x0>, <y0>, <x1>, <y1>],
               \"on_the_ground\": <true or false>}]}

All coordinates are fractions of the image, 0 to 1, with 0,0 at the top left.

The vanishing point is where receding parallel lines — a corridor, a road, a row of windows — \
would meet if you extended them. It is usually on the horizon.
The back wall is the rectangle of the furthest flat surface facing you. It must contain the \
vanishing point.
If nothing recedes — a portrait, a close-up, a flat wall — set has_perspective to false and the \
back wall to [0, 0, 1, 1].
List at most six objects: things standing in front of the background that a person could walk \
around. Give each a tight bounding box.";

pub const SCENE_MARGIN: f32 = 0.06;
pub const SCENE_MIN_GAP: f32 = 0.03;
pub const SCENE_INSET: f32 = 0.42;
const MAX_SCENE_OBJECTS: usize = 6;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneObject {
    pub label: String,
    pub bounds: (f32, f32, f32, f32),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneReading {
    pub scene_type: Option<String>,
    pub flat: bool,
    pub vanishing_point: (f32, f32),
    pub back_wall: (f32, f32, f32, f32),
    pub objects: Vec<SceneObject>,
}

#[derive(Deserialize)]
struct RawScene {
    #[serde(default)]
    scene_type: Option<String>,
    #[serde(default)]
    has_perspective: Option<bool>,
    #[serde(default)]
    vanishing_point: Option<Vec<Option<f32>>>,
    #[serde(default)]
    back_wall: Option<Vec<Option<f32>>>,
    #[serde(default)]
    objects: Vec<RawObject>,
}

#[derive(Deserialize)]
struct RawObject {
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    r#box: Option<Vec<Option<f32>>>,
    #[serde(default)]
    on_the_ground: Option<bool>,
}

fn four(values: &[Option<f32>]) -> Option<[f32; 4]> {
    if values.len() < 4 {
        return None;
    }
    let mut out = [0.0f32; 4];
    for (index, slot) in out.iter_mut().enumerate() {
        let value = values[index]?;
        if !value.is_finite() {
            return None;
        }
        *slot = value;
    }
    Some(out)
}

fn clamp(value: f32, low: f32, high: f32) -> f32 {
    if value < low {
        low
    } else if value > high {
        high
    } else {
        value
    }
}

fn coordinate(value: Option<f32>, fallback: f32) -> f32 {
    match value {
        Some(v) if v.is_finite() => v,
        _ => fallback,
    }
}

fn inset_around(u: f32, v: f32) -> (f32, f32, f32, f32) {
    let s = SCENE_INSET;
    (
        u * (1.0 - s),
        v * (1.0 - s),
        u + s * (1.0 - u),
        v + s * (1.0 - v),
    )
}

pub fn parse_scene(answer: &str) -> Result<SceneReading> {
    let body = extract_object(answer)
        .ok_or_else(|| Error::Ai(format!("the model did not answer with JSON: {answer}")))?;
    let raw: RawScene = serde_json::from_str(body)
        .map_err(|e| Error::Ai(format!("could not read the model's answer: {e}")))?;

    let point = raw.vanishing_point.unwrap_or_default();
    let u = clamp(
        coordinate(point.first().copied().flatten(), 0.5),
        SCENE_MARGIN,
        1.0 - SCENE_MARGIN,
    );
    let v = clamp(
        coordinate(point.get(1).copied().flatten(), 0.5),
        SCENE_MARGIN,
        1.0 - SCENE_MARGIN,
    );

    let flat = raw.has_perspective == Some(false);
    let wall = raw.back_wall.as_deref().and_then(four);

    let back_wall = match (flat, wall) {
        (false, Some(values)) => {
            let mut x0 = clamp(values[0], 0.0, 1.0);
            let mut y0 = clamp(values[1], 0.0, 1.0);
            let mut x1 = clamp(values[2], 0.0, 1.0);
            let mut y1 = clamp(values[3], 0.0, 1.0);

            if x0 > x1 {
                std::mem::swap(&mut x0, &mut x1);
            }
            if y0 > y1 {
                std::mem::swap(&mut y0, &mut y1);
            }

            (
                x0.min(u - SCENE_MIN_GAP),
                y0.min(v - SCENE_MIN_GAP),
                x1.max(u + SCENE_MIN_GAP),
                y1.max(v + SCENE_MIN_GAP),
            )
        }
        _ => inset_around(u, v),
    };

    let mut objects = Vec::new();
    for object in raw.objects {
        if object.on_the_ground == Some(false) {
            continue;
        }
        let Some(values) = object.r#box.as_deref().and_then(four) else {
            continue;
        };
        let Some(label) = clean(object.label).map(|l| l.to_lowercase()) else {
            continue;
        };

        let mut x0 = clamp(values[0], 0.0, 1.0);
        let mut y0 = clamp(values[1], 0.0, 1.0);
        let mut x1 = clamp(values[2], 0.0, 1.0);
        let mut y1 = clamp(values[3], 0.0, 1.0);
        if x0 > x1 {
            std::mem::swap(&mut x0, &mut x1);
        }
        if y0 > y1 {
            std::mem::swap(&mut y0, &mut y1);
        }

        let area = (x1 - x0) * (y1 - y0);
        if area <= 0.0004 || area > 0.9 {
            continue;
        }

        objects.push(SceneObject {
            label,
            bounds: (x0, y0, x1, y1),
        });
        if objects.len() >= MAX_SCENE_OBJECTS {
            break;
        }
    }

    Ok(SceneReading {
        scene_type: clean(raw.scene_type).map(|t| t.to_lowercase()),
        flat,
        vanishing_point: (u, v),
        back_wall,
        objects,
    })
}

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

    fn near(left: (f32, f32, f32, f32), right: (f32, f32, f32, f32)) {
        for (a, b) in [
            (left.0, right.0),
            (left.1, right.1),
            (left.2, right.2),
            (left.3, right.3),
        ] {
            assert!((a - b).abs() < 1e-5, "{left:?} is not {right:?}");
        }
    }

    fn holds(reading: &SceneReading) -> bool {
        let (u, v) = reading.vanishing_point;
        let (x0, y0, x1, y1) = reading.back_wall;
        x0 < u && u < x1 && y0 < v && v < y1 && x0 >= 0.0 && y0 >= 0.0 && x1 <= 1.0 && y1 <= 1.0
    }

    #[test]
    fn reads_a_clean_scene_answer() {
        let scene = parse_scene(
            r#"{"scene_type":"Corridor","has_perspective":true,
                "vanishing_point":[0.48,0.51],
                "back_wall":[0.3,0.32,0.66,0.7],
                "objects":[{"label":"  Bench ","box":[0.1,0.5,0.2,0.8],"on_the_ground":true}]}"#,
        )
        .unwrap();

        assert_eq!(scene.scene_type.as_deref(), Some("corridor"));
        assert!(!scene.flat);
        assert_eq!(scene.vanishing_point, (0.48, 0.51));
        assert_eq!(scene.back_wall, (0.3, 0.32, 0.66, 0.7));
        assert_eq!(scene.objects.len(), 1);
        assert_eq!(scene.objects[0].label, "bench");
        assert!(holds(&scene));
    }

    #[test]
    fn digs_the_scene_json_out_of_a_chatty_answer() {
        let scene = parse_scene(
            "Certainly!\n```json\n{\"scene_type\": \"street\", \"vanishing_point\": [0.5, 0.4]}\n```\nHope that helps.",
        )
        .unwrap();
        assert_eq!(scene.scene_type.as_deref(), Some("street"));
        assert_eq!(scene.vanishing_point, (0.5, 0.4));
    }

    #[test]
    fn clamps_a_vanishing_point_outside_the_picture() {
        let scene = parse_scene(r#"{"vanishing_point":[1.7,-0.3]}"#).unwrap();
        assert_eq!(scene.vanishing_point, (1.0 - SCENE_MARGIN, SCENE_MARGIN));
        assert!(holds(&scene));
    }

    #[test]
    fn widens_a_back_wall_that_does_not_hold_the_vanishing_point() {
        let scene = parse_scene(
            r#"{"has_perspective":true,"vanishing_point":[0.8,0.8],"back_wall":[0.1,0.1,0.3,0.3]}"#,
        )
        .unwrap();

        assert!(holds(&scene));
        assert!(scene.back_wall.2 >= 0.8 + SCENE_MIN_GAP);
        assert!(scene.back_wall.3 >= 0.8 + SCENE_MIN_GAP);
    }

    #[test]
    fn sorts_an_inverted_back_wall() {
        let scene = parse_scene(
            r#"{"has_perspective":true,"vanishing_point":[0.5,0.4],"back_wall":[0.8,0.7,0.2,0.1]}"#,
        )
        .unwrap();
        assert_eq!(scene.back_wall, (0.2, 0.1, 0.8, 0.7));
        assert!(holds(&scene));
    }

    #[test]
    fn a_flat_scene_is_reported_rather_than_forced() {
        let scene = parse_scene(
            r#"{"scene_type":"portrait","has_perspective":false,"back_wall":[0,0,1,1]}"#,
        )
        .unwrap();

        assert!(scene.flat);
        assert_eq!(scene.scene_type.as_deref(), Some("portrait"));
        near(scene.back_wall, inset_around(0.5, 0.5));
        assert!(holds(&scene));
    }

    #[test]
    fn missing_fields_fall_back_to_the_centred_default() {
        let scene = parse_scene("{}").unwrap();
        assert_eq!(scene.vanishing_point, (0.5, 0.5));
        assert_eq!(SCENE_INSET, 0.42);
        near(scene.back_wall, (0.29, 0.29, 0.71, 0.71));
        assert!(scene.objects.is_empty());
        assert!(holds(&scene));
    }

    #[test]
    fn drops_objects_with_broken_boxes_and_caps_the_list() {
        let scene = parse_scene(
            r#"{"objects":[
                {"label":"short","box":[0.1,0.2,0.3]},
                {"label":"nan","box":[0.1,null,0.3,0.4]},
                {"label":"empty","box":[0.3,0.3,0.3,0.3]},
                {"label":"everything","box":[0,0,1,1]},
                {"label":"flying","box":[0.1,0.1,0.2,0.2],"on_the_ground":false},
                {"label":"","box":[0.1,0.4,0.2,0.7]},
                {"label":"A","box":[0.1,0.4,0.2,0.7]},
                {"label":"B","box":[0.1,0.4,0.2,0.7]},
                {"label":"C","box":[0.1,0.4,0.2,0.7]},
                {"label":"D","box":[0.1,0.4,0.2,0.7]},
                {"label":"E","box":[0.1,0.4,0.2,0.7]},
                {"label":"F","box":[0.1,0.4,0.2,0.7]},
                {"label":"G","box":[0.1,0.4,0.2,0.7]}
            ]}"#,
        )
        .unwrap();

        assert_eq!(scene.objects.len(), 6);
        assert_eq!(
            scene
                .objects
                .iter()
                .map(|o| o.label.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b", "c", "d", "e", "f"]
        );
    }

    #[test]
    fn a_scene_answer_with_no_json_is_an_error() {
        assert!(parse_scene("I cannot make out any perspective here.").is_err());
        assert!(parse_scene("{\"vanishing_point\": [0.5, 0.5]").is_err());
    }
}
