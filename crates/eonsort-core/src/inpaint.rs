use crate::error::{Error, Result};
use base64::Engine;

pub const BOUNDARY: &str = "eonsortinpaint7f3a9c1b";
pub const DEFAULT_MODEL: &str = "gpt-image-1";
pub const DEFAULT_PROMPT: &str = "continue the background that the removed object was hiding; match the surrounding lighting, colour and texture; add nothing new";
pub const MAX_REPLY: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Edit {
    pub endpoint: String,
    pub key: String,
    pub model: String,
    pub prompt: String,
    pub size: String,
    pub image: Vec<u8>,
    pub mask: Vec<u8>,
}

pub fn edits_url(endpoint: &str) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return "https://api.openai.com/v1/images/edits".into();
    }
    if trimmed.contains("/images/edits") || trimmed.contains("/sdapi/") {
        return trimmed.into();
    }
    if trimmed.ends_with("/v1") {
        return format!("{trimmed}/images/edits");
    }
    format!("{trimmed}/v1/images/edits")
}

pub fn content_type() -> String {
    format!("multipart/form-data; boundary={BOUNDARY}")
}

pub fn multipart(edit: &Edit) -> Vec<u8> {
    let mut body = Vec::with_capacity(edit.image.len() + edit.mask.len() + 1024);

    let mut field = |name: &str, value: &str| {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(value.as_bytes());
        body.extend_from_slice(b"\r\n");
    };

    field("model", &edit.model);
    field("prompt", &edit.prompt);
    field("n", "1");
    field("response_format", "b64_json");
    if !edit.size.is_empty() {
        field("size", &edit.size);
    }

    let mut file = |name: &str, filename: &str, bytes: &[u8]| {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(b"Content-Type: image/png\r\n\r\n");
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    };

    file("image", "photo.png", &edit.image);
    file("mask", "mask.png", &edit.mask);

    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

pub fn decode_reply(body: &str) -> Result<Vec<u8>> {
    let reply: serde_json::Value = serde_json::from_str(body).map_err(|_| {
        Error::Ai(format!(
            "the filling service answered with something other than JSON: {}",
            shorten(body)
        ))
    })?;

    if let Some(message) = reply
        .get("error")
        .and_then(|error| error.get("message").or(Some(error)))
        .and_then(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .or_else(|| Some(value.to_string()))
        })
    {
        return Err(Error::Ai(format!("the filling service refused: {message}")));
    }

    let encoded = reply
        .get("data")
        .and_then(|data| data.get(0))
        .and_then(|first| first.get("b64_json"))
        .and_then(|value| value.as_str())
        .or_else(|| {
            reply
                .get("images")
                .and_then(|images| images.get(0))
                .and_then(|value| value.as_str())
        })
        .ok_or_else(|| {
            Error::Ai(format!(
                "the filling service returned no image: {}",
                shorten(body)
            ))
        })?;

    let cleaned = encoded
        .rsplit_once("base64,")
        .map(|(_, tail)| tail)
        .unwrap_or(encoded);

    base64::engine::general_purpose::STANDARD
        .decode(cleaned.trim())
        .map_err(|e| Error::Ai(format!("the filled image was not readable: {e}")))
}

pub fn fill(edit: &Edit) -> Result<Vec<u8>> {
    use std::io::Read;
    use std::time::Duration;

    let url = edits_url(&edit.endpoint);

    if url.starts_with("https://") && !cfg!(feature = "download") {
        return Err(Error::Ai(
            "this build can only reach a filling service over http://, not https://".into(),
        ));
    }
    if edit.image.is_empty() || edit.mask.is_empty() {
        return Err(Error::Ai("there is nothing to fill".into()));
    }

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_global(Some(Duration::from_secs(300)))
        .http_status_as_error(false)
        .build()
        .into();

    let mut request = agent.post(&url).header("content-type", content_type());
    if !edit.key.trim().is_empty() {
        request = request.header("authorization", format!("Bearer {}", edit.key.trim()));
    }

    let response = request
        .send(&multipart(edit)[..])
        .map_err(|e| Error::Ai(format!("could not reach {url}: {e}")))?;

    let status = response.status().as_u16();
    let mut body = String::new();
    response
        .into_body()
        .into_reader()
        .take(MAX_REPLY)
        .read_to_string(&mut body)
        .map_err(|e| Error::Ai(format!("{url} answered with unreadable text: {e}")))?;

    if !(200..300).contains(&status) {
        return match decode_reply(&body) {
            Err(Error::Ai(message)) => {
                Err(Error::Ai(format!("{url} answered {status}: {message}")))
            }
            _ => Err(Error::Ai(format!(
                "{url} answered {status}: {}",
                shorten(&body)
            ))),
        };
    }

    decode_reply(&body)
}

fn shorten(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "an empty answer".into();
    }
    let taken: String = trimmed.chars().take(200).collect();
    if trimmed.chars().count() > 200 {
        format!("{taken}…")
    } else {
        taken
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn an_edit() -> Edit {
        Edit {
            endpoint: "http://localhost:8080".into(),
            key: "secret".into(),
            model: DEFAULT_MODEL.into(),
            prompt: "fill it".into(),
            size: "512x512".into(),
            image: vec![1, 2, 3],
            mask: vec![4, 5],
        }
    }

    #[test]
    fn a_bare_host_gains_the_edits_path() {
        assert_eq!(
            edits_url("http://localhost:8080"),
            "http://localhost:8080/v1/images/edits"
        );
    }

    #[test]
    fn a_version_prefix_is_not_repeated() {
        assert_eq!(
            edits_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/images/edits"
        );
    }

    #[test]
    fn a_complete_address_is_left_alone() {
        assert_eq!(
            edits_url("http://box/v1/images/edits"),
            "http://box/v1/images/edits"
        );
        assert_eq!(
            edits_url("http://box/sdapi/v1/img2img"),
            "http://box/sdapi/v1/img2img"
        );
    }

    #[test]
    fn an_empty_address_falls_back_to_the_public_one() {
        assert_eq!(edits_url("   "), "https://api.openai.com/v1/images/edits");
    }

    #[test]
    fn the_body_carries_both_pictures_and_closes_itself() {
        let body = multipart(&an_edit());
        let text = String::from_utf8_lossy(&body).into_owned();

        assert!(text.contains("name=\"image\"; filename=\"photo.png\""));
        assert!(text.contains("name=\"mask\"; filename=\"mask.png\""));
        assert!(text.contains("name=\"prompt\""));
        assert!(text.contains("fill it"));
        assert!(text.contains("name=\"size\""));
        assert!(text.ends_with(&format!("--{BOUNDARY}--\r\n")));
        assert_eq!(text.matches(&format!("--{BOUNDARY}\r\n")).count(), 7);
    }

    #[test]
    fn an_unset_size_is_left_out() {
        let mut edit = an_edit();
        edit.size = String::new();
        let text = String::from_utf8_lossy(&multipart(&edit)).into_owned();
        assert!(!text.contains("name=\"size\""));
    }

    #[test]
    fn the_openai_shape_is_understood() {
        let encoded = base64::engine::general_purpose::STANDARD.encode([9u8, 8, 7]);
        let body = format!("{{\"data\":[{{\"b64_json\":\"{encoded}\"}}]}}");
        assert_eq!(decode_reply(&body).unwrap(), vec![9, 8, 7]);
    }

    #[test]
    fn the_diffusion_webui_shape_is_understood() {
        let encoded = base64::engine::general_purpose::STANDARD.encode([1u8]);
        let body = format!("{{\"images\":[\"{encoded}\"]}}");
        assert_eq!(decode_reply(&body).unwrap(), vec![1]);
    }

    #[test]
    fn a_data_url_prefix_is_stripped() {
        let encoded = base64::engine::general_purpose::STANDARD.encode([2u8, 2]);
        let body = format!("{{\"images\":[\"data:image/png;base64,{encoded}\"]}}");
        assert_eq!(decode_reply(&body).unwrap(), vec![2, 2]);
    }

    #[test]
    fn a_refusal_is_reported_with_its_reason() {
        let body = "{\"error\":{\"message\":\"mask must be the same size\"}}";
        let message = decode_reply(body).unwrap_err().to_string();
        assert!(message.contains("mask must be the same size"), "{message}");
    }

    #[test]
    fn prose_is_an_error_not_an_image() {
        assert!(decode_reply("I could not do that").is_err());
    }

    #[test]
    fn an_answer_without_an_image_is_an_error() {
        let message = decode_reply("{\"data\":[]}").unwrap_err().to_string();
        assert!(message.contains("no image"), "{message}");
    }

    #[test]
    fn a_long_answer_is_shortened_for_the_message() {
        let long = "x".repeat(500);
        let message = decode_reply(&long).unwrap_err().to_string();
        assert!(message.contains('…'));
        assert!(message.chars().count() < 300);
    }
}
