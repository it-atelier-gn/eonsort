use base64::Engine;
use eonsort_core::rotate::{self, Transform};
use serde::Serialize;
use std::fs;
use std::io::Read;
use std::path::Path;

const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_THUMBNAIL_BYTES: u64 = 96 * 1024 * 1024;
const MIN_EMBEDDED_EDGE: u32 = 96;
const TEXT_HEAD_BYTES: usize = 16 * 1024;
const MAX_PREVIEW_EDGE: u32 = 2048;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Preview {
    Image {
        mime: String,
        data: String,
        bytes: u64,
    },
    Video {
        mime: String,
        bytes: u64,
    },
    Audio {
        mime: String,
        bytes: u64,
    },
    Pdf {
        bytes: u64,
    },
    Text {
        head: String,
        bytes: u64,
        truncated: bool,
    },
    Binary {
        bytes: u64,
    },
    Missing,
}

pub fn preview(path: &Path) -> Preview {
    let Ok(meta) = fs::metadata(path) else {
        return Preview::Missing;
    };
    let bytes = meta.len();
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();

    if let Some(mime) = image_mime(&extension) {
        if bytes <= MAX_IMAGE_BYTES {
            if let Ok(raw) = fs::read(path) {
                return Preview::Image {
                    mime: mime.to_string(),
                    data: base64::engine::general_purpose::STANDARD.encode(raw),
                    bytes,
                };
            }
        }
        return Preview::Binary { bytes };
    }

    if needs_decode(&extension) {
        if bytes <= MAX_IMAGE_BYTES {
            if let Some(data) = decode_to_png(path) {
                return Preview::Image {
                    mime: "image/png".to_string(),
                    data,
                    bytes,
                };
            }
        }
        return Preview::Binary { bytes };
    }

    if let Some(mime) = video_mime(&extension) {
        return Preview::Video {
            mime: mime.to_string(),
            bytes,
        };
    }

    if let Some(mime) = audio_mime(&extension) {
        return Preview::Audio {
            mime: mime.to_string(),
            bytes,
        };
    }

    if extension == "pdf" {
        return Preview::Pdf { bytes };
    }

    if is_text(&extension) {
        if let Ok(file) = fs::File::open(path) {
            let mut buffer = Vec::new();
            let mut handle = file.take(TEXT_HEAD_BYTES as u64);
            if handle.read_to_end(&mut buffer).is_ok() {
                return Preview::Text {
                    head: String::from_utf8_lossy(&buffer).into_owned(),
                    bytes,
                    truncated: bytes > buffer.len() as u64,
                };
            }
        }
    }

    Preview::Binary { bytes }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Thumbnail {
    Image {
        data: String,
        width: u32,
        height: u32,
    },
    Playable {
        mime: String,
    },
    None,
}

pub fn thumbnail(path: &Path, edge: u32, transform: Option<Transform>) -> Thumbnail {
    let Ok(meta) = fs::metadata(path) else {
        return Thumbnail::None;
    };
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();

    if let Some(mime) = video_mime(&extension) {
        return Thumbnail::Playable {
            mime: mime.to_string(),
        };
    }

    if extension == "svg" {
        return Thumbnail::None;
    }

    if (image_mime(&extension).is_none() && !needs_decode(&extension))
        || meta.len() > MAX_THUMBNAIL_BYTES
    {
        return Thumbnail::None;
    }

    let edge = edge.clamp(32, 1024);
    let heif = eonsort_core::imageio::is_heif_extension(&extension);
    let decoded = if heif {
        eonsort_core::imageio::open(path)
    } else {
        embedded_thumbnail(path)
            .filter(|image| image.width().max(image.height()) >= MIN_EMBEDDED_EDGE)
            .or_else(|| eonsort_core::imageio::open(path))
    };
    let Some(decoded) = decoded else {
        return Thumbnail::None;
    };
    let turn = transform.unwrap_or_else(|| {
        if heif {
            Transform::None
        } else {
            Transform::for_orientation(rotate::read_orientation(path))
        }
    });
    let small = rotate::applied(decoded.thumbnail(edge, edge), turn).to_rgb8();
    let (width, height) = (small.width(), small.height());

    let mut buffer = Vec::new();
    if image::DynamicImage::ImageRgb8(small)
        .write_to(
            &mut std::io::Cursor::new(&mut buffer),
            image::ImageFormat::Jpeg,
        )
        .is_err()
    {
        return Thumbnail::None;
    }

    Thumbnail::Image {
        data: base64::engine::general_purpose::STANDARD.encode(buffer),
        width,
        height,
    }
}

fn embedded_thumbnail(path: &Path) -> Option<image::DynamicImage> {
    let exif = eonsort_core::exifread::from_path(path)?;
    let offset = exif
        .get_field(exif::Tag::JPEGInterchangeFormat, exif::In::THUMBNAIL)?
        .value
        .get_uint(0)? as usize;
    let length = exif
        .get_field(exif::Tag::JPEGInterchangeFormatLength, exif::In::THUMBNAIL)?
        .value
        .get_uint(0)? as usize;
    let bytes = exif.buf().get(offset..offset.checked_add(length)?)?;
    image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg).ok()
}

fn image_mime(extension: &str) -> Option<&'static str> {
    Some(match extension {
        "png" => "image/png",
        "jpg" | "jpeg" | "jpe" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        _ => return None,
    })
}

fn needs_decode(extension: &str) -> bool {
    matches!(extension, "tif" | "tiff") || eonsort_core::imageio::is_heif_extension(extension)
}

fn decode_to_png(path: &Path) -> Option<String> {
    let img = eonsort_core::imageio::open(path)?;
    let img = if img.width().max(img.height()) > MAX_PREVIEW_EDGE {
        img.thumbnail(MAX_PREVIEW_EDGE, MAX_PREVIEW_EDGE)
    } else {
        img
    };
    let mut buffer = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut buffer),
        image::ImageFormat::Png,
    )
    .ok()?;
    Some(base64::engine::general_purpose::STANDARD.encode(buffer))
}

fn video_mime(extension: &str) -> Option<&'static str> {
    Some(match extension {
        "mp4" | "m4v" | "f4v" => "video/mp4",
        "mov" | "qt" => "video/quicktime",
        "3gp" => "video/3gpp",
        "3g2" => "video/3gpp2",
        "mj2" => "video/mj2",
        _ => return None,
    })
}

fn audio_mime(extension: &str) -> Option<&'static str> {
    Some(match extension {
        "m4a" | "m4b" | "m4p" => "audio/mp4",
        _ => return None,
    })
}

fn is_text(extension: &str) -> bool {
    matches!(
        extension,
        "txt"
            | "md"
            | "markdown"
            | "log"
            | "csv"
            | "tsv"
            | "json"
            | "jsonl"
            | "xml"
            | "yml"
            | "yaml"
            | "toml"
            | "ini"
            | "cfg"
            | "conf"
            | "rs"
            | "py"
            | "js"
            | "ts"
            | "svelte"
            | "html"
            | "css"
            | "sh"
            | "bat"
            | "ps1"
            | "sql"
            | "srt"
            | "vtt"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("eonsort-preview-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reports_a_missing_file() {
        assert_eq!(preview(Path::new("does-not-exist.png")), Preview::Missing);
    }

    #[test]
    fn encodes_a_small_image() {
        let path = temp_dir().join("pixel.png");
        fs::write(&path, b"\x89PNG\r\n\x1a\n").unwrap();

        match preview(&path) {
            Preview::Image { mime, data, bytes } => {
                assert_eq!(mime, "image/png");
                assert_eq!(bytes, 8);
                assert!(!data.is_empty());
            }
            other => panic!("expected an image preview, got {other:?}"),
        }
    }

    #[test]
    fn truncates_a_long_text_file() {
        let path = temp_dir().join("big.txt");
        fs::write(&path, "x".repeat(TEXT_HEAD_BYTES * 2)).unwrap();

        match preview(&path) {
            Preview::Text {
                head, truncated, ..
            } => {
                assert_eq!(head.len(), TEXT_HEAD_BYTES);
                assert!(truncated);
            }
            other => panic!("expected a text preview, got {other:?}"),
        }
    }

    #[test]
    fn falls_back_to_binary_for_unknown_types() {
        let path = temp_dir().join("thing.bin");
        fs::write(&path, b"\x00\x01\x02").unwrap();

        assert_eq!(preview(&path), Preview::Binary { bytes: 3 });
    }

    #[test]
    fn detects_a_video_file() {
        let path = temp_dir().join("clip.mp4");
        fs::write(&path, b"stub").unwrap();

        match preview(&path) {
            Preview::Video { mime, bytes } => {
                assert_eq!(mime, "video/mp4");
                assert_eq!(bytes, 4);
            }
            other => panic!("expected a video preview, got {other:?}"),
        }
    }

    #[test]
    fn detects_an_audio_file() {
        let path = temp_dir().join("clip.m4a");
        fs::write(&path, b"stub").unwrap();

        match preview(&path) {
            Preview::Audio { mime, bytes } => {
                assert_eq!(mime, "audio/mp4");
                assert_eq!(bytes, 4);
            }
            other => panic!("expected an audio preview, got {other:?}"),
        }
    }

    #[test]
    fn detects_a_pdf_file() {
        let path = temp_dir().join("doc.pdf");
        fs::write(&path, b"%PDF-1.4").unwrap();

        assert_eq!(preview(&path), Preview::Pdf { bytes: 8 });
    }

    #[test]
    fn shrinks_a_picture_to_fit_the_requested_edge() {
        let path = temp_dir().join("wide.png");
        image::RgbImage::from_pixel(400, 200, image::Rgb([0, 128, 255]))
            .save(&path)
            .unwrap();

        match thumbnail(&path, 64, None) {
            Thumbnail::Image {
                data,
                width,
                height,
            } => {
                assert_eq!((width, height), (64, 32));
                assert!(!data.is_empty());
            }
            other => panic!("expected an image thumbnail, got {other:?}"),
        }
    }

    #[test]
    fn a_sideways_photo_gets_an_upright_thumbnail() {
        let path = temp_dir().join("portrait.jpg");
        let mut pixels = image::RgbImage::new(64, 32);
        for (x, y, pixel) in pixels.enumerate_pixels_mut() {
            *pixel = image::Rgb([(x % 256) as u8, (y % 256) as u8, 90]);
        }
        let mut body = Vec::new();
        image::DynamicImage::ImageRgb8(pixels)
            .write_to(
                &mut std::io::Cursor::new(&mut body),
                image::ImageFormat::Jpeg,
            )
            .unwrap();
        fs::write(&path, exif_app1(&body, 6)).unwrap();

        match thumbnail(&path, 64, None) {
            Thumbnail::Image { width, height, .. } => assert_eq!((width, height), (32, 64)),
            other => panic!("expected an image thumbnail, got {other:?}"),
        }

        match thumbnail(&path, 64, Some(Transform::None)) {
            Thumbnail::Image { width, height, .. } => assert_eq!((width, height), (64, 32)),
            other => panic!("expected an image thumbnail, got {other:?}"),
        }
    }

    fn exif_app1(jpeg: &[u8], orientation: u16) -> Vec<u8> {
        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"MM");
        tiff.extend_from_slice(&42u16.to_be_bytes());
        tiff.extend_from_slice(&8u32.to_be_bytes());
        tiff.extend_from_slice(&1u16.to_be_bytes());
        tiff.extend_from_slice(&0x0112u16.to_be_bytes());
        tiff.extend_from_slice(&3u16.to_be_bytes());
        tiff.extend_from_slice(&1u32.to_be_bytes());
        tiff.extend_from_slice(&orientation.to_be_bytes());
        tiff.extend_from_slice(&[0, 0]);
        tiff.extend_from_slice(&0u32.to_be_bytes());

        let mut app1 = Vec::new();
        app1.extend_from_slice(b"Exif\0\0");
        app1.extend_from_slice(&tiff);

        let mut out = Vec::new();
        out.extend_from_slice(&jpeg[0..2]);
        out.extend_from_slice(&[0xFF, 0xE1]);
        out.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
        out.extend_from_slice(&app1);
        out.extend_from_slice(&jpeg[2..]);
        out
    }

    #[test]
    fn a_video_is_reported_as_playable_rather_than_decoded() {
        let path = temp_dir().join("thumb.mp4");
        fs::write(&path, b"stub").unwrap();

        assert_eq!(
            thumbnail(&path, 128, None),
            Thumbnail::Playable {
                mime: "video/mp4".to_string()
            }
        );
    }

    #[test]
    fn files_with_nothing_to_show_have_no_thumbnail() {
        let path = temp_dir().join("notes.txt");
        fs::write(&path, b"hello").unwrap();

        assert_eq!(thumbnail(&path, 128, None), Thumbnail::None);
        assert_eq!(
            thumbnail(Path::new("does-not-exist.png"), 128, None),
            Thumbnail::None
        );
    }

    #[test]
    fn decodes_a_tiff_into_png() {
        let path = temp_dir().join("scan.tiff");
        let img = image::RgbImage::from_pixel(2, 2, image::Rgb([255, 0, 0]));
        img.save(&path).unwrap();

        match preview(&path) {
            Preview::Image { mime, data, .. } => {
                assert_eq!(mime, "image/png");
                assert!(!data.is_empty());
            }
            other => panic!("expected an image preview, got {other:?}"),
        }
    }
}
