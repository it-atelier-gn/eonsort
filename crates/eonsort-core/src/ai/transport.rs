use super::{AiConfig, Api};
use crate::error::{Error, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PullProgress {
    pub status: String,
    pub completed: u64,
    pub total: u64,
}

pub fn vision(config: &AiConfig, image_base64: &str, instruction: &str) -> Result<String> {
    match config.api {
        Api::Ollama => {
            let body = json!({
                "model": config.vision_model,
                "prompt": instruction,
                "images": [image_base64],
                "stream": false,
                "format": "json",
            });
            let answer = post(config, "/api/generate", body)?;
            text_at(&answer, &["response"])
        }
        Api::OpenAi => {
            let body = json!({
                "model": config.vision_model,
                "messages": [{
                    "role": "user",
                    "content": [
                        {"type": "text", "text": instruction},
                        {"type": "image_url", "image_url": {
                            "url": format!("data:image/png;base64,{image_base64}")
                        }},
                    ],
                }],
                "stream": false,
            });
            let answer = post(config, "/v1/chat/completions", body)?;
            answer
                .pointer("/choices/0/message/content")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| Error::Ai(format!("unexpected answer shape: {answer}")))
        }
    }
}

pub fn probe(config: &AiConfig) -> Result<Vec<String>> {
    if !config.usable() {
        return Err(Error::AiDisabled);
    }
    let (path, pointer, key) = match config.api {
        Api::Ollama => ("/api/tags", "/models", "name"),
        Api::OpenAi => ("/v1/models", "/data", "id"),
    };

    let answer = get(config, path)?;
    let entries = answer
        .pointer(pointer)
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Ai(format!("unexpected model list: {answer}")))?;

    Ok(entries
        .iter()
        .filter_map(|entry| entry.get(key).and_then(Value::as_str))
        .map(str::to_string)
        .collect())
}

pub fn pull(
    config: &AiConfig,
    model: &str,
    cancel: &AtomicBool,
    on_progress: &dyn Fn(PullProgress),
) -> Result<()> {
    let model = require_manageable(config, model)?;
    let url = format!("{}/api/pull", config.base());

    let response = download_agent()
        .post(&url)
        .header("content-type", "application/json")
        .send_json(json!({"model": model, "stream": true}))
        .map_err(|e| unreachable(config, e))?;

    let mut finished = false;
    for line in BufReader::new(response.into_body().into_reader()).lines() {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let line = line.map_err(|e| Error::Ai(format!("the download stopped early: {e}")))?;
        if line.trim().is_empty() {
            continue;
        }
        let update: Value = serde_json::from_str(&line)
            .map_err(|e| Error::Ai(format!("could not read the download progress: {e}")))?;

        if let Some(message) = update.get("error").and_then(Value::as_str) {
            return Err(Error::Ai(message.to_string()));
        }

        let status = update
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        finished |= status == "success";
        on_progress(PullProgress {
            status: status.to_string(),
            completed: number_at(&update, "completed"),
            total: number_at(&update, "total"),
        });
    }

    if finished {
        Ok(())
    } else {
        Err(Error::Ai(format!(
            "{model} did not finish downloading; nothing was installed"
        )))
    }
}

pub fn remove(config: &AiConfig, model: &str) -> Result<()> {
    let model = require_manageable(config, model)?;
    let url = format!("{}/api/delete", config.base());

    let request = ureq::http::Request::delete(&url)
        .header("content-type", "application/json")
        .body(json!({"model": model}).to_string())
        .map_err(|e| Error::Ai(e.to_string()))?;

    agent(config).run(request).map_err(|e| match e {
        ureq::Error::StatusCode(404) => Error::Ai(format!("{model} is not installed")),
        other => unreachable(config, other),
    })?;
    Ok(())
}

fn require_manageable<'a>(config: &AiConfig, model: &'a str) -> Result<&'a str> {
    if !config.usable() {
        return Err(Error::AiDisabled);
    }
    if config.api != Api::Ollama {
        return Err(Error::Ai(
            "only an Ollama runner can install and remove models from here".into(),
        ));
    }
    let model = model.trim();
    if model.is_empty() {
        return Err(Error::Ai("name a model first".into()));
    }
    Ok(model)
}

fn number_at(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn post(config: &AiConfig, path: &str, body: Value) -> Result<Value> {
    let url = format!("{}{path}", config.base());
    agent(config)
        .post(&url)
        .header("content-type", "application/json")
        .send_json(&body)
        .map_err(|e| unreachable(config, e))?
        .body_mut()
        .read_json::<Value>()
        .map_err(|e| Error::Ai(format!("could not read the answer from {url}: {e}")))
}

fn get(config: &AiConfig, path: &str) -> Result<Value> {
    let url = format!("{}{path}", config.base());
    agent(config)
        .get(&url)
        .call()
        .map_err(|e| unreachable(config, e))?
        .body_mut()
        .read_json::<Value>()
        .map_err(|e| Error::Ai(format!("could not read the answer from {url}: {e}")))
}

fn agent(config: &AiConfig) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(config.timeout()))
        .build()
        .into()
}

fn download_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .build()
        .into()
}

fn unreachable(config: &AiConfig, error: ureq::Error) -> Error {
    Error::Ai(format!(
        "could not reach the {} model runner at {}: {error}",
        config.api.label(),
        config.base()
    ))
}

fn text_at(value: &Value, path: &[&str]) -> Result<String> {
    let mut cursor = value;
    for key in path {
        cursor = cursor
            .get(key)
            .ok_or_else(|| Error::Ai(format!("unexpected answer shape: {value}")))?;
    }
    cursor
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| Error::Ai(format!("unexpected answer shape: {value}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(api: Api) -> AiConfig {
        AiConfig {
            enabled: true,
            api,
            ..AiConfig::default()
        }
    }

    #[test]
    fn refuses_to_probe_while_disabled() {
        assert!(matches!(
            probe(&AiConfig::default()),
            Err(Error::AiDisabled)
        ));
    }

    #[test]
    fn refuses_to_install_or_remove_while_disabled() {
        let off = AiConfig::default();
        let idle = AtomicBool::new(false);
        assert!(matches!(
            pull(&off, "qwen2.5vl", &idle, &|_| {}),
            Err(Error::AiDisabled)
        ));
        assert!(matches!(remove(&off, "qwen2.5vl"), Err(Error::AiDisabled)));
    }

    #[test]
    fn only_ollama_can_install_and_remove() {
        let settings = config(Api::OpenAi);
        match remove(&settings, "qwen2.5vl") {
            Err(Error::Ai(message)) => assert!(message.contains("Ollama"), "{message}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn asks_for_a_name_before_dialling_out() {
        let settings = config(Api::Ollama);
        match remove(&settings, "   ") {
            Err(Error::Ai(message)) => assert!(message.contains("name a model"), "{message}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_cancelled_download_never_opens_a_connection() {
        let mut settings = config(Api::Ollama);
        settings.endpoint = "http://127.0.0.1:1".into();
        let stopped = AtomicBool::new(true);
        assert!(matches!(
            pull(&settings, "qwen2.5vl", &stopped, &|_| {}),
            Err(Error::Ai(_))
        ));
    }

    #[test]
    fn reports_an_unreachable_runner_by_name_and_address() {
        let mut settings = config(Api::Ollama);
        settings.endpoint = "http://127.0.0.1:1".into();
        settings.timeout_seconds = 5;

        match probe(&settings) {
            Err(Error::Ai(message)) => {
                assert!(message.contains("ollama"), "{message}");
                assert!(message.contains("127.0.0.1:1"), "{message}");
            }
            other => panic!("expected an unreachable error, got {other:?}"),
        }
    }
}
