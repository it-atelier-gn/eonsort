use crate::error::{Error, Result};
use crate::exif_write;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const LOSSLESS_EXTENSIONS: [&str; 3] = ["jpg", "jpeg", "jpe"];
const REENCODE_QUALITY: u8 = 92;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transform {
    #[default]
    None,
    Rotate90,
    Rotate180,
    Rotate270,
    FlipH,
    FlipV,
    Transpose,
    Transverse,
}

impl Transform {
    pub fn for_orientation(orientation: u16) -> Transform {
        match orientation {
            2 => Transform::FlipH,
            3 => Transform::Rotate180,
            4 => Transform::FlipV,
            5 => Transform::Transpose,
            6 => Transform::Rotate90,
            7 => Transform::Transverse,
            8 => Transform::Rotate270,
            _ => Transform::None,
        }
    }

    fn parts(self) -> (u8, bool) {
        match self {
            Transform::None => (0, false),
            Transform::Rotate90 => (1, false),
            Transform::Rotate180 => (2, false),
            Transform::Rotate270 => (3, false),
            Transform::FlipH => (0, true),
            Transform::Transverse => (1, true),
            Transform::FlipV => (2, true),
            Transform::Transpose => (3, true),
        }
    }

    fn from_parts(quarters: u8, mirrored: bool) -> Transform {
        match (quarters % 4, mirrored) {
            (0, false) => Transform::None,
            (1, false) => Transform::Rotate90,
            (2, false) => Transform::Rotate180,
            (3, false) => Transform::Rotate270,
            (0, true) => Transform::FlipH,
            (1, true) => Transform::Transverse,
            (2, true) => Transform::FlipV,
            _ => Transform::Transpose,
        }
    }

    pub fn turn(self, quarter_turns: i8) -> Transform {
        let (quarters, mirrored) = self.parts();
        let shifted = (i16::from(quarters) + i16::from(quarter_turns)).rem_euclid(4) as u8;
        Transform::from_parts(shifted, mirrored)
    }

    pub fn swaps_axes(self) -> bool {
        self.parts().0 % 2 == 1
    }

    pub fn is_identity(self) -> bool {
        self == Transform::None
    }

    pub fn describe(self) -> &'static str {
        match self {
            Transform::None => "left as it is",
            Transform::Rotate90 => "turned a quarter to the right",
            Transform::Rotate180 => "turned upside down",
            Transform::Rotate270 => "turned a quarter to the left",
            Transform::FlipH => "mirrored left to right",
            Transform::FlipV => "mirrored top to bottom",
            Transform::Transpose => "mirrored along the main diagonal",
            Transform::Transverse => "mirrored along the other diagonal",
        }
    }

    fn operation(self) -> turbojpeg::TransformOp {
        use turbojpeg::TransformOp;
        match self {
            Transform::None => TransformOp::None,
            Transform::Rotate90 => TransformOp::Rot90,
            Transform::Rotate180 => TransformOp::Rot180,
            Transform::Rotate270 => TransformOp::Rot270,
            Transform::FlipH => TransformOp::Hflip,
            Transform::FlipV => TransformOp::Vflip,
            Transform::Transpose => TransformOp::Transpose,
            Transform::Transverse => TransformOp::Transverse,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Written {
    pub size: u64,
    pub hash: String,
}

pub fn lossless_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| LOSSLESS_EXTENSIONS.contains(&e.as_str()))
}

pub fn read_orientation(path: &Path) -> u16 {
    let Ok(file) = fs::File::open(path) else {
        return 1;
    };
    let mut reader = std::io::BufReader::new(file);
    let Some(exif) = crate::exifread::from_reader(&mut reader) else {
        return 1;
    };
    exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .and_then(|field| field.value.get_uint(0))
        .filter(|value| (1..=8).contains(value))
        .map(|value| value as u16)
        .unwrap_or(1)
}

pub fn applied(image: image::DynamicImage, transform: Transform) -> image::DynamicImage {
    match transform {
        Transform::None => image,
        Transform::Rotate90 => image.rotate90(),
        Transform::Rotate180 => image.rotate180(),
        Transform::Rotate270 => image.rotate270(),
        Transform::FlipH => image.fliph(),
        Transform::FlipV => image.flipv(),
        Transform::Transpose => image.fliph().rotate270(),
        Transform::Transverse => image.fliph().rotate90(),
    }
}

pub fn losslessly(source: &Path, bytes: &[u8], transform: Transform) -> Result<Vec<u8>> {
    if !lossless_extension(source) {
        return Err(Error::RotationNotLossless(source.to_path_buf()));
    }
    let mut options = turbojpeg::Transform::op(transform.operation());
    options.perfect = true;

    let turned = turbojpeg::transform(&options, bytes)
        .map_err(|_| Error::RotationNotLossless(source.to_path_buf()))?;

    let mut out = turned.to_vec();
    exif_write::set_orientation(&mut out, 1);
    Ok(out)
}

fn reencoded(source: &Path, bytes: &[u8], transform: Transform) -> Result<Vec<u8>> {
    let format = image::ImageFormat::from_path(source).map_err(|e| Error::Rotation {
        path: source.to_path_buf(),
        message: e.to_string(),
    })?;
    let decoded =
        image::load_from_memory_with_format(bytes, format).map_err(|e| Error::Rotation {
            path: source.to_path_buf(),
            message: e.to_string(),
        })?;

    let turned = applied(decoded, transform);

    let mut out = Vec::new();
    let result = if format == image::ImageFormat::Jpeg {
        turned.write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut std::io::Cursor::new(&mut out),
            REENCODE_QUALITY,
        ))
    } else {
        turned.write_to(&mut std::io::Cursor::new(&mut out), format)
    };
    result.map_err(|e| Error::Rotation {
        path: source.to_path_buf(),
        message: e.to_string(),
    })?;
    Ok(out)
}

pub fn render(source: &Path, transform: Transform, reencode: bool) -> Result<Vec<u8>> {
    let bytes = fs::read(source).map_err(|e| Error::io(source, e))?;
    match losslessly(source, &bytes, transform) {
        Ok(turned) => Ok(turned),
        Err(error) => {
            if reencode {
                reencoded(source, &bytes, transform)
            } else {
                Err(error)
            }
        }
    }
}

pub fn write_rotated(
    source: &Path,
    temp: &Path,
    transform: Transform,
    reencode: bool,
) -> Result<Written> {
    let turned = render(source, transform, reencode)?;
    fs::write(temp, &turned).map_err(|e| Error::io(temp, e))?;
    fs::OpenOptions::new()
        .write(true)
        .open(temp)
        .and_then(|f| f.sync_all())
        .map_err(|e| Error::io(temp, e))?;

    Ok(Written {
        size: turned.len() as u64,
        hash: blake3::hash(&turned).to_hex().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exif_write::{jpeg_with_exif, plain_jpeg};
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn pixels(jpeg: &[u8]) -> image::RgbImage {
        image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg)
            .unwrap()
            .to_rgb8()
    }

    #[test]
    fn a_turn_moves_the_corner_it_is_meant_to() {
        let mut image = image::RgbImage::from_pixel(4, 2, image::Rgb([0, 0, 0]));
        image.put_pixel(3, 0, image::Rgb([255, 0, 0]));
        let image = image::DynamicImage::ImageRgb8(image);

        let sideways = applied(image.clone(), Transform::Rotate90);
        assert_eq!((sideways.width(), sideways.height()), (2, 4));
        assert_eq!(sideways.to_rgb8().get_pixel(1, 3), &image::Rgb([255, 0, 0]));

        assert_eq!(
            applied(image.clone(), Transform::None).to_rgb8(),
            image.to_rgb8()
        );
    }

    #[test]
    fn every_exif_orientation_maps_to_the_turn_that_rights_it() {
        let expected = [
            (1, Transform::None),
            (2, Transform::FlipH),
            (3, Transform::Rotate180),
            (4, Transform::FlipV),
            (5, Transform::Transpose),
            (6, Transform::Rotate90),
            (7, Transform::Transverse),
            (8, Transform::Rotate270),
        ];
        for (orientation, transform) in expected {
            assert_eq!(Transform::for_orientation(orientation), transform);
        }
    }

    #[test]
    fn an_unknown_orientation_means_no_turn() {
        assert_eq!(Transform::for_orientation(0), Transform::None);
        assert_eq!(Transform::for_orientation(9), Transform::None);
        assert_eq!(Transform::for_orientation(65535), Transform::None);
    }

    #[test]
    fn turning_right_four_times_comes_back_to_the_start() {
        for start in [
            Transform::None,
            Transform::Rotate90,
            Transform::FlipH,
            Transform::Transpose,
        ] {
            let mut turned = start;
            for _ in 0..4 {
                turned = turned.turn(1);
            }
            assert_eq!(turned, start);
        }
    }

    #[test]
    fn turning_keeps_a_mirror_that_is_already_there() {
        assert_eq!(Transform::FlipH.turn(1), Transform::Transverse);
        assert_eq!(Transform::FlipH.turn(2), Transform::FlipV);
        assert_eq!(Transform::FlipH.turn(3), Transform::Transpose);
        assert_eq!(Transform::Transpose.turn(1), Transform::FlipH);
    }

    #[test]
    fn turning_left_is_the_opposite_of_turning_right() {
        assert_eq!(Transform::None.turn(-1), Transform::Rotate270);
        assert_eq!(Transform::None.turn(1).turn(-1), Transform::None);
        assert_eq!(Transform::Rotate90.turn(-1), Transform::None);
    }

    #[test]
    fn only_quarter_turns_swap_the_sides() {
        assert!(Transform::Rotate90.swaps_axes());
        assert!(Transform::Rotate270.swaps_axes());
        assert!(Transform::Transpose.swaps_axes());
        assert!(Transform::Transverse.swaps_axes());

        assert!(!Transform::None.swaps_axes());
        assert!(!Transform::Rotate180.swaps_axes());
        assert!(!Transform::FlipH.swaps_axes());
        assert!(!Transform::FlipV.swaps_axes());
    }

    #[test]
    fn a_quarter_turn_swaps_the_sides_of_the_picture() {
        let source = PathBuf::from("holiday.jpg");
        let jpeg = plain_jpeg(64, 32);

        let turned = losslessly(&source, &jpeg, Transform::Rotate90).unwrap();

        let image = pixels(&turned);
        assert_eq!((image.width(), image.height()), (32, 64));
    }

    #[test]
    fn four_quarter_turns_give_the_original_picture_back() {
        let source = PathBuf::from("holiday.jpg");
        let original = plain_jpeg(64, 32);

        let mut turned = original.clone();
        for _ in 0..4 {
            turned = losslessly(&source, &turned, Transform::Rotate90).unwrap();
        }

        assert_eq!(pixels(&turned), pixels(&original));
    }

    #[test]
    fn two_quarter_turns_are_the_same_as_a_half_turn() {
        let source = PathBuf::from("holiday.jpg");
        let original = plain_jpeg(64, 32);

        let once = losslessly(&source, &original, Transform::Rotate90).unwrap();
        let twice = losslessly(&source, &once, Transform::Rotate90).unwrap();
        let half = losslessly(&source, &original, Transform::Rotate180).unwrap();

        assert_eq!(pixels(&twice), pixels(&half));
    }

    #[test]
    fn a_turned_picture_is_marked_as_already_upright() {
        let source = PathBuf::from("holiday.jpg");
        let jpeg = jpeg_with_exif(64, 32, 6);
        assert_eq!(read_orientation_of(&jpeg), 6);

        let turned = losslessly(&source, &jpeg, Transform::Rotate90).unwrap();

        assert_eq!(read_orientation_of(&turned), 1);
    }

    #[test]
    fn a_turned_picture_keeps_the_rest_of_its_exif() {
        let source = PathBuf::from("holiday.jpg");
        let jpeg = jpeg_with_exif(64, 32, 6);

        let turned = losslessly(&source, &jpeg, Transform::Rotate90).unwrap();

        let mut cursor = std::io::Cursor::new(&turned);
        let exif = exif::Reader::new()
            .read_from_container(&mut cursor)
            .unwrap();
        let make = exif
            .get_field(exif::Tag::Make, exif::In::PRIMARY)
            .unwrap()
            .display_value()
            .to_string();
        assert_eq!(make, "\"eonsort\"");

        let width = exif
            .get_field(exif::Tag::PixelXDimension, exif::In::PRIMARY)
            .unwrap()
            .value
            .get_uint(0)
            .unwrap();
        let height = exif
            .get_field(exif::Tag::PixelYDimension, exif::In::PRIMARY)
            .unwrap()
            .value
            .get_uint(0)
            .unwrap();
        assert_eq!((width, height), (32, 64));
    }

    #[test]
    fn a_picture_with_ragged_edges_is_refused_rather_than_spoiled() {
        let source = PathBuf::from("holiday.jpg");
        let jpeg = plain_jpeg(99, 49);

        let refused = losslessly(&source, &jpeg, Transform::Rotate90);

        assert!(matches!(refused, Err(Error::RotationNotLossless(_))));
    }

    #[test]
    fn a_ragged_picture_can_still_be_turned_by_re_encoding_it() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("holiday.jpg");
        fs::write(&source, plain_jpeg(99, 49)).unwrap();

        assert!(render(&source, Transform::Rotate90, false).is_err());

        let turned = render(&source, Transform::Rotate90, true).unwrap();
        let image = pixels(&turned);
        assert_eq!((image.width(), image.height()), (49, 99));
    }

    #[test]
    fn a_png_is_only_turned_when_re_encoding_is_allowed() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("drawing.png");
        image::RgbImage::from_pixel(20, 10, image::Rgb([10, 20, 30]))
            .save(&source)
            .unwrap();

        assert!(matches!(
            render(&source, Transform::Rotate90, false),
            Err(Error::RotationNotLossless(_))
        ));

        let turned = render(&source, Transform::Rotate90, true).unwrap();
        let image = image::load_from_memory_with_format(&turned, image::ImageFormat::Png).unwrap();
        assert_eq!((image.width(), image.height()), (10, 20));
    }

    #[test]
    fn a_progressive_jpeg_still_turns_losslessly() {
        let source = PathBuf::from("holiday.jpg");
        let mut options = turbojpeg::Transform::op(turbojpeg::TransformOp::None);
        options.progressive = true;
        let progressive = turbojpeg::transform(&options, &plain_jpeg(64, 32))
            .unwrap()
            .to_vec();

        let turned = losslessly(&source, &progressive, Transform::Rotate90).unwrap();

        let image = pixels(&turned);
        assert_eq!((image.width(), image.height()), (32, 64));
    }

    #[test]
    fn writing_a_turned_copy_reports_what_landed_on_disk() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("holiday.jpg");
        let temp = dir.path().join("holiday.part");
        fs::write(&source, jpeg_with_exif(64, 32, 6)).unwrap();

        let written = write_rotated(&source, &temp, Transform::Rotate90, false).unwrap();

        let landed = fs::read(&temp).unwrap();
        assert_eq!(written.size, landed.len() as u64);
        assert_eq!(written.hash, blake3::hash(&landed).to_hex().to_string());
        assert_eq!(read_orientation(&temp), 1);
    }

    #[test]
    fn the_orientation_of_a_file_without_exif_reads_as_upright() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("holiday.jpg");
        fs::write(&source, plain_jpeg(16, 16)).unwrap();

        assert_eq!(read_orientation(&source), 1);
        assert_eq!(read_orientation(&dir.path().join("missing.jpg")), 1);
    }

    #[test]
    fn the_orientation_is_read_from_the_file() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("holiday.jpg");
        fs::write(&source, jpeg_with_exif(64, 32, 8)).unwrap();

        assert_eq!(read_orientation(&source), 8);
    }

    #[test]
    fn only_jpeg_files_take_the_lossless_path() {
        assert!(lossless_extension(Path::new("a.jpg")));
        assert!(lossless_extension(Path::new("a.JPEG")));
        assert!(lossless_extension(Path::new("a.jpe")));
        assert!(!lossless_extension(Path::new("a.png")));
        assert!(!lossless_extension(Path::new("a.tiff")));
        assert!(!lossless_extension(Path::new("a")));
    }

    fn read_orientation_of(jpeg: &[u8]) -> u16 {
        let mut cursor = std::io::Cursor::new(jpeg);
        exif::Reader::new()
            .read_from_container(&mut cursor)
            .ok()
            .and_then(|exif| {
                exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
                    .and_then(|f| f.value.get_uint(0))
            })
            .unwrap_or(1) as u16
    }
}
