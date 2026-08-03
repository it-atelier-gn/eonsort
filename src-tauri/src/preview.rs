use base64::Engine;
use serde::Serialize;
use std::fs;
use std::io::Read;
use std::path::Path;

const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
const TEXT_HEAD_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Preview {
    Image {
        mime: String,
        data: String,
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
}
