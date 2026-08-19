use super::Detection;
use chrono::{DateTime, Local, NaiveDateTime};
use std::fs::Metadata;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

const HANDLER_TIMEOUT: Duration = Duration::from_secs(3);
const SAME_MOMENT_SECONDS: i64 = 2;

pub fn detect(path: &Path, meta: &Metadata) -> Option<Detection> {
    let (name, taken) = behind_a_timeout(path)?;
    if echoes_the_file(taken, meta) {
        return None;
    }

    Some(Detection {
        provider: super::Provider::System,
        info: Some(name),
        taken,
    })
}

fn echoes_the_file(taken: NaiveDateTime, meta: &Metadata) -> bool {
    [meta.created().ok(), meta.modified().ok()]
        .into_iter()
        .flatten()
        .filter_map(wall_clock)
        .any(|stamp| (stamp - taken).num_seconds().abs() <= SAME_MOMENT_SECONDS)
}

fn wall_clock(time: SystemTime) -> Option<NaiveDateTime> {
    Some(DateTime::<Local>::from(time).naive_local())
}

fn behind_a_timeout(path: &Path) -> Option<(String, NaiveDateTime)> {
    let owned = path.to_path_buf();
    let (sender, receiver) = mpsc::channel();

    std::thread::Builder::new()
        .name("system-properties".into())
        .spawn(move || {
            let _ = sender.send(ask(&owned));
        })
        .ok()?;

    receiver.recv_timeout(HANDLER_TIMEOUT).ok().flatten()
}

#[cfg(not(any(windows, target_os = "macos")))]
fn ask(_path: &Path) -> Option<(String, NaiveDateTime)> {
    None
}

#[cfg(windows)]
fn ask(path: &Path) -> Option<(String, NaiveDateTime)> {
    windows_shell::ask(path)
}

#[cfg(target_os = "macos")]
fn ask(path: &Path) -> Option<(String, NaiveDateTime)> {
    spotlight::ask(path)
}

#[cfg(windows)]
mod windows_shell {
    use chrono::{DateTime, NaiveDateTime};
    use std::path::Path;
    use windows::core::{GUID, HSTRING, PCWSTR};
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::System::Com::StructuredStorage::PropVariantToFileTime;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    use windows::Win32::System::Variant::PSTF_LOCAL;
    use windows::Win32::UI::Shell::PropertiesSystem::{
        IPropertyStore, SHGetPropertyStoreFromParsingName, GPS_DEFAULT,
    };

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

    pub fn ask(path: &Path) -> Option<(String, NaiveDateTime)> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let found = property_store(path);
            CoUninitialize();
            found
        }
    }

    unsafe fn property_store(path: &Path) -> Option<(String, NaiveDateTime)> {
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
    }
}

#[cfg(target_os = "macos")]
mod spotlight {
    use chrono::{DateTime, Local, NaiveDateTime};
    use std::ffi::c_void;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    type CFTypeRef = *const c_void;

    const UTF8: u32 = 0x0800_0100;
    const SECONDS_TO_UNIX: i64 = 978_307_200;
    const ATTRIBUTE: &str = "kMDItemContentCreationDate";

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringCreateWithBytes(
            allocator: CFTypeRef,
            bytes: *const u8,
            length: isize,
            encoding: u32,
            is_external: u8,
        ) -> CFTypeRef;
        fn CFURLCreateFromFileSystemRepresentation(
            allocator: CFTypeRef,
            buffer: *const u8,
            length: isize,
            is_directory: u8,
        ) -> CFTypeRef;
        fn CFRelease(cf: CFTypeRef);
        fn CFGetTypeID(cf: CFTypeRef) -> usize;
        fn CFDateGetTypeID() -> usize;
        fn CFDateGetAbsoluteTime(date: CFTypeRef) -> f64;
    }

    #[link(name = "CoreServices", kind = "framework")]
    extern "C" {
        fn MDItemCreateWithURL(allocator: CFTypeRef, url: CFTypeRef) -> CFTypeRef;
        fn MDItemCopyAttribute(item: CFTypeRef, name: CFTypeRef) -> CFTypeRef;
    }

    pub fn ask(path: &Path) -> Option<(String, NaiveDateTime)> {
        let bytes = path.as_os_str().as_bytes();

        unsafe {
            let url = CFURLCreateFromFileSystemRepresentation(
                std::ptr::null(),
                bytes.as_ptr(),
                bytes.len() as isize,
                0,
            );
            if url.is_null() {
                return None;
            }

            let item = MDItemCreateWithURL(std::ptr::null(), url);
            CFRelease(url);
            if item.is_null() {
                return None;
            }

            let taken = attribute(item, ATTRIBUTE);
            CFRelease(item);
            taken.map(|taken| (ATTRIBUTE.to_string(), taken))
        }
    }

    unsafe fn attribute(item: CFTypeRef, name: &str) -> Option<NaiveDateTime> {
        let key = CFStringCreateWithBytes(
            std::ptr::null(),
            name.as_ptr(),
            name.len() as isize,
            UTF8,
            0,
        );
        if key.is_null() {
            return None;
        }

        let value = MDItemCopyAttribute(item, key);
        CFRelease(key);
        if value.is_null() {
            return None;
        }

        let taken = (CFGetTypeID(value) == CFDateGetTypeID())
            .then(|| local(CFDateGetAbsoluteTime(value)))
            .flatten();
        CFRelease(value);
        taken
    }

    fn local(seconds: f64) -> Option<NaiveDateTime> {
        let unix = seconds as i64 + SECONDS_TO_UNIX;
        DateTime::from_timestamp(unix, 0).map(|stamped| stamped.with_timezone(&Local).naive_local())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn wall(unix: i64) -> NaiveDateTime {
            DateTime::from_timestamp(unix, 0)
                .unwrap()
                .with_timezone(&Local)
                .naive_local()
        }

        #[test]
        fn counts_from_the_epoch_apple_counts_from() {
            assert_eq!(local(0.0), Some(wall(SECONDS_TO_UNIX)));
            assert_eq!(
                DateTime::from_timestamp(SECONDS_TO_UNIX, 0)
                    .unwrap()
                    .date_naive(),
                chrono::NaiveDate::from_ymd_opt(2001, 1, 1).unwrap()
            );
        }

        #[test]
        fn turns_an_absolute_time_into_the_wall_clock_it_stands_for() {
            let seconds = (1_557_818_523 - SECONDS_TO_UNIX) as f64;
            assert_eq!(local(seconds), Some(wall(1_557_818_523)));
        }

        #[test]
        fn a_file_spotlight_has_never_seen_says_nothing() {
            assert!(ask(Path::new("/nowhere/at/all.jpg")).is_none());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn at(y: i32, m: u32, d: u32, hh: u32, mm: u32, ss: u32) -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(hh, mm, ss)
            .unwrap()
    }

    #[test]
    fn a_file_with_nothing_to_read_says_nothing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        fs::write(&path, b"no dates in here").unwrap();
        let meta = fs::metadata(&path).unwrap();

        assert!(detect(&path, &meta).is_none());
    }

    #[test]
    fn a_file_that_is_not_there_says_nothing() {
        let dir = tempdir().unwrap();
        let stand_in = dir.path().join("present.txt");
        fs::write(&stand_in, b"x").unwrap();
        let meta = fs::metadata(&stand_in).unwrap();

        assert!(detect(Path::new("/nowhere/at/all.jpg"), &meta).is_none());
    }

    #[test]
    fn a_date_that_only_repeats_the_file_times_is_not_worth_reporting() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("plain.txt");
        fs::write(&path, b"x").unwrap();
        let meta = fs::metadata(&path).unwrap();

        let modified = wall_clock(meta.modified().unwrap()).unwrap();
        assert!(echoes_the_file(modified, &meta));
        assert!(echoes_the_file(
            modified + chrono::TimeDelta::seconds(SAME_MOMENT_SECONDS),
            &meta
        ));
        assert!(!echoes_the_file(at(2003, 1, 1, 0, 0, 12), &meta));
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn a_picture_the_desktop_can_date_is_read_through_it() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("dated.jpg");
        fs::write(&path, crate::exif_write::jpeg_with_exif(64, 32, 1)).unwrap();
        let meta = fs::metadata(&path).unwrap();

        let found = detect(&path, &meta).expect("the desktop should date a jpeg carrying EXIF");
        assert_eq!(found.provider, super::super::Provider::System);
        assert_eq!(
            found.taken,
            chrono::NaiveDate::from_ymd_opt(2003, 1, 1)
                .unwrap()
                .and_hms_opt(0, 0, 12)
                .unwrap()
        );
    }
}
