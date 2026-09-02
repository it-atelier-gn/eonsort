use crate::error::Result;
use crate::tags::Spot;
use std::path::Path;

pub const CREDIT: &str = if cfg!(feature = "faces") {
    "Face detection by YuNet, MIT licensed, Copyright (c) 2020 Shiqi Yu <shiqi.yu@gmail.com>"
} else {
    ""
};

pub const KNOWING_CREDIT: &str = if cfg!(feature = "faces") {
    "Face recognition by SFace, Apache 2.0, Copyright (c) 2021 Zhong Yaoyao, Deng Weihong"
} else {
    ""
};

pub const SMALLEST_SHARE: f32 = 0.025;

pub fn big_enough(width: f32, height: f32, across: f32, down: f32) -> bool {
    let smallest = across.max(down) * SMALLEST_SHARE;
    width >= smallest && height >= smallest
}

pub const FINDING_FILE: &str = "yunet.safetensors";
pub const KNOWING_FILE: &str = "sface.safetensors";

pub fn built_in() -> bool {
    cfg!(feature = "faces")
}

pub fn finding_at(dir: &Path) -> std::path::PathBuf {
    dir.join(FINDING_FILE)
}

pub fn knowing_at(dir: &Path) -> std::path::PathBuf {
    dir.join(KNOWING_FILE)
}

pub fn installed(dir: &Path) -> bool {
    finding_at(dir).is_file() && knowing_at(dir).is_file()
}

#[cfg(feature = "faces")]
fn must_be_there(model: &Path) -> Result<()> {
    if model.is_file() {
        return Ok(());
    }
    Err(crate::error::Error::Tagging(format!(
        "{}: the face model is not installed beside the program",
        model.display()
    )))
}

#[cfg(feature = "faces")]
mod real {
    use super::*;
    use crate::error::Error;
    use crate::yunet::YuNet;

    pub struct Finder {
        net: YuNet,
    }

    impl Finder {
        pub fn load(model: &Path) -> Result<Self> {
            super::must_be_there(model)?;
            YuNet::load_from(model)
                .map(|net| Self { net })
                .map_err(|e| Error::Tagging(e.to_string()))
        }

        pub fn faces_in(&self, picture: &image::DynamicImage) -> Result<Vec<crate::yunet::Face>> {
            self.net.look_at(picture)
        }

        pub fn look(&self, path: &Path) -> Result<Vec<Spot>> {
            let opened = crate::imageio::open_upright(path).ok_or_else(|| {
                Error::Tagging(format!("{}: cannot be read as a picture", path.display()))
            })?;
            let across = opened.width().max(1) as f32;
            let down = opened.height().max(1) as f32;

            Ok(self
                .net
                .look_at(&opened)?
                .into_iter()
                .filter(|face| super::big_enough(face.width, face.height, across, down))
                .map(|face| Spot {
                    x: face.x / across,
                    y: face.y / down,
                    width: face.width / across,
                    height: face.height / down,
                    score: face.score,
                    label: None,
                })
                .collect())
        }
    }
}

#[cfg(feature = "faces")]
mod knowing {
    use super::*;
    use crate::error::Error;
    use crate::sface::{self, SFace};
    use crate::yunet::Face;

    #[derive(Debug)]
    pub struct Recogniser {
        net: SFace,
    }

    impl Recogniser {
        pub fn load(model: &Path) -> Result<Self> {
            super::must_be_there(model)?;
            SFace::load_from(model)
                .map(|net| Self { net })
                .map_err(|e| Error::Tagging(e.to_string()))
        }

        pub fn describe(&self, picture: &image::RgbImage, face: &Face) -> Result<Vec<f32>> {
            let cropped = sface::align(picture, &face.points);
            let mut vector = self
                .net
                .embed(&image::DynamicImage::ImageRgb8(cropped))
                .map_err(|e| Error::Tagging(e.to_string()))?;
            crate::tags::normalise(&mut vector);
            Ok(vector)
        }
    }
}

#[cfg(feature = "faces")]
pub fn study(
    finder: &Finder,
    knowing: Option<&Recogniser>,
    path: &Path,
) -> Result<Vec<crate::tags::Found>> {
    let opened = crate::imageio::open_upright(path).ok_or_else(|| {
        crate::error::Error::Tagging(format!("{}: cannot be read as a picture", path.display()))
    })?;
    let across = opened.width().max(1) as f32;
    let down = opened.height().max(1) as f32;

    let faces = finder.faces_in(&opened)?;
    let picture = knowing.is_some().then(|| opened.to_rgb8());

    let mut held = Vec::with_capacity(faces.len());
    for face in faces {
        if !big_enough(face.width, face.height, across, down) {
            continue;
        }
        let vector = match (knowing, &picture) {
            (Some(knowing), Some(picture)) => knowing.describe(picture, &face)?,
            _ => Vec::new(),
        };
        held.push(crate::tags::Found {
            spot: Spot {
                x: face.x / across,
                y: face.y / down,
                width: face.width / across,
                height: face.height / down,
                score: face.score,
                label: None,
            },
            vector,
        });
    }
    Ok(held)
}

#[cfg(feature = "faces")]
pub use knowing::Recogniser;

#[cfg(feature = "faces")]
pub use real::Finder;

#[cfg(not(feature = "faces"))]
fn without() -> crate::error::Error {
    crate::error::Error::Tagging("this build was made without the face detector".into())
}

#[cfg(not(feature = "faces"))]
pub fn study(
    _finder: &Finder,
    _knowing: Option<&Recogniser>,
    _path: &Path,
) -> Result<Vec<crate::tags::Found>> {
    Err(without())
}

#[cfg(not(feature = "faces"))]
#[derive(Debug)]
pub struct Recogniser;

#[cfg(not(feature = "faces"))]
impl Recogniser {
    pub fn load(_model: &Path) -> Result<Self> {
        Err(without())
    }
}

#[cfg(not(feature = "faces"))]
#[derive(Debug)]
pub struct Finder;

#[cfg(not(feature = "faces"))]
impl Finder {
    pub fn load(_model: &Path) -> Result<Self> {
        Err(crate::error::Error::Tagging(
            "this build was made without the face detector".into(),
        ))
    }

    pub fn look(&self, _path: &Path) -> Result<Vec<Spot>> {
        Err(crate::error::Error::Tagging(
            "this build was made without the face detector".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_credit_is_there_when_the_detector_is() {
        assert_eq!(built_in(), cfg!(feature = "faces"));
        if built_in() {
            assert!(CREDIT.contains("MIT"), "the credit must name the licence");
            assert!(
                CREDIT.contains("Shiqi Yu"),
                "the credit must name the author"
            );
        }
    }

    #[test]
    fn a_face_too_small_to_be_worth_naming_is_ignored() {
        let across = 4000.0;
        let down = 3000.0;
        let smallest = across * SMALLEST_SHARE;

        assert!(big_enough(smallest, smallest, across, down));
        assert!(!big_enough(smallest - 1.0, smallest, across, down));
        assert!(!big_enough(smallest, smallest - 1.0, across, down));
        assert!(!big_enough(0.0, 0.0, across, down));
    }

    #[test]
    fn the_floor_follows_the_longer_edge_whichever_way_the_picture_stands() {
        let upright = big_enough(50.0, 50.0, 3000.0, 4000.0);
        let sideways = big_enough(50.0, 50.0, 4000.0, 3000.0);
        assert_eq!(upright, sideways);
        assert!(!upright);
        assert!(big_enough(120.0, 120.0, 3000.0, 4000.0));
    }

    #[cfg(not(feature = "faces"))]
    #[test]
    fn without_the_detector_looking_says_so_rather_than_finding_nothing() {
        let err = Finder::load(Path::new("nowhere")).unwrap_err().to_string();
        assert!(err.contains("without the face detector"), "{err}");
    }

    #[cfg(feature = "faces")]
    #[test]
    fn the_spots_it_reports_sit_inside_the_picture() {
        let dir = std::env::temp_dir().join("eonsort-faces-probe");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("probe.png");
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("probe-face.png");
        std::fs::copy(&source, &path).unwrap();

        let found = Finder::load(&crate::yunet::beside_the_crate())
            .unwrap()
            .look(&path)
            .unwrap();
        for spot in &found {
            assert!(spot.score >= crate::yunet::SCORE_FLOOR, "{spot:?}");
            assert!(spot.width > 0.0 && spot.height > 0.0, "{spot:?}");
            assert!(
                spot.width.max(spot.height) >= SMALLEST_SHARE,
                "a spot too small to be worth naming got through: {spot:?}"
            );
        }
    }

    #[cfg(feature = "faces")]
    #[test]
    fn the_recogniser_says_where_it_looked_when_the_model_is_missing() {
        let missing = std::path::Path::new("nowhere").join(KNOWING_FILE);
        let err = Recogniser::load(&missing).unwrap_err().to_string();
        assert!(err.contains("not installed beside the program"), "{err}");
        assert!(err.contains(KNOWING_FILE), "{err}");
    }

    #[cfg(feature = "faces")]
    #[test]
    fn the_shipped_weights_are_the_size_they_are_meant_to_be() {
        let path = crate::sface::beside_the_crate();
        let found = std::fs::metadata(&path).unwrap().len();
        assert_eq!(
            found,
            crate::sface::BYTES,
            "{} is not intact",
            path.display()
        );
    }

    #[cfg(feature = "faces")]
    #[test]
    fn the_same_face_described_twice_is_the_same_person() {
        use crate::yunet::Face;

        let mut picture = image::RgbImage::new(160, 160);
        for (x, y, pixel) in picture.enumerate_pixels_mut() {
            let v = ((x * 7 + y * 13) % 255) as u8;
            *pixel = image::Rgb([v, 255 - v, (x % 200) as u8]);
        }
        let face = Face {
            x: 20.0,
            y: 20.0,
            width: 100.0,
            height: 100.0,
            score: 0.9,
            points: crate::sface::TEMPLATE.map(|p| [p[0] + 20.0, p[1] + 20.0]),
        };

        let knowing = Recogniser::load(&crate::sface::beside_the_crate()).unwrap();
        let one = knowing.describe(&picture, &face).unwrap();
        let two = knowing.describe(&picture, &face).unwrap();

        assert_eq!(one.len(), crate::sface::EMBEDDING);
        let length = one.iter().map(|v| v * v).sum::<f32>();
        assert!((length - 1.0).abs() < 1e-4, "the vector is not unit length");
        assert!(crate::sface::same_person(&one, &two), "not itself");
    }
}
