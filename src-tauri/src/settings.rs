use eonsort_core::model::DEFAULT_FOLDER_PATTERN;
use eonsort_core::naming::DEFAULT_NAME_PATTERN;
use eonsort_core::providers::{clean_weights, Provider, Strategy, Weights};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

fn yes() -> bool {
    true
}

fn default_name_pattern() -> String {
    DEFAULT_NAME_PATTERN.to_string()
}

const FILE_NAME: &str = "settings.json";
const DIR_NAME: &str = "eonsort";
const ROOT_VAR: &str = "EONSORT_HOME";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub sources: Vec<PathBuf>,
    pub destination: Option<PathBuf>,
    pub folder_pattern: String,
    #[serde(default = "default_name_pattern")]
    pub name_pattern: String,
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
            name_pattern: default_name_pattern(),
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

pub fn data_directory(app: &AppHandle) -> Option<PathBuf> {
    Some(rooted(chosen_root(), &app.path().data_dir().ok()?))
}

pub fn plan_directory(app: &AppHandle) -> Option<PathBuf> {
    let dir = data_directory(app)?.join("plans");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn ours(base: &Path) -> PathBuf {
    base.join(DIR_NAME)
}

fn chosen_root() -> Option<PathBuf> {
    std::env::var_os(ROOT_VAR)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn rooted(chosen: Option<PathBuf>, usual: &Path) -> PathBuf {
    chosen.unwrap_or_else(|| ours(usual))
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    Some(rooted(chosen_root(), &app.path().config_dir().ok()?).join(FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_lives_under_one_folder_named_for_the_program() {
        let base = Path::new("/base");
        assert_eq!(ours(base), Path::new("/base/eonsort"));
        assert_eq!(
            ours(base).join(FILE_NAME),
            Path::new("/base/eonsort/settings.json")
        );
    }

    #[test]
    fn a_folder_of_its_own_takes_the_place_of_the_usual_one() {
        let usual = Path::new("/base");
        assert_eq!(rooted(None, usual), Path::new("/base/eonsort"));
        assert_eq!(
            rooted(Some(PathBuf::from("/elsewhere")), usual),
            Path::new("/elsewhere")
        );
    }

    #[test]
    fn settings_written_before_the_weights_existed_still_read() {
        let held: Settings = serde_json::from_str(r#"{"providers":["exif"]}"#).unwrap();
        assert_eq!(held.providers, vec![Provider::Exif]);
        assert!(held.weights.is_empty());
    }

    #[test]
    fn settings_written_before_the_name_pattern_existed_still_read() {
        let held: Settings = serde_json::from_str(r#"{"providers":["exif"]}"#).unwrap();
        assert_eq!(held.name_pattern, DEFAULT_NAME_PATTERN);
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
