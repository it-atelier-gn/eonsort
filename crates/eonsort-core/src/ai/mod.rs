mod prompt;
mod transport;

pub use prompt::{parse_reading, Reading};
pub use transport::{probe, pull, remove, PullProgress};

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

pub const DEFAULT_ENDPOINT: &str = "http://localhost:11434";
pub const DEFAULT_VISION_MODEL: &str = "qwen2.5vl";
pub const DEFAULT_EMBED_MODEL: &str = "nomic-embed-text";
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 120;
pub const MAX_IMAGE_EDGE: u32 = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Api {
    #[default]
    Ollama,
    OpenAi,
}

impl Api {
    pub fn label(self) -> &'static str {
        match self {
            Api::Ollama => "ollama",
            Api::OpenAi => "openai-compatible",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub api: Api,
    pub vision_model: String,
    pub embed_model: String,
    pub vision_in_scan: bool,
    pub timeout_seconds: u64,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: DEFAULT_ENDPOINT.to_string(),
            api: Api::default(),
            vision_model: DEFAULT_VISION_MODEL.to_string(),
            embed_model: DEFAULT_EMBED_MODEL.to_string(),
            vision_in_scan: false,
            timeout_seconds: DEFAULT_TIMEOUT_SECONDS,
        }
    }
}

impl AiConfig {
    pub fn base(&self) -> &str {
        self.endpoint.trim_end_matches('/')
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_seconds.clamp(5, 3600))
    }

    pub fn usable(&self) -> bool {
        self.enabled && !self.endpoint.trim().is_empty()
    }
}

pub struct Client {
    config: AiConfig,
}

impl Client {
    pub fn new(config: AiConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &AiConfig {
        &self.config
    }

    pub fn read_image(&self, path: &Path) -> Result<Reading> {
        if !self.config.usable() {
            return Err(Error::AiDisabled);
        }
        let image = encode_image(path)?;
        let answer = transport::vision(&self.config, &image, prompt::READ_IMAGE)?;
        parse_reading(&answer)
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        if !self.config.usable() {
            return Err(Error::AiDisabled);
        }
        transport::embed(&self.config, text)
    }
}

pub fn encode_image(path: &Path) -> Result<String> {
    use base64::Engine;

    let decoded = image::open(path).map_err(|e| Error::Ai(format!("{}: {e}", path.display())))?;
    let scaled = if decoded.width().max(decoded.height()) > MAX_IMAGE_EDGE {
        decoded.thumbnail(MAX_IMAGE_EDGE, MAX_IMAGE_EDGE)
    } else {
        decoded
    };

    let mut buffer = std::io::Cursor::new(Vec::new());
    scaled
        .to_rgb8()
        .write_to(&mut buffer, image::ImageFormat::Png)
        .map_err(|e| Error::Ai(e.to_string()))?;

    Ok(base64::engine::general_purpose::STANDARD.encode(buffer.into_inner()))
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut left = 0.0f32;
    let mut right = 0.0f32;
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        left += x * x;
        right += y * y;
    }
    let magnitude = (left.sqrt()) * (right.sqrt());
    if magnitude == 0.0 {
        0.0
    } else {
        dot / magnitude
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_disabled_client_refuses_rather_than_dialling_out() {
        let client = Client::new(AiConfig::default());
        assert!(matches!(client.embed("anything"), Err(Error::AiDisabled)));
    }

    #[test]
    fn an_enabled_config_with_a_blank_endpoint_is_not_usable() {
        let config = AiConfig {
            enabled: true,
            endpoint: "   ".into(),
            ..AiConfig::default()
        };
        assert!(!config.usable());
    }

    #[test]
    fn trims_a_trailing_slash_off_the_endpoint() {
        let config = AiConfig {
            endpoint: "http://localhost:11434/".into(),
            ..AiConfig::default()
        };
        assert_eq!(config.base(), "http://localhost:11434");
    }

    #[test]
    fn keeps_the_timeout_inside_sane_bounds() {
        let quick = AiConfig {
            timeout_seconds: 0,
            ..AiConfig::default()
        };
        assert_eq!(quick.timeout(), Duration::from_secs(5));

        let forever = AiConfig {
            timeout_seconds: 99_999,
            ..AiConfig::default()
        };
        assert_eq!(forever.timeout(), Duration::from_secs(3600));
    }

    #[test]
    fn cosine_similarity_scores_identical_and_opposite_vectors() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!((cosine(&[1.0, 0.0], &[-1.0, 0.0]) + 1.0).abs() < 1e-6);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
    }

    #[test]
    fn mismatched_or_empty_vectors_score_zero() {
        assert_eq!(cosine(&[1.0, 2.0], &[1.0]), 0.0);
        assert_eq!(cosine(&[], &[]), 0.0);
        assert_eq!(cosine(&[0.0, 0.0], &[0.0, 0.0]), 0.0);
    }
}
