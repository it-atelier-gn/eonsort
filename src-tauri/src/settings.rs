use eonsort_core::model::DEFAULT_FOLDER_PATTERN;
use eonsort_core::providers::{Provider, Strategy};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const FILE_NAME: &str = "settings.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub sources: Vec<PathBuf>,
    pub destination: Option<PathBuf>,
    pub folder_pattern: String,
    pub providers: Vec<Provider>,
    pub strategy: Strategy,
    pub follow_symlinks: bool,
    pub jobs: usize,
    pub preserve_times: bool,
    pub compare_hashes: bool,
    pub last_plan: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sources: Vec::new(),
            destination: None,
            folder_pattern: DEFAULT_FOLDER_PATTERN.to_string(),
            providers: Provider::ALL.to_vec(),
            strategy: Strategy::Oldest,
            follow_symlinks: false,
            jobs: eonsort_core::copy::default_concurrency(),
            preserve_times: true,
            compare_hashes: false,
            last_plan: None,
        }
    }
}

pub fn load(app: &AppHandle) -> Settings {
    path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = path(app).ok_or("no configuration directory available")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let raw = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| e.to_string())
}

/// Plans live next to the settings so a run can be resumed after a restart.
pub fn plan_directory(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?.join("plans");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join(FILE_NAME))
}
