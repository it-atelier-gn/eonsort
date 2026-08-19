#[cfg(not(windows))]
pub fn detect(_path: &std::path::Path) -> Option<super::Detection> {
    None
}

#[cfg(windows)]
pub use shell::detect;

#[cfg(windows)]
mod shell {
    use super::super::{Detection, Provider};
    use chrono::{DateTime, NaiveDateTime};
    use std::path::Path;
    use std::sync::mpsc;
    use std::time::Duration;
    use windows::core::{GUID, HSTRING, PCWSTR};
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::System::Com::StructuredStorage::PropVariantToFileTime;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    use windows::Win32::System::Variant::PSTF_LOCAL;
    use windows::Win32::UI::Shell::PropertiesSystem::{
        IPropertyStore, SHGetPropertyStoreFromParsingName, GPS_DEFAULT,
    };

    const HANDLER_TIMEOUT: Duration = Duration::from_secs(3);
    const TICKS_PER_SECOND: i64 = 10_000_000;
    const SECONDS_TO_UNIX: i64 = 11_644_473_600;

    const KEYS: [(&str, PROPERTYKEY); 2] = [
        (
            "System.Photo.DateTaken",
            PROPERTYKEY {
                fmtid: GUID::from_u128(0x14b81da1_0135_4d31_96d9_6cbfc9671a99),
                pid: 36867,
            },
        ),
        (
            "System.Media.DateEncoded",
            PROPERTYKEY {
                fmtid: GUID::from_u128(0x2e4b640d_5019_46d8_8881_55414cc5caa0),
                pid: 100,
            },
        ),
    ];

    pub fn detect(path: &Path) -> Option<Detection> {
        let (name, taken) = asked(path)?;
        Some(Detection {
            provider: Provider::Windows,
            info: Some(name),
            taken,
        })
    }

    fn asked(path: &Path) -> Option<(String, NaiveDateTime)> {
        let owned = path.to_path_buf();
        let (sender, receiver) = mpsc::channel();

        std::thread::Builder::new()
            .name("windows-properties".into())
            .spawn(move || {
                let found = read(&owned);
                let _ = sender.send(found);
            })
            .ok()?;

        receiver.recv_timeout(HANDLER_TIMEOUT).ok().flatten()
    }

    fn read(path: &Path) -> Option<(String, NaiveDateTime)> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let found = ask_shell(path);
            CoUninitialize();
            found
        }
    }

    unsafe fn ask_shell(path: &Path) -> Option<(String, NaiveDateTime)> {
        let wide = HSTRING::from(path.as_os_str());
        let store: IPropertyStore =
            SHGetPropertyStoreFromParsingName(PCWSTR(wide.as_ptr()), None, GPS_DEFAULT).ok()?;

        KEYS.iter().find_map(|(name, key)| {
            let value = store.GetValue(key).ok()?;
            let stamp = PropVariantToFileTime(&value, PSTF_LOCAL).ok()?;
            let ticks = ((stamp.dwHighDateTime as u64) << 32 | stamp.dwLowDateTime as u64) as i64;
            local(ticks).map(|taken| ((*name).to_string(), taken))
        })
    }

    fn local(ticks: i64) -> Option<NaiveDateTime> {
        if ticks <= 0 {
            return None;
        }
        let unix = ticks / TICKS_PER_SECOND - SECONDS_TO_UNIX;
        DateTime::from_timestamp(unix, 0).map(|stamped| stamped.naive_utc())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use chrono::NaiveDate;
        use std::fs;
        use tempfile::tempdir;

        #[test]
        fn turns_a_file_time_into_the_wall_clock_it_stands_for() {
            let ticks = (1_557_818_523 + SECONDS_TO_UNIX) * TICKS_PER_SECOND;
            assert_eq!(
                local(ticks),
                Some(
                    NaiveDate::from_ymd_opt(2019, 5, 14)
                        .unwrap()
                        .and_hms_opt(7, 22, 3)
                        .unwrap()
                )
            );
        }

        #[test]
        fn an_unset_file_time_is_no_date() {
            assert!(local(0).is_none());
            assert!(local(-1).is_none());
        }

        #[test]
        fn a_picture_windows_can_date_is_read_through_the_shell() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("dated.jpg");
            fs::write(&path, crate::exif_write::jpeg_with_exif(64, 32, 1)).unwrap();

            let found = detect(&path).expect("the shell should date a jpeg carrying EXIF");
            assert_eq!(found.provider, Provider::Windows);
            assert_eq!(found.info.as_deref(), Some("System.Photo.DateTaken"));
            assert_eq!(
                found.taken,
                NaiveDate::from_ymd_opt(2003, 1, 1)
                    .unwrap()
                    .and_hms_opt(0, 0, 12)
                    .unwrap()
            );
        }

        #[test]
        fn a_file_with_nothing_to_read_says_nothing() {
            let dir = tempdir().unwrap();
            let path = dir.path().join("notes.txt");
            fs::write(&path, b"no dates in here").unwrap();

            assert!(detect(&path).is_none());
        }

        #[test]
        fn a_file_that_is_not_there_says_nothing() {
            assert!(detect(Path::new("Z:/nowhere/at/all.jpg")).is_none());
        }
    }
}
