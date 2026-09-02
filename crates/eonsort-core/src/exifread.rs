use ::exif::{Error, Exif, Reader};
use std::io::{BufRead, BufReader, Seek};
use std::path::Path;

pub fn from_path(path: &Path) -> Option<Exif> {
    let file = std::fs::File::open(path).ok()?;
    from_reader(&mut BufReader::new(file))
}

pub fn from_bytes(bytes: &[u8]) -> Option<Exif> {
    from_reader(&mut std::io::Cursor::new(bytes))
}

pub fn from_reader<R: BufRead + Seek>(reader: &mut R) -> Option<Exif> {
    match Reader::new()
        .continue_on_error(true)
        .read_from_container(reader)
    {
        Ok(exif) => Some(exif),
        Err(Error::PartialResult(partial)) => {
            let (exif, _) = partial.into_inner();
            Some(exif)
        }
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::exif::{In, Tag};

    fn dated_jpeg() -> Vec<u8> {
        crate::exif_write::jpeg_with_exif(16, 16, 1)
    }

    #[test]
    fn reads_a_sound_block() {
        let exif = from_bytes(&dated_jpeg()).expect("the block is sound");
        assert!(exif.get_field(Tag::DateTimeOriginal, In::PRIMARY).is_some());
    }

    #[test]
    fn a_file_that_is_not_a_picture_reads_as_nothing() {
        assert!(from_bytes(b"not a picture at all").is_none());
        assert!(from_bytes(&[]).is_none());
    }

    #[test]
    fn a_missing_file_reads_as_nothing() {
        assert!(from_path(Path::new("/nowhere/at/all.jpg")).is_none());
    }

    #[test]
    fn a_block_the_strict_reader_refuses_still_gives_up_its_fields() {
        let mut jpeg = dated_jpeg();
        let tiff = tiff_start(&jpeg).expect("the fixture carries a block");
        let next = tiff + 8 + 2 + 3 * 12;
        jpeg[next..next + 4].copy_from_slice(&0xDEAD_BEEFu32.to_be_bytes());

        assert!(
            Reader::new()
                .read_from_container(&mut std::io::Cursor::new(&jpeg))
                .is_err(),
            "the strict reader should refuse this"
        );

        let exif = from_bytes(&jpeg).expect("the tolerant reader keeps what it parsed");
        assert!(exif.get_field(Tag::DateTimeOriginal, In::PRIMARY).is_some());
    }

    fn tiff_start(jpeg: &[u8]) -> Option<usize> {
        let mut at = 2;
        while at + 4 <= jpeg.len() {
            if jpeg[at] != 0xFF {
                return None;
            }
            let marker = jpeg[at + 1];
            if marker == 0xD9 || marker == 0xDA {
                return None;
            }
            let length = u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]) as usize;
            if marker == 0xE1 && jpeg[at + 4..].starts_with(b"Exif\0\0") {
                return Some(at + 4 + 6);
            }
            at += 2 + length;
        }
        None
    }
}
