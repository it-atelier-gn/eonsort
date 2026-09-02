use eonsort_core::model::DEFAULT_FOLDER_PATTERN;
use eonsort_core::providers::{clean_weights, Provider, Strategy, Weights};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

fn yes() -> bool {
    true
}

const FILE_NAME: &str = "settings.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub sources: Vec<PathBuf>,
    pub destination: Option<PathBuf>,
    pub folder_pattern: String,
    pub providers: Vec<Provider>,
    pub strategy: Strategy,
    #[serde(default)]
    pub weights: Weights,
    pub follow_symlinks: bool,
    pub auto_rotate: bool,
    #[serde(default = "yes")]
    pub pair_companions: bool,
    #[serde(default)]
    pub tag_pictures: bool,
    #[serde(default)]
    pub rate_quality: bool,
    #[serde(default)]
    pub find_faces: bool,
    #[serde(default)]
    pub name_places: bool,
    pub preserve_times: bool,
    #[serde(default)]
    pub stamp_date: bool,
    #[serde(default)]
    pub write_sidecars: bool,
    pub compare_hashes: bool,
    pub last_plan: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sources: Vec::new(),
            destination: None,
            folder_pattern: DEFAULT_FOLDER_PATTERN.to_string(),
            providers: Provider::DEFAULT.to_vec(),
            strategy: Strategy::default(),
            weights: Weights::new(),
            follow_symlinks: false,
            auto_rotate: false,
            pair_companions: true,
            tag_pictures: false,
            rate_quality: false,
            find_faces: false,
            name_places: false,
            preserve_times: true,
            stamp_date: false,
            write_sidecars: false,
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
    let settings = &Settings {
        weights: clean_weights(&settings.weights),
        ..settings.clone()
    };
    let path = path(app).ok_or("no configuration directory available")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let raw = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| e.to_string())
}

pub fn plan_directory(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?.join("plans");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join(FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_written_before_the_weights_existed_still_read() {
        let held: Settings = serde_json::from_str(r#"{"providers":["exif"]}"#).unwrap();
        assert_eq!(held.providers, vec![Provider::Exif]);
        assert!(held.weights.is_empty());
    }

    #[test]
    fn a_weight_of_your_own_survives_the_round_trip() {
        let mut settings = Settings::default();
        settings.weights.insert(Provider::Filename, 90);
        let raw = serde_json::to_string(&settings).unwrap();
        let read: Settings = serde_json::from_str(&raw).unwrap();
        assert_eq!(read.weights[&Provider::Filename], 90);
    }

    #[test]
    fn a_weight_off_the_scale_is_pulled_back_in_before_it_is_written() {
        let mut settings = Settings::default();
        settings.weights.insert(Provider::Filename, 4000);
        assert_eq!(clean_weights(&settings.weights)[&Provider::Filename], 100);
    }
}
