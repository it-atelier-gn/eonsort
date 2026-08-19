use chrono::NaiveDateTime;
use std::ops::Range;

const EXIF_PREFIX: &[u8] = b"Exif\0\0";
const TAG_ORIENTATION: u16 = 0x0112;
const TAG_DATE_TIME: u16 = 0x0132;
const TAG_DATE_TIME_ORIGINAL: u16 = 0x9003;
const TAG_DATE_TIME_DIGITIZED: u16 = 0x9004;
const TYPE_ASCII: u16 = 2;
const TYPE_SHORT: u16 = 3;
const DATE_LEN: usize = 20;
const ENTRY_SIZE: usize = 12;
const TIFF_MAGIC: u16 = 42;

const TAG_EXIF_IFD: u16 = 0x8769;
#[cfg(test)]
const TAG_PIXEL_X: u16 = 0xA002;
#[cfg(test)]
const TAG_PIXEL_Y: u16 = 0xA003;
#[cfg(test)]
const TYPE_LONG: u16 = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Order {
    Little,
    Big,
}

pub fn set_orientation(jpeg: &mut [u8], orientation: u16) -> bool {
    let Some(range) = exif_range(jpeg) else {
        return false;
    };
    let tiff = &mut jpeg[range];
    let Some((order, ifd0)) = header(tiff) else {
        return false;
    };
    let Some(entry) = find_entry(tiff, order, ifd0, TAG_ORIENTATION) else {
        return false;
    };
    if read_u16(tiff, entry + 2, order) != Some(TYPE_SHORT) {
        return false;
    }
    write_u16(tiff, entry + 8, order, orientation)
}

pub fn set_taken(jpeg: &mut [u8], taken: NaiveDateTime) -> bool {
    let Some(range) = exif_range(jpeg) else {
        return false;
    };
    let tiff = &mut jpeg[range];
    let Some((order, ifd0)) = header(tiff) else {
        return false;
    };

    let stamp = format!("{}\0", taken.format("%Y:%m:%d %H:%M:%S"));
    if stamp.len() != DATE_LEN {
        return false;
    }

    let mut targets = vec![(ifd0, TAG_DATE_TIME)];
    if let Some(sub) = find_entry(tiff, order, ifd0, TAG_EXIF_IFD)
        .and_then(|entry| read_u32(tiff, entry + 8, order))
        .map(|at| at as usize)
    {
        targets.push((sub, TAG_DATE_TIME_ORIGINAL));
        targets.push((sub, TAG_DATE_TIME_DIGITIZED));
    }

    let mut written = false;
    for (ifd, tag) in targets {
        written |= write_ascii(tiff, order, ifd, tag, stamp.as_bytes());
    }
    written
}

fn write_ascii(buf: &mut [u8], order: Order, ifd: usize, tag: u16, value: &[u8]) -> bool {
    let Some(entry) = find_entry(buf, order, ifd, tag) else {
        return false;
    };
    if read_u16(buf, entry + 2, order) != Some(TYPE_ASCII)
        || read_u32(buf, entry + 4, order) != Some(value.len() as u32)
    {
        return false;
    }
    let Some(at) = read_u32(buf, entry + 8, order).map(|at| at as usize) else {
        return false;
    };
    let Some(slot) = buf.get_mut(at..at + value.len()) else {
        return false;
    };
    slot.copy_from_slice(value);
    true
}

fn exif_range(jpeg: &[u8]) -> Option<Range<usize>> {
    if jpeg.len() < 4 || jpeg[0] != 0xFF || jpeg[1] != 0xD8 {
        return None;
    }
    let mut pos = 2;
    loop {
        while jpeg.get(pos)? == &0xFF && jpeg.get(pos + 1)? == &0xFF {
            pos += 1;
        }
        if jpeg.get(pos)? != &0xFF {
            return None;
        }
        let marker = *jpeg.get(pos + 1)?;
        pos += 2;

        if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            continue;
        }
        if marker == 0xD9 || marker == 0xDA {
            return None;
        }

        let length = u16::from_be_bytes([*jpeg.get(pos)?, *jpeg.get(pos + 1)?]) as usize;
        if length < 2 {
            return None;
        }
        let payload = pos + 2..pos + length;
        if marker == 0xE1 && jpeg.get(payload.clone())?.starts_with(EXIF_PREFIX) {
            return Some(payload.start + EXIF_PREFIX.len()..payload.end);
        }
        pos += length;
    }
}

fn header(tiff: &[u8]) -> Option<(Order, usize)> {
    let order = match tiff.get(0..2)? {
        b"II" => Order::Little,
        b"MM" => Order::Big,
        _ => return None,
    };
    if read_u16(tiff, 2, order)? != TIFF_MAGIC {
        return None;
    }
    Some((order, read_u32(tiff, 4, order)? as usize))
}

fn find_entry(tiff: &[u8], order: Order, ifd: usize, tag: u16) -> Option<usize> {
    let count = read_u16(tiff, ifd, order)? as usize;
    (0..count)
        .map(|index| ifd + 2 + index * ENTRY_SIZE)
        .find(|entry| read_u16(tiff, *entry, order) == Some(tag))
}

fn read_u16(buf: &[u8], at: usize, order: Order) -> Option<u16> {
    let raw: [u8; 2] = buf.get(at..at + 2)?.try_into().ok()?;
    Some(match order {
        Order::Little => u16::from_le_bytes(raw),
        Order::Big => u16::from_be_bytes(raw),
    })
}

fn read_u32(buf: &[u8], at: usize, order: Order) -> Option<u32> {
    let raw: [u8; 4] = buf.get(at..at + 4)?.try_into().ok()?;
    Some(match order {
        Order::Little => u32::from_le_bytes(raw),
        Order::Big => u32::from_be_bytes(raw),
    })
}

fn write_u16(buf: &mut [u8], at: usize, order: Order, value: u16) -> bool {
    let raw = match order {
        Order::Little => value.to_le_bytes(),
        Order::Big => value.to_be_bytes(),
    };
    match buf.get_mut(at..at + 2) {
        Some(slot) => {
            slot.copy_from_slice(&raw);
            true
        }
        None => false,
    }
}

#[cfg(test)]
pub(crate) fn plain_jpeg(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = image::RgbImage::new(width, height);
    for (x, y, pixel) in pixels.enumerate_pixels_mut() {
        *pixel = image::Rgb([(x % 256) as u8, (y % 256) as u8, 90]);
    }
    let mut out = Vec::new();
    image::DynamicImage::ImageRgb8(pixels)
        .write_to(
            &mut std::io::Cursor::new(&mut out),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
    out
}

#[cfg(test)]
pub(crate) fn jpeg_with_exif(width: u32, height: u32, orientation: u16) -> Vec<u8> {
    let jpeg = plain_jpeg(width, height);

    let ifd0_entries: u16 = 3;
    let ifd0_end = 8 + 2 + usize::from(ifd0_entries) * ENTRY_SIZE + 4;
    let make_at = ifd0_end;
    let sub_ifd_at = make_at + 8;

    let mut tiff = Vec::new();
    tiff.extend_from_slice(b"MM");
    tiff.extend_from_slice(&TIFF_MAGIC.to_be_bytes());
    tiff.extend_from_slice(&8u32.to_be_bytes());
    tiff.extend_from_slice(&ifd0_entries.to_be_bytes());

    tiff.extend_from_slice(&0x010Fu16.to_be_bytes());
    tiff.extend_from_slice(&2u16.to_be_bytes());
    tiff.extend_from_slice(&8u32.to_be_bytes());
    tiff.extend_from_slice(&(make_at as u32).to_be_bytes());

    tiff.extend_from_slice(&TAG_ORIENTATION.to_be_bytes());
    tiff.extend_from_slice(&TYPE_SHORT.to_be_bytes());
    tiff.extend_from_slice(&1u32.to_be_bytes());
    tiff.extend_from_slice(&orientation.to_be_bytes());
    tiff.extend_from_slice(&[0, 0]);

    tiff.extend_from_slice(&TAG_EXIF_IFD.to_be_bytes());
    tiff.extend_from_slice(&TYPE_LONG.to_be_bytes());
    tiff.extend_from_slice(&1u32.to_be_bytes());
    tiff.extend_from_slice(&(sub_ifd_at as u32).to_be_bytes());

    tiff.extend_from_slice(&0u32.to_be_bytes());
    tiff.extend_from_slice(b"eonsort\0");

    tiff.extend_from_slice(&3u16.to_be_bytes());

    tiff.extend_from_slice(&TAG_PIXEL_X.to_be_bytes());
    tiff.extend_from_slice(&TYPE_LONG.to_be_bytes());
    tiff.extend_from_slice(&1u32.to_be_bytes());
    tiff.extend_from_slice(&width.to_be_bytes());

    tiff.extend_from_slice(&TAG_PIXEL_Y.to_be_bytes());
    tiff.extend_from_slice(&TYPE_LONG.to_be_bytes());
    tiff.extend_from_slice(&1u32.to_be_bytes());
    tiff.extend_from_slice(&height.to_be_bytes());

    let date_at = sub_ifd_at + 2 + 3 * ENTRY_SIZE + 4;
    tiff.extend_from_slice(&TAG_DATE_TIME_ORIGINAL.to_be_bytes());
    tiff.extend_from_slice(&TYPE_ASCII.to_be_bytes());
    tiff.extend_from_slice(&(DATE_LEN as u32).to_be_bytes());
    tiff.extend_from_slice(&(date_at as u32).to_be_bytes());

    tiff.extend_from_slice(&0u32.to_be_bytes());
    tiff.extend_from_slice(b"2003:01:01 00:00:12\0");

    let mut app1 = Vec::new();
    app1.extend_from_slice(EXIF_PREFIX);
    app1.extend_from_slice(&tiff);

    let mut out = Vec::new();
    out.extend_from_slice(&jpeg[0..2]);
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(&app1);
    out.extend_from_slice(&jpeg[2..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_back(jpeg: &[u8]) -> Option<(u16, u32, u32, String)> {
        let mut cursor = std::io::Cursor::new(jpeg);
        let exif = exif::Reader::new().read_from_container(&mut cursor).ok()?;
        let field = |tag| {
            exif.get_field(tag, exif::In::PRIMARY)
                .and_then(|f| f.value.get_uint(0))
        };
        Some((
            field(exif::Tag::Orientation)? as u16,
            field(exif::Tag::PixelXDimension)?,
            field(exif::Tag::PixelYDimension)?,
            exif.get_field(exif::Tag::Make, exif::In::PRIMARY)?
                .display_value()
                .to_string(),
        ))
    }

    fn date_of(jpeg: &[u8]) -> Option<String> {
        let mut cursor = std::io::Cursor::new(jpeg);
        let exif = exif::Reader::new().read_from_container(&mut cursor).ok()?;
        Some(
            exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)?
                .display_value()
                .to_string(),
        )
    }

    fn at(y: i32, m: u32, d: u32, hh: u32, mm: u32, ss: u32) -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(hh, mm, ss)
            .unwrap()
    }

    #[test]
    fn stamping_the_taken_date_leaves_the_file_the_same_length() {
        let mut jpeg = jpeg_with_exif(64, 32, 1);
        let before = jpeg.len();

        assert!(set_taken(&mut jpeg, at(2019, 5, 14, 9, 22, 3)));

        assert_eq!(jpeg.len(), before);
        assert_eq!(date_of(&jpeg).as_deref(), Some("2019-05-14 09:22:03"));
        assert_eq!(
            read_back(&jpeg),
            Some((1, 64, 32, "\"eonsort\"".to_string()))
        );
    }

    #[test]
    fn a_jpeg_without_exif_takes_no_date() {
        let mut jpeg = plain_jpeg(16, 16);
        assert!(!set_taken(&mut jpeg, at(2019, 5, 14, 9, 22, 3)));
    }

    #[test]
    fn bytes_that_are_not_a_jpeg_take_no_date() {
        let mut rubbish = b"not a jpeg at all".to_vec();
        assert!(!set_taken(&mut rubbish, at(2019, 5, 14, 9, 22, 3)));
    }

    #[test]
    fn the_fixture_is_readable_by_a_real_exif_parser() {
        let jpeg = jpeg_with_exif(64, 32, 6);
        assert_eq!(
            read_back(&jpeg),
            Some((6, 64, 32, "\"eonsort\"".to_string()))
        );
    }

    #[test]
    fn setting_the_orientation_leaves_everything_else_alone() {
        let mut jpeg = jpeg_with_exif(64, 32, 6);
        let before = jpeg.len();

        assert!(set_orientation(&mut jpeg, 1));

        assert_eq!(jpeg.len(), before);
        assert_eq!(
            read_back(&jpeg),
            Some((1, 64, 32, "\"eonsort\"".to_string()))
        );
    }

    #[test]
    fn a_jpeg_without_exif_is_left_untouched() {
        let mut jpeg = plain_jpeg(16, 16);
        let before = jpeg.clone();

        assert!(!set_orientation(&mut jpeg, 1));
        assert_eq!(jpeg, before);
    }

    #[test]
    fn bytes_that_are_not_a_jpeg_are_refused() {
        let mut nonsense = b"not a jpeg at all".to_vec();
        assert!(!set_orientation(&mut nonsense, 1));

        let mut truncated = vec![0xFF, 0xD8, 0xFF];
        assert!(!set_orientation(&mut truncated, 1));

        let mut empty: Vec<u8> = Vec::new();
        assert!(!set_orientation(&mut empty, 1));
    }

    #[test]
    fn the_orientation_entry_is_found_before_it_is_patched() {
        let mut jpeg = jpeg_with_exif(64, 32, 8);
        let range = exif_range(&jpeg).unwrap();

        let tiff = &mut jpeg[range];
        let (order, ifd0) = header(tiff).unwrap();
        assert!(order == Order::Big);
        let entry = find_entry(tiff, order, ifd0, TAG_ORIENTATION).unwrap();
        assert_eq!(read_u16(tiff, entry + 8, order), Some(8));

        assert!(set_orientation(&mut jpeg, 1));
        assert_eq!(read_back(&jpeg).unwrap().0, 1);
    }

    #[test]
    fn a_lossless_turn_already_swaps_the_recorded_pixel_dimensions() {
        let raw = jpeg_with_exif(64, 32, 6);
        let mut options = turbojpeg::Transform::op(turbojpeg::TransformOp::Rot90);
        options.perfect = true;
        let turned = turbojpeg::transform(&options, &raw).unwrap().to_vec();

        let (_, width, height, _) = read_back(&turned).unwrap();
        assert_eq!((width, height), (32, 64));
    }
}
