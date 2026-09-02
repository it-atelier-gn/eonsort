use crate::error::{Error, Result};
use crate::geocode::Place;
use crate::model::{folder_segment, SUBJECT_TOKEN, UNKNOWN_SUBJECT};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const DEFAULT_NAME_PATTERN: &str = "{original_name}";
const MAX_SEGMENT: usize = 96;
const FORBIDDEN: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    Subject,
    City,
    Region,
    Country,
    CountryCode,
    CameraMake,
    CameraModel,
    OriginalName,
    OriginalStem,
    Extension,
}

impl Token {
    pub const ALL: [Token; 10] = [
        Token::Subject,
        Token::City,
        Token::Region,
        Token::Country,
        Token::CountryCode,
        Token::CameraMake,
        Token::CameraModel,
        Token::OriginalName,
        Token::OriginalStem,
        Token::Extension,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Token::Subject => "subject",
            Token::City => "city",
            Token::Region => "region",
            Token::Country => "country",
            Token::CountryCode => "country_code",
            Token::CameraMake => "camera_make",
            Token::CameraModel => "camera_model",
            Token::OriginalName => "original_name",
            Token::OriginalStem => "original_stem",
            Token::Extension => "ext",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        Token::ALL.into_iter().find(|t| t.label() == text)
    }

    fn slugged(self) -> bool {
        self == Token::Subject
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facts {
    pub subject: Option<String>,
    pub place: Place,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub original_name: Option<String>,
}

impl Facts {
    pub fn for_source(source: &Path) -> Self {
        Self {
            original_name: source
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            ..Self::default()
        }
    }

    pub fn with_subject(mut self, subject: Option<&str>) -> Self {
        self.subject = subject.map(str::to_string);
        self
    }

    pub fn with_place(mut self, place: Place) -> Self {
        self.place = place;
        self
    }

    fn stem_and_extension(&self) -> (Option<String>, Option<String>) {
        let Some(name) = self.original_name.as_deref() else {
            return (None, None);
        };
        match name.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => {
                (Some(stem.to_string()), Some(ext.to_string()))
            }
            _ => (Some(name.to_string()), None),
        }
    }

    fn value(&self, token: Token) -> Option<String> {
        match token {
            Token::Subject => self.subject.clone(),
            Token::City => self.place.city.clone(),
            Token::Region => self.place.region.clone(),
            Token::Country => self.place.country.clone(),
            Token::CountryCode => self.place.country_code.clone(),
            Token::CameraMake => self.camera_make.clone(),
            Token::CameraModel => self.camera_model.clone(),
            Token::OriginalName => self.original_name.clone(),
            Token::OriginalStem => self.stem_and_extension().0,
            Token::Extension => self.stem_and_extension().1,
        }
        .filter(|value| !value.trim().is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    Token(Token),
    Literal(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Chain {
    steps: Vec<Step>,
}

impl Chain {
    fn parse(body: &str) -> Result<Self> {
        let mut steps = Vec::new();
        for part in body.split('|') {
            let part = part.trim();
            if part.is_empty() {
                return Err(Error::InvalidFolderPattern(body.to_string()));
            }
            if let Some(literal) = part.strip_prefix('"').and_then(|p| p.strip_suffix('"')) {
                steps.push(Step::Literal(literal.to_string()));
                continue;
            }
            match Token::parse(part) {
                Some(token) => steps.push(Step::Token(token)),
                None => return Err(Error::InvalidFolderPattern(body.to_string())),
            }
        }
        if steps.is_empty() {
            return Err(Error::InvalidFolderPattern(body.to_string()));
        }
        Ok(Self { steps })
    }

    fn resolve(&self, facts: &Facts) -> Option<String> {
        for step in &self.steps {
            match step {
                Step::Literal(text) => return Some(clean(text, false)),
                Step::Token(token) => {
                    if let Some(value) = facts.value(*token) {
                        return Some(clean(&value, token.slugged()));
                    }
                }
            }
        }
        None
    }

    fn only_subject(&self) -> bool {
        self.steps == vec![Step::Token(Token::Subject)]
    }
}

fn clean(value: &str, slug: bool) -> String {
    if slug {
        return folder_segment(value);
    }
    let stripped: String = value
        .chars()
        .filter(|c| !c.is_control())
        .map(|c| if FORBIDDEN.contains(&c) { '-' } else { c })
        .collect();

    let trimmed = stripped.trim().trim_end_matches('.').trim();
    if trimmed.chars().all(|c| c == '.') {
        return String::new();
    }
    let bounded: String = trimmed.chars().take(MAX_SEGMENT).collect();
    bounded.trim().replace('%', "%%")
}

pub fn resolve(pattern: &str, facts: &Facts) -> Result<String> {
    let mut out = String::with_capacity(pattern.len());
    let mut rest = pattern;

    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            return Err(Error::InvalidFolderPattern(pattern.to_string()));
        };
        let body = &after[..close];
        let chain = Chain::parse(body)?;

        match chain.resolve(facts) {
            Some(value) => out.push_str(&value),
            None if chain.only_subject() => out.push_str(UNKNOWN_SUBJECT),
            None => {}
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

pub fn validate(pattern: &str) -> Result<()> {
    let mut rest = pattern;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            return Err(Error::InvalidFolderPattern(pattern.to_string()));
        };
        Chain::parse(&after[..close])?;
        rest = &after[close + 1..];
    }
    if rest.contains('}') && !pattern.contains('{') {
        return Err(Error::InvalidFolderPattern(pattern.to_string()));
    }
    Ok(())
}

pub fn needs(pattern: &str, token: Token) -> bool {
    let mut rest = pattern;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            return false;
        };
        if let Ok(chain) = Chain::parse(&after[..close]) {
            if chain.steps.contains(&Step::Token(token)) {
                return true;
            }
        }
        rest = &after[close + 1..];
    }
    false
}

pub fn file_name(pattern: &str, facts: &Facts) -> Result<String> {
    let resolved = resolve(pattern, facts)?.replace("%%", "%");
    file_name_from(&resolved, facts).ok_or_else(|| Error::InvalidFolderPattern(pattern.to_string()))
}

pub fn file_name_from(resolved: &str, facts: &Facts) -> Option<String> {
    let cleaned = clean(resolved, false).replace("%%", "%");
    if cleaned.trim().is_empty() {
        return facts.original_name.clone();
    }

    let (_, extension) = facts.stem_and_extension();
    match extension {
        Some(ext)
            if !cleaned
                .to_lowercase()
                .ends_with(&format!(".{}", ext.to_lowercase())) =>
        {
            Some(format!("{cleaned}.{ext}"))
        }
        _ => Some(cleaned),
    }
}

pub fn is_default_name_pattern(pattern: &str) -> bool {
    pattern.trim() == DEFAULT_NAME_PATTERN
}

pub fn subject_token_is_legacy(pattern: &str) -> bool {
    pattern.contains(SUBJECT_TOKEN)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> Facts {
        Facts {
            subject: Some("Beach Sunset".to_string()),
            place: Place {
                city: Some("Munich".to_string()),
                region: Some("Bavaria".to_string()),
                country: Some("Germany".to_string()),
                country_code: Some("DE".to_string()),
            },
            camera_make: Some("Canon".to_string()),
            camera_model: Some("EOS 5D".to_string()),
            original_name: Some("IMG_3900.jpg".to_string()),
        }
    }

    #[test]
    fn a_pattern_without_tokens_is_left_exactly_as_it_was() {
        assert_eq!(resolve("%Y/%m", &facts()).unwrap(), "%Y/%m");
    }

    #[test]
    fn a_token_is_replaced_by_what_the_file_knows() {
        assert_eq!(resolve("%Y/{city}", &facts()).unwrap(), "%Y/Munich");
        assert_eq!(resolve("{country}", &facts()).unwrap(), "Germany");
        assert_eq!(resolve("{camera_model}", &facts()).unwrap(), "EOS 5D");
    }

    #[test]
    fn the_subject_token_still_slugs_the_way_it_always_did() {
        assert_eq!(resolve("{subject}", &facts()).unwrap(), "beach-sunset");
    }

    #[test]
    fn a_missing_subject_still_lands_in_the_named_folder() {
        let empty = Facts::default();
        assert_eq!(resolve("{subject}", &empty).unwrap(), UNKNOWN_SUBJECT);
    }

    #[test]
    fn a_chain_walks_on_until_something_answers() {
        let mut thin = facts();
        thin.place.city = None;
        assert_eq!(resolve("{city|region|country}", &thin).unwrap(), "Bavaria");

        thin.place.region = None;
        assert_eq!(resolve("{city|region|country}", &thin).unwrap(), "Germany");
    }

    #[test]
    fn a_quoted_last_resort_ends_the_chain() {
        let empty = Facts::default();
        assert_eq!(
            resolve(r#"{city|country|"Beats me"}"#, &empty).unwrap(),
            "Beats me"
        );
    }

    #[test]
    fn a_chain_nothing_answers_leaves_an_empty_space() {
        let empty = Facts::default();
        assert_eq!(resolve("%Y/{city|country}", &empty).unwrap(), "%Y/");
    }

    #[test]
    fn a_blank_value_counts_as_no_answer() {
        let mut blank = facts();
        blank.place.city = Some("   ".to_string());
        assert_eq!(resolve("{city|country}", &blank).unwrap(), "Germany");
    }

    #[test]
    fn a_value_can_never_climb_out_of_its_folder() {
        let mut hostile = Facts::default();
        for attempt in ["../../etc", "C:\\Windows", "a/b", "..", ".", "..."] {
            hostile.place.city = Some(attempt.to_string());
            let resolved = resolve("{city}", &hostile).unwrap();
            assert!(
                !resolved.contains(['/', '\\', ':']),
                "{attempt} -> {resolved}"
            );
            assert_ne!(resolved, "..", "{attempt}");
            assert_ne!(resolved, ".", "{attempt}");
        }

        hostile.place.city = Some("..".to_string());
        assert_eq!(resolve("%Y/{city}", &hostile).unwrap(), "%Y/");
    }

    #[test]
    fn a_percent_in_a_value_is_never_read_as_a_date_field() {
        let mut sneaky = Facts::default();
        sneaky.place.city = Some("100%Real".to_string());
        assert_eq!(resolve("{city}", &sneaky).unwrap(), "100%%Real");
    }

    #[test]
    fn a_value_is_held_to_a_sensible_length() {
        let mut long = Facts::default();
        long.place.city = Some("x".repeat(400));
        assert_eq!(resolve("{city}", &long).unwrap().len(), MAX_SEGMENT);
    }

    #[test]
    fn an_unknown_token_is_refused_rather_than_left_in_the_path() {
        assert!(resolve("{nonsense}", &facts()).is_err());
        assert!(validate("{nonsense}").is_err());
    }

    #[test]
    fn an_unclosed_token_is_refused() {
        assert!(resolve("%Y/{city", &facts()).is_err());
        assert!(validate("%Y/{city").is_err());
    }

    #[test]
    fn an_empty_chain_is_refused() {
        assert!(validate("{}").is_err());
        assert!(validate("{city|}").is_err());
    }

    #[test]
    fn every_documented_token_validates() {
        for token in Token::ALL {
            let pattern = format!("{{{}}}", token.label());
            assert!(validate(&pattern).is_ok(), "{pattern}");
        }
    }

    #[test]
    fn a_pattern_says_which_tokens_it_needs() {
        assert!(needs("%Y/{city}", Token::City));
        assert!(needs("{city|country}", Token::Country));
        assert!(!needs("%Y/%m", Token::City));
        assert!(!needs("{subject}", Token::City));
        assert!(needs("{subject}", Token::Subject));
    }

    #[test]
    fn the_default_name_pattern_keeps_the_name_the_file_arrived_with() {
        assert_eq!(
            file_name(DEFAULT_NAME_PATTERN, &facts()).unwrap(),
            "IMG_3900.jpg"
        );
        assert!(is_default_name_pattern(DEFAULT_NAME_PATTERN));
    }

    #[test]
    fn a_name_pattern_keeps_the_extension_without_being_told_to() {
        assert_eq!(
            file_name("{original_stem}-{city}", &facts()).unwrap(),
            "IMG_3900-Munich.jpg"
        );
    }

    #[test]
    fn a_name_pattern_that_names_the_extension_does_not_get_it_twice() {
        assert_eq!(
            file_name("{original_stem}.{ext}", &facts()).unwrap(),
            "IMG_3900.jpg"
        );
    }

    #[test]
    fn a_name_pattern_that_resolves_to_nothing_falls_back_to_the_original() {
        let bare = Facts {
            original_name: Some("holiday.jpg".to_string()),
            ..Facts::default()
        };
        assert_eq!(file_name("{city}", &bare).unwrap(), "holiday.jpg");
    }

    #[test]
    fn a_file_with_no_extension_keeps_none() {
        let mut plain = facts();
        plain.original_name = Some("README".to_string());
        assert_eq!(file_name("{original_stem}", &plain).unwrap(), "README");
    }

    #[test]
    fn a_dotfile_is_not_mistaken_for_an_extension() {
        let hidden = Facts {
            original_name: Some(".gitignore".to_string()),
            ..Facts::default()
        };
        assert_eq!(file_name("{original_stem}", &hidden).unwrap(), ".gitignore");
    }

    #[test]
    fn facts_read_the_name_off_the_source_path() {
        let facts = Facts::for_source(Path::new("/src/holiday/IMG_1.jpg"));
        assert_eq!(facts.original_name.as_deref(), Some("IMG_1.jpg"));
        assert_eq!(facts.value(Token::OriginalStem).as_deref(), Some("IMG_1"));
        assert_eq!(facts.value(Token::Extension).as_deref(), Some("jpg"));
    }

    #[test]
    fn a_percent_survives_a_name_pattern_as_itself() {
        let odd = Facts {
            original_name: Some("50%.jpg".to_string()),
            ..Facts::default()
        };
        assert_eq!(file_name("{original_stem}", &odd).unwrap(), "50%.jpg");
    }

    #[test]
    fn the_legacy_subject_token_is_recognised() {
        assert!(subject_token_is_legacy("%Y/{subject}"));
        assert!(!subject_token_is_legacy("%Y/%m"));
    }
}
