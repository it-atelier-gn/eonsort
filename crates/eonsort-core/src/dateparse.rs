use chrono::{NaiveDate, NaiveDateTime};
use regex::Regex;
use std::sync::LazyLock;

const MIN_YEAR: i32 = 1970;
const MAX_YEAR: i32 = 2100;

const YEAR: &str = r"(?P<y>(?:19|20)\d{2})";
const MONTH: &str = r"(?P<mo>0[1-9]|1[0-2])";
const DAY: &str = r"(?P<d>0[1-9]|[12]\d|3[01])";
const HOUR: &str = r"(?P<h>[01]\d|2[0-3])";
const MINUTE: &str = r"(?P<mi>[0-5]\d)";
const SECOND: &str = r"(?P<s>[0-5]\d)";
const LEFT: &str = r"(?:\A|\D)";
const RIGHT: &str = r"(?:\D|\z)";

static PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    let sources = [
        format!("{YEAR}[-:._]{MONTH}[-:._]{DAY}\\D{{1,6}}{HOUR}[-:._]{MINUTE}[-:._]{SECOND}"),
        format!("{LEFT}{YEAR}{MONTH}{DAY}[-_.]{HOUR}{MINUTE}{SECOND}{RIGHT}"),
        format!("{LEFT}{YEAR}{MONTH}{DAY}{HOUR}{MINUTE}{SECOND}{RIGHT}"),
        format!("{LEFT}{YEAR}[-:._]{MONTH}[-:._]{DAY}{RIGHT}"),
        format!("{LEFT}{DAY}[-.]{MONTH}[-.]{YEAR}{RIGHT}"),
        format!("{LEFT}{YEAR}{MONTH}{DAY}{RIGHT}"),
    ];
    sources
        .iter()
        .map(|s| Regex::new(s).expect("date pattern must compile"))
        .collect()
});

pub fn parse_date(text: &str) -> Option<NaiveDateTime> {
    if text.is_empty() {
        return None;
    }
    PATTERNS
        .iter()
        .find_map(|re| re.captures_iter(text).find_map(|caps| build(&caps)))
}

fn build(caps: &regex::Captures<'_>) -> Option<NaiveDateTime> {
    let num = |name: &str| caps.name(name).and_then(|m| m.as_str().parse::<u32>().ok());
    let year = caps.name("y")?.as_str().parse::<i32>().ok()?;
    if !(MIN_YEAR..=MAX_YEAR).contains(&year) {
        return None;
    }
    let date = NaiveDate::from_ymd_opt(year, num("mo")?, num("d")?)?;
    date.and_hms_opt(
        num("h").unwrap_or(0),
        num("mi").unwrap_or(0),
        num("s").unwrap_or(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, mo, d)
            .unwrap()
            .and_hms_opt(h, mi, s)
            .unwrap()
    }

    #[test]
    fn parses_iso_datetime() {
        assert_eq!(
            parse_date("2023-05-06 12:34:56"),
            Some(dt(2023, 5, 6, 12, 34, 56))
        );
        assert_eq!(
            parse_date("2023-05-06T12:34:56"),
            Some(dt(2023, 5, 6, 12, 34, 56))
        );
    }

    #[test]
    fn parses_exif_colon_datetime() {
        assert_eq!(
            parse_date("2019:11:02 08:15:30"),
            Some(dt(2019, 11, 2, 8, 15, 30))
        );
    }

    #[test]
    fn parses_compact_filename_datetime() {
        assert_eq!(
            parse_date("IMG_20230506_123456.jpg"),
            Some(dt(2023, 5, 6, 12, 34, 56))
        );
        assert_eq!(
            parse_date("VID-20230506-123456.mp4"),
            Some(dt(2023, 5, 6, 12, 34, 56))
        );
        assert_eq!(
            parse_date("20230506123456.jpg"),
            Some(dt(2023, 5, 6, 12, 34, 56))
        );
    }

    #[test]
    fn parses_screenshot_with_at_separator() {
        assert_eq!(
            parse_date("Screenshot 2021-07-04 at 09.05.01.png"),
            Some(dt(2021, 7, 4, 9, 5, 1))
        );
    }

    #[test]
    fn parses_date_only_variants() {
        assert_eq!(
            parse_date("IMG_20230506.jpg"),
            Some(dt(2023, 5, 6, 0, 0, 0))
        );
        assert_eq!(
            parse_date("scan 2023-05-06.pdf"),
            Some(dt(2023, 5, 6, 0, 0, 0))
        );
        assert_eq!(
            parse_date("scan 2023_05_06.pdf"),
            Some(dt(2023, 5, 6, 0, 0, 0))
        );
        assert_eq!(
            parse_date("scan 2023.05.06.pdf"),
            Some(dt(2023, 5, 6, 0, 0, 0))
        );
    }

    #[test]
    fn parses_european_date() {
        assert_eq!(
            parse_date("Rechnung 06.05.2023.pdf"),
            Some(dt(2023, 5, 6, 0, 0, 0))
        );
        assert_eq!(
            parse_date("Rechnung 06-05-2023.pdf"),
            Some(dt(2023, 5, 6, 0, 0, 0))
        );
    }

    #[test]
    fn prefers_datetime_over_date_only() {
        assert_eq!(
            parse_date("2023-05-06 report 2020-01-01 11:22:33"),
            Some(dt(2020, 1, 1, 11, 22, 33))
        );
    }

    #[test]
    fn rejects_invalid_and_absent_dates() {
        assert_eq!(parse_date(""), None);
        assert_eq!(parse_date("holiday-photo.jpg"), None);
        assert_eq!(parse_date("20230230.jpg"), None);
        assert_eq!(parse_date("20231345.jpg"), None);
        assert_eq!(parse_date("1234567890.log"), None);
    }

    #[test]
    fn ignores_digits_glued_to_a_candidate() {
        assert_eq!(parse_date("991220230506.bin"), None);
    }

    #[test]
    fn rejects_year_out_of_range() {
        assert_eq!(parse_date("1850-05-06 12:00:00"), None);
    }
}
