use crate::preview::{preview, Preview};
use crate::settings::{self, Settings};
use crate::state::AppState;
use eonsort_core::copy::{self, CopyOptions, CopyProgress, CopyReport, Outcome};
use eonsort_core::model::PlanEntry;
use eonsort_core::providers::{DetectOptions, Provider, Strategy};
use eonsort_core::scan::{ScanOptions, ScanProgress};
use eonsort_core::verify::{VerifyOptions, VerifyProgress, VerifyReport};
use eonsort_core::{default_plan_name, read_plan, validate_folder_pattern, Plan};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

const PROGRESS_INTERVAL: Duration = Duration::from_millis(60);

#[derive(Debug, Clone, Deserialize)]
pub struct ScanRequest {
    pub sources: Vec<PathBuf>,
    pub destination: PathBuf,
    pub folder_pattern: String,
    pub providers: Vec<Provider>,
    pub strategy: Strategy,
    pub follow_symlinks: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanSummary {
    pub plan_path: String,
    pub sources: Vec<PathBuf>,
    pub destination: PathBuf,
    pub folder_pattern: String,
    pub files: u64,
    pub bytes: u64,
    pub skipped: u64,
    pub folders: u64,
    pub copied: u64,
    pub duplicates: u64,
    pub already_present: u64,
    pub failed: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FolderNode {
    pub path: String,
    pub files: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntryView {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub name: String,
    pub folder: String,
    pub taken: String,
    pub provider: String,
    pub provider_info: Option<String>,
    pub size: u64,
    pub destination_exists: bool,
    pub outcome: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkippedView {
    pub source: PathBuf,
    pub reason: String,
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Settings {
    settings::load(&app)
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    settings::save(&app, &settings)
}

#[tauri::command]
pub fn check_folder_pattern(pattern: String) -> Result<(), String> {
    validate_folder_pattern(&pattern).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cancel_job(state: State<'_, AppState>) {
    state.cancel.store(true, Ordering::Relaxed);
}

#[tauri::command]
pub fn start_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ScanRequest,
) -> Result<String, String> {
    validate_folder_pattern(&request.folder_pattern).map_err(|e| e.to_string())?;
    if request.sources.is_empty() {
        return Err("add at least one source folder".into());
    }
    if request.providers.is_empty() {
        return Err("enable at least one date source".into());
    }

    let plan_path = settings::plan_directory(&app)
        .ok_or("no data directory available")?
        .join(default_plan_name(&request.sources, &request.destination));

    state.begin("A scan")?;

    let options = ScanOptions {
        sources: request.sources,
        destination: request.destination,
        folder_pattern: request.folder_pattern,
        detect: DetectOptions {
            providers: request.providers,
            strategy: request.strategy,
        },
        follow_symlinks: request.follow_symlinks,
    };

    let handle = app.clone();
    let target = plan_path.clone();
    std::thread::spawn(move || {
        let state = handle.state::<AppState>();
        let throttle = Mutex::new(Instant::now() - PROGRESS_INTERVAL);
        let result = eonsort_core::scan(
            &target,
            &options,
            &state.cancel,
            &|progress: ScanProgress| {
                emit_throttled(&handle, "scan:progress", &progress, &throttle);
            },
        );
        state.finish();

        match result {
            Ok(plan) => {
                let summary = adopt(&handle, target, plan);
                let _ = handle.emit("scan:done", summary);
            }
            Err(err) => {
                let _ = handle.emit("scan:error", err.to_string());
            }
        }
    });

    Ok(plan_path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn start_copy(
    app: AppHandle,
    state: State<'_, AppState>,
    jobs: usize,
    preserve_times: bool,
) -> Result<(), String> {
    let plan_path = state
        .session
        .lock()
        .unwrap()
        .plan_path
        .clone()
        .ok_or("run a scan first")?;

    state.begin("A copy")?;

    let options = CopyOptions {
        concurrency: jobs.max(1),
        preserve_times,
    };
    let handle = app.clone();
    std::thread::spawn(move || {
        let state = handle.state::<AppState>();
        let throttle = Mutex::new(Instant::now() - PROGRESS_INTERVAL);
        let result = copy::execute(
            &plan_path,
            &options,
            &state.cancel,
            &|progress: CopyProgress| {
                emit_throttled(&handle, "copy:progress", &progress, &throttle);
            },
        );
        state.finish();
        refresh_journal(&handle);

        match result {
            Ok(report) => {
                let _ = handle.emit("copy:done", CopyDone { report });
            }
            Err(err) => {
                let _ = handle.emit("copy:error", err.to_string());
            }
        }
    });

    Ok(())
}

#[derive(Clone, Serialize)]
struct CopyDone {
    report: CopyReport,
}

#[tauri::command]
pub fn start_verify(
    app: AppHandle,
    state: State<'_, AppState>,
    compare_hashes: bool,
) -> Result<(), String> {
    let plan_path = state
        .session
        .lock()
        .unwrap()
        .plan_path
        .clone()
        .ok_or("run a scan first")?;

    state.begin("A check")?;

    let handle = app.clone();
    std::thread::spawn(move || {
        let state = handle.state::<AppState>();
        let throttle = Mutex::new(Instant::now() - PROGRESS_INTERVAL);
        let result = eonsort_core::verify(
            &plan_path,
            &VerifyOptions { compare_hashes },
            &state.cancel,
            &|progress: VerifyProgress| {
                emit_throttled(&handle, "verify:progress", &progress, &throttle);
            },
        );
        state.finish();

        match result {
            Ok(report) => {
                let _ = handle.emit("verify:done", VerifyDone { report });
            }
            Err(err) => {
                let _ = handle.emit("verify:error", err.to_string());
            }
        }
    });

    Ok(())
}

#[derive(Clone, Serialize)]
struct VerifyDone {
    report: VerifyReport,
}

#[tauri::command]
pub fn open_plan(app: AppHandle, path: PathBuf) -> Result<PlanSummary, String> {
    let plan = read_plan(&path).map_err(|e| e.to_string())?;
    Ok(adopt(&app, path, plan))
}

#[tauri::command]
pub fn list_folders(state: State<'_, AppState>) -> Vec<FolderNode> {
    let session = state.session.lock().unwrap();
    let Some(plan) = &session.plan else {
        return Vec::new();
    };

    let mut folders: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for entry in &plan.entries {
        let slot = folders
            .entry(relative_folder(entry, &plan.header.destination))
            .or_insert((0, 0));
        slot.0 += 1;
        slot.1 += entry.size;
    }

    folders
        .into_iter()
        .map(|(path, (files, bytes))| FolderNode { path, files, bytes })
        .collect()
}

#[tauri::command]
pub fn list_entries(state: State<'_, AppState>, folder: String) -> Vec<EntryView> {
    let session = state.session.lock().unwrap();
    let Some(plan) = &session.plan else {
        return Vec::new();
    };

    let mut views: Vec<EntryView> = plan
        .entries
        .iter()
        .filter(|e| relative_folder(e, &plan.header.destination) == folder)
        .map(|entry| view(entry, &plan.header.destination, &session.journal))
        .collect();
    views.sort_by(|a, b| a.name.cmp(&b.name));
    views
}

#[tauri::command]
pub fn list_skipped(state: State<'_, AppState>) -> Vec<SkippedView> {
    let session = state.session.lock().unwrap();
    session
        .plan
        .as_ref()
        .map(|plan| {
            plan.skipped
                .iter()
                .map(|s| SkippedView {
                    source: s.source.clone(),
                    reason: s.reason.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

#[tauri::command]
pub fn preview_file(path: PathBuf) -> Preview {
    preview(&path)
}

fn view(
    entry: &PlanEntry,
    root: &Path,
    journal: &std::collections::HashMap<PathBuf, Outcome>,
) -> EntryView {
    EntryView {
        name: entry
            .destination
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        folder: relative_folder(entry, root),
        taken: entry.taken.format("%Y-%m-%d %H:%M:%S").to_string(),
        provider: entry.provider.label().to_string(),
        provider_info: entry.provider_info.clone(),
        size: entry.size,
        destination_exists: entry.destination.exists(),
        outcome: journal.get(&entry.source).map(outcome_label),
        source: entry.source.clone(),
        destination: entry.destination.clone(),
    }
}

fn outcome_label(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Copied { .. } => "copied",
        Outcome::Duplicate { .. } => "duplicate",
        Outcome::AlreadyPresent { .. } => "already present",
        Outcome::Failed { .. } => "failed",
    }
    .to_string()
}

fn relative_folder(entry: &PlanEntry, root: &Path) -> String {
    let folder = entry.destination.parent().unwrap_or(root);
    folder
        .strip_prefix(root)
        .unwrap_or(folder)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn adopt(app: &AppHandle, plan_path: PathBuf, plan: Plan) -> PlanSummary {
    let journal = copy::read_journal(&copy::journal_path(&plan_path)).unwrap_or_default();
    let summary = summarise(app, &plan_path, &plan, &journal);

    let state = app.state::<AppState>();
    let mut session = state.session.lock().unwrap();
    session.plan_path = Some(plan_path);
    session.plan = Some(plan);
    session.journal = journal;

    summary
}

fn refresh_journal(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut session = state.session.lock().unwrap();
    if let Some(path) = &session.plan_path {
        session.journal = copy::read_journal(&copy::journal_path(path)).unwrap_or_default();
    }
}

fn summarise(
    app: &AppHandle,
    plan_path: &Path,
    plan: &Plan,
    journal: &std::collections::HashMap<PathBuf, Outcome>,
) -> PlanSummary {
    let mut settings = settings::load(app);
    settings.last_plan = Some(plan_path.to_path_buf());
    let _ = settings::save(app, &settings);

    let folders: std::collections::HashSet<String> = plan
        .entries
        .iter()
        .map(|e| relative_folder(e, &plan.header.destination))
        .collect();

    let count = |pred: fn(&Outcome) -> bool| journal.values().filter(|o| pred(o)).count() as u64;

    PlanSummary {
        plan_path: plan_path.to_string_lossy().into_owned(),
        sources: plan.header.sources.clone(),
        destination: plan.header.destination.clone(),
        folder_pattern: plan.header.folder_pattern.clone(),
        files: plan.entries.len() as u64,
        bytes: plan.total_bytes(),
        skipped: plan.skipped.len() as u64,
        folders: folders.len() as u64,
        copied: count(|o| matches!(o, Outcome::Copied { .. })),
        duplicates: count(|o| matches!(o, Outcome::Duplicate { .. })),
        already_present: count(|o| matches!(o, Outcome::AlreadyPresent { .. })),
        failed: count(|o| matches!(o, Outcome::Failed { .. })),
    }
}

fn emit_throttled<T: Serialize + Clone>(
    app: &AppHandle,
    event: &str,
    payload: &T,
    throttle: &Mutex<Instant>,
) {
    let mut last = throttle.lock().unwrap();
    if last.elapsed() < PROGRESS_INTERVAL {
        return;
    }
    *last = Instant::now();
    drop(last);
    let _ = app.emit(event, payload.clone());
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use eonsort_core::providers::Provider;

    fn entry(destination: &str) -> PlanEntry {
        PlanEntry {
            source: PathBuf::from("/src/a.jpg"),
            destination: PathBuf::from(destination),
            taken: NaiveDate::from_ymd_opt(2023, 5, 6)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            provider: Provider::Filename,
            provider_info: None,
            size: 1,
        }
    }

    #[test]
    fn folder_is_relative_to_the_destination_root() {
        let root = Path::new("/out");
        assert_eq!(
            relative_folder(&entry("/out/2023/05/a.jpg"), root),
            "2023/05"
        );
        assert_eq!(relative_folder(&entry("/out/a.jpg"), root), "");
    }

    #[test]
    fn folder_falls_back_to_the_full_path_when_outside_the_root() {
        let root = Path::new("/other");
        let folder = relative_folder(&entry("/out/2023/05/a.jpg"), root);
        assert!(
            folder.ends_with("out/2023/05"),
            "unexpected folder: {folder}"
        );
    }
}
