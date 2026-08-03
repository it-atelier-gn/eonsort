use super::{Detection, Provider};
use crate::dateparse::parse_date;
use std::path::Path;

pub fn detect(path: &Path) -> Option<Detection> {
    let name = path.file_name()?.to_str()?;
    parse_date(name).map(|taken| Detection {
        provider: Provider::Filename,
        info: Some(name.to_string()),
        taken,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn reads_date_from_file_name_only() {
        let found = detect(Path::new("/archive/2019/IMG_20230506_123456.jpg")).unwrap();
        assert_eq!(
            found.taken,
            NaiveDate::from_ymd_opt(2023, 5, 6)
                .unwrap()
                .and_hms_opt(12, 34, 56)
                .unwrap()
        );
    }

    #[test]
    fn ignores_dates_that_only_appear_in_parent_directories() {
        assert!(detect(Path::new("/archive/2023-05-06/holiday.jpg")).is_none());
    }
}
