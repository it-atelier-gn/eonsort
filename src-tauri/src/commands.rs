use crate::preview::{preview, thumbnail, Preview, Thumbnail};
use crate::settings::{self, Settings};
use crate::state::AppState;
use chrono::{Local, NaiveDate, NaiveDateTime};
use eonsort_core::copy::{self, CopyOptions, CopyProgress, CopyReport, Outcome};
use eonsort_core::faces;
use eonsort_core::geocode;
use eonsort_core::model::PlanEntry;
use eonsort_core::overrides::{
    self, DateOverride, OverrideOrigin, Overrides, RotationOverride, Rotations,
};
use eonsort_core::providers::{DetectOptions, Provider, Strategy, Weights};
use eonsort_core::quality;
use eonsort_core::rotate::{self, Transform};
use eonsort_core::scan::{ScanOptions, ScanProgress};
use eonsort_core::similar;
use eonsort_core::suspect::{self, EntryFacts, Flag};
use eonsort_core::tagging;
use eonsort_core::tags;
use eonsort_core::upright;
use eonsort_core::verify::{VerifyOptions, VerifyProgress, VerifyReport};
use eonsort_core::{default_plan_name, read_plan, validate_folder_pattern, Plan};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

const PROGRESS_INTERVAL: Duration = Duration::from_millis(60);
const SAME_PERSON: f32 = 0.363;
const TAG_BATCH: usize = 40;
const HANDOVER: Duration = Duration::from_secs(90);
const SEARCH_LIMIT: usize = 500;
const FACE_FRAME: &str = "face_frame";
const UPRIGHT_FRAME: &str = "upright";

#[derive(Debug, Clone, Deserialize)]
pub struct ScanRequest {
    pub sources: Vec<PathBuf>,
    #[serde(default)]
    pub destination: Option<PathBuf>,
    pub folder_pattern: String,
    pub providers: Vec<Provider>,
    pub strategy: Strategy,
    #[serde(default)]
    pub weights: Weights,
    pub follow_symlinks: bool,
    #[serde(default)]
    pub auto_rotate: bool,
    #[serde(default)]
    pub pair_companions: bool,
    #[serde(default)]
    pub upright: bool,
    #[serde(default)]
    pub name_places: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanSummary {
    pub plan_path: String,
    pub sources: Vec<PathBuf>,
    pub destination: Option<PathBuf>,
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
pub struct CandidateView {
    pub provider: String,
    pub provider_info: Option<String>,
    pub taken: String,
    pub taken_epoch: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FlagView {
    pub kind: String,
    pub description: String,
    pub hard: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntryView {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub name: String,
    pub folder: String,
    pub taken: String,
    pub taken_epoch: i64,
    pub provider: String,
    pub provider_info: Option<String>,
    pub size: u64,
    pub destination_exists: bool,
    pub outcome: Option<String>,
    pub candidates: Vec<CandidateView>,
    pub flags: Vec<FlagView>,
    pub confidence: String,
    pub override_origin: Option<String>,
    pub orientation: u16,
    pub rotate: String,
    pub rotate_by_hand: bool,
    pub rotate_lossless: bool,
    pub reencode: bool,
    pub subject: Option<String>,
    pub tags: Vec<String>,
    pub caption: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkippedView {
    pub source: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SuspectGroup {
    pub key: String,
    pub kind: String,
    pub reason: String,
    pub folder: String,
    pub files: u64,
    pub earliest: String,
    pub latest: String,
    pub sources: Vec<PathBuf>,
    pub destination_folders: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DateChoice {
    Candidate { provider: Provider },
    Manual { taken: String },
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Settings {
    settings::load(&app)
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    settings::save(&app, &settings)?;
    let saved = settings::load(&app);
    app.emit("settings:changed", saved)
        .map_err(|e| e.to_string())
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
        .join(default_plan_name(
            &request.sources,
            request.destination.as_deref(),
        ));

    state.begin("A scan")?;

    let options = ScanOptions {
        sources: request.sources,
        destination: request.destination,
        folder_pattern: request.folder_pattern,
        gazetteer: request
            .name_places
            .then(|| models_directory(&app).map(|dir| geocode::directory(&dir)))
            .transpose()?,
        name_pattern: eonsort_core::naming::DEFAULT_NAME_PATTERN.to_string(),
        detect: DetectOptions {
            providers: request.providers,
            strategy: request.strategy,
            weights: request.weights,
        },
        follow_symlinks: request.follow_symlinks,
        auto_rotate: request.auto_rotate,
        pair_companions: request.pair_companions,
        upright_model_dir: request
            .upright
            .then(|| models_directory(&app))
            .transpose()?,
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
            Ok(plan) => match adopt(&handle, target, plan) {
                Ok(summary) => {
                    let _ = handle.emit("scan:done", summary);
                }
                Err(err) => {
                    let _ = handle.emit("scan:error", err);
                }
            },
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
    preserve_times: bool,
    stamp_date: bool,
    write_sidecars: bool,
) -> Result<(), String> {
    let plan_path = {
        let session = state.session.lock().unwrap();
        if session
            .plan
            .as_ref()
            .is_some_and(|p| p.header.destination.is_none())
        {
            return Err("choose a destination folder first".into());
        }
        session.plan_path.clone().ok_or("run a scan first")?
    };

    state.begin("A copy")?;

    let options = CopyOptions {
        concurrency: None,
        preserve_times,
        stamp_date,
        write_sidecars,
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
    let plan_path = {
        let session = state.session.lock().unwrap();
        if session
            .plan
            .as_ref()
            .is_some_and(|p| p.header.destination.is_none())
        {
            return Err("choose a destination folder first".into());
        }
        session.plan_path.clone().ok_or("run a scan first")?
    };

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
    adopt(&app, path, plan)
}

#[tauri::command]
pub fn set_destination(
    app: AppHandle,
    state: State<'_, AppState>,
    destination: Option<PathBuf>,
) -> Result<PlanSummary, String> {
    let plan_path = state
        .session
        .lock()
        .unwrap()
        .plan_path
        .clone()
        .ok_or("no plan is open")?;

    let plan =
        eonsort_core::retarget(&plan_path, destination.as_deref()).map_err(|e| e.to_string())?;
    adopt(&app, plan_path, plan)
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
            .entry(relative_folder(entry, plan.header.root()))
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
pub fn list_all_entries(state: State<'_, AppState>) -> Vec<EntryView> {
    let session = state.session.lock().unwrap();
    let Some(plan) = &session.plan else {
        return Vec::new();
    };
    plan.entries
        .iter()
        .map(|entry| {
            view(
                entry,
                plan.header.root(),
                &session.journal,
                &session.overrides,
                &session.rotations,
            )
        })
        .collect()
}

#[tauri::command]
pub fn list_suspects(state: State<'_, AppState>) -> Vec<SuspectGroup> {
    let session = state.session.lock().unwrap();
    let Some(plan) = &session.plan else {
        return Vec::new();
    };

    let mut groups: BTreeMap<(String, String), (String, Vec<&PlanEntry>)> = BTreeMap::new();
    for entry in &plan.entries {
        let Some(flag) = entry
            .flags
            .iter()
            .find(|f| f.severity() == suspect::Severity::Hard)
        else {
            continue;
        };
        let folder = entry
            .source
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let kind = flag_kind(flag).to_string();
        groups
            .entry((folder, kind))
            .or_insert_with(|| (flag.describe(), Vec::new()))
            .1
            .push(entry);
    }

    let root = plan.header.root();
    let mut out: Vec<SuspectGroup> = groups
        .into_iter()
        .map(|((folder, kind), (reason, entries))| {
            let earliest = entries.iter().map(|e| e.taken).min().unwrap();
            let latest = entries.iter().map(|e| e.taken).max().unwrap();
            let mut destination_folders: Vec<String> = entries
                .iter()
                .map(|e| relative_folder(e, root))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            destination_folders.truncate(24);
            SuspectGroup {
                key: format!("{folder}::{kind}"),
                kind,
                reason,
                folder,
                files: entries.len() as u64,
                earliest: format_time(earliest),
                latest: format_time(latest),
                sources: entries.iter().map(|e| e.source.clone()).collect(),
                destination_folders,
            }
        })
        .collect();
    out.sort_by(|a, b| b.files.cmp(&a.files).then(a.key.cmp(&b.key)));
    out
}

#[tauri::command]
pub fn set_date_override(
    app: AppHandle,
    state: State<'_, AppState>,
    source: PathBuf,
    choice: DateChoice,
) -> Result<EntryView, String> {
    let taken = {
        let session = state.session.lock().unwrap();
        let entry = find_entry(&session.plan, &source)?;
        refuse_if_copied(&session.journal, &source)?;
        match &choice {
            DateChoice::Manual { taken } => parse_manual(taken)?,
            DateChoice::Candidate { provider } => {
                entry
                    .candidate(*provider)
                    .ok_or_else(|| {
                        format!("{} has no date from {}", display(&source), provider.label())
                    })?
                    .taken
            }
        }
    };

    let origin = match choice {
        DateChoice::Manual { .. } => OverrideOrigin::Manual,
        DateChoice::Candidate { provider } => OverrideOrigin::Candidate { provider },
    };

    let mut applied = state.session.lock().unwrap().overrides.clone();
    applied.set(source.clone(), stamp(taken, origin));
    store(&app, &applied)?;
    reload(&app)?;
    entry_view(&state, &source)
}

#[tauri::command]
pub fn clear_date_override(
    app: AppHandle,
    state: State<'_, AppState>,
    source: PathBuf,
) -> Result<EntryView, String> {
    let mut applied = state.session.lock().unwrap().overrides.clone();
    if !applied.clear(&source) {
        return entry_view(&state, &source);
    }
    store(&app, &applied)?;
    reload(&app)?;
    entry_view(&state, &source)
}

#[tauri::command]
pub fn shift_dates(
    app: AppHandle,
    state: State<'_, AppState>,
    sources: Vec<PathBuf>,
    seconds: i64,
) -> Result<u64, String> {
    let shift = chrono::Duration::try_seconds(seconds).ok_or("that shift is too large")?;

    let mut applied = state.session.lock().unwrap().overrides.clone();
    let mut changed = 0u64;
    {
        let session = state.session.lock().unwrap();
        for source in &sources {
            let entry = find_entry(&session.plan, source)?;
            refuse_if_copied(&session.journal, source)?;
            let taken = entry
                .taken
                .checked_add_signed(shift)
                .ok_or_else(|| format!("{} would move outside the calendar", display(source)))?;
            applied.set(
                source.clone(),
                stamp(taken, OverrideOrigin::Shift { seconds }),
            );
            changed += 1;
        }
    }

    store(&app, &applied)?;
    reload(&app)?;
    Ok(changed)
}

#[tauri::command]
pub fn reprovider_cluster(
    app: AppHandle,
    state: State<'_, AppState>,
    sources: Vec<PathBuf>,
    provider: Provider,
) -> Result<u64, String> {
    let mut applied = state.session.lock().unwrap().overrides.clone();
    let mut changed = 0u64;
    {
        let session = state.session.lock().unwrap();
        for source in &sources {
            let entry = find_entry(&session.plan, source)?;
            refuse_if_copied(&session.journal, source)?;
            let Some(candidate) = entry.candidate(provider) else {
                continue;
            };
            applied.set(
                source.clone(),
                stamp(candidate.taken, OverrideOrigin::Candidate { provider }),
            );
            changed += 1;
        }
    }

    if changed == 0 {
        return Err(format!(
            "none of those files have a date from {}",
            provider.label()
        ));
    }

    store(&app, &applied)?;
    reload(&app)?;
    Ok(changed)
}

#[derive(Debug, Clone, Serialize)]
pub struct RotationProbe {
    pub lossless: bool,
    pub reason: Option<String>,
}

fn transform_from_label(label: &str) -> Option<Transform> {
    Some(match label {
        "none" => Transform::None,
        "rotate90" => Transform::Rotate90,
        "rotate180" => Transform::Rotate180,
        "rotate270" => Transform::Rotate270,
        "flip_h" => Transform::FlipH,
        "flip_v" => Transform::FlipV,
        "transpose" => Transform::Transpose,
        "transverse" => Transform::Transverse,
        _ => return None,
    })
}

fn rotate_label(transform: Transform) -> &'static str {
    match transform {
        Transform::None => "none",
        Transform::Rotate90 => "rotate90",
        Transform::Rotate180 => "rotate180",
        Transform::Rotate270 => "rotate270",
        Transform::FlipH => "flip_h",
        Transform::FlipV => "flip_v",
        Transform::Transpose => "transpose",
        Transform::Transverse => "transverse",
    }
}

fn turn_stamp(transform: Transform, reencode: bool) -> RotationOverride {
    RotationOverride {
        transform,
        reencode,
        at: Local::now().naive_local(),
    }
}

#[tauri::command]
pub fn turn_rotation(
    app: AppHandle,
    state: State<'_, AppState>,
    source: PathBuf,
    quarter_turns: i8,
) -> Result<EntryView, String> {
    let (transform, reencode) = {
        let session = state.session.lock().unwrap();
        let entry = find_entry(&session.plan, &source)?;
        refuse_if_copied(&session.journal, &source)?;
        (entry.rotate.turn(quarter_turns), entry.reencode)
    };

    let mut turns = state.session.lock().unwrap().rotations.clone();
    turns.set(source.clone(), turn_stamp(transform, reencode));
    store_rotations(&app, &turns)?;
    reload(&app)?;
    entry_view(&state, &source)
}

#[tauri::command]
pub fn set_rotation(
    app: AppHandle,
    state: State<'_, AppState>,
    source: PathBuf,
    reencode: bool,
) -> Result<EntryView, String> {
    let transform = {
        let session = state.session.lock().unwrap();
        let entry = find_entry(&session.plan, &source)?;
        refuse_if_copied(&session.journal, &source)?;
        entry.rotate
    };

    let mut turns = state.session.lock().unwrap().rotations.clone();
    turns.set(source.clone(), turn_stamp(transform, reencode));
    store_rotations(&app, &turns)?;
    reload(&app)?;
    entry_view(&state, &source)
}

#[tauri::command]
pub fn clear_rotation(
    app: AppHandle,
    state: State<'_, AppState>,
    source: PathBuf,
) -> Result<EntryView, String> {
    let mut turns = state.session.lock().unwrap().rotations.clone();
    if !turns.clear(&source) {
        return entry_view(&state, &source);
    }
    store_rotations(&app, &turns)?;
    reload(&app)?;
    entry_view(&state, &source)
}

#[tauri::command]
pub fn rotate_marked(
    app: AppHandle,
    state: State<'_, AppState>,
    sources: Vec<PathBuf>,
    quarter_turns: i8,
) -> Result<u64, String> {
    let mut turns = state.session.lock().unwrap().rotations.clone();
    let mut changed = 0u64;
    {
        let session = state.session.lock().unwrap();
        for source in &sources {
            let entry = find_entry(&session.plan, source)?;
            refuse_if_copied(&session.journal, source)?;
            turns.set(
                source.clone(),
                turn_stamp(entry.rotate.turn(quarter_turns), entry.reencode),
            );
            changed += 1;
        }
    }

    store_rotations(&app, &turns)?;
    reload(&app)?;
    Ok(changed)
}

#[tauri::command]
pub async fn probe_rotation(source: PathBuf) -> RotationProbe {
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = match std::fs::read(&source) {
            Ok(bytes) => bytes,
            Err(e) => {
                return RotationProbe {
                    lossless: false,
                    reason: Some(e.to_string()),
                }
            }
        };
        match rotate::losslessly(&source, &bytes, Transform::Rotate90) {
            Ok(_) => RotationProbe {
                lossless: true,
                reason: None,
            },
            Err(e) => RotationProbe {
                lossless: false,
                reason: Some(e.to_string()),
            },
        }
    })
    .await
    .unwrap_or(RotationProbe {
        lossless: false,
        reason: Some("could not look at that file".into()),
    })
}

fn store_rotations(app: &AppHandle, turns: &Rotations) -> Result<(), String> {
    let plan_path = {
        let state = app.state::<AppState>();
        let session = state.session.lock().unwrap();
        session.plan_path.clone()
    };
    let plan_path = plan_path.ok_or("no plan is open")?;
    overrides::write_rotations(&overrides::rotations_path(&plan_path), turns)
        .map_err(|e| e.to_string())
}

fn stamp(taken: NaiveDateTime, origin: OverrideOrigin) -> DateOverride {
    DateOverride {
        taken,
        origin,
        at: Local::now().naive_local(),
    }
}

fn display(source: &Path) -> String {
    source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| source.to_string_lossy().into_owned())
}

fn find_entry<'a>(plan: &'a Option<Plan>, source: &Path) -> Result<&'a PlanEntry, String> {
    plan.as_ref()
        .ok_or("no plan is open")?
        .entries
        .iter()
        .find(|e| e.source == source)
        .ok_or_else(|| format!("{} is not part of this plan", display(source)))
}

fn refuse_if_copied(
    journal: &std::collections::HashMap<PathBuf, Outcome>,
    source: &Path,
) -> Result<(), String> {
    match journal.get(source) {
        Some(Outcome::Failed { .. }) | None => Ok(()),
        Some(_) => Err(format!(
            "{} has already been copied, so re-dating it would leave the copy behind",
            display(source)
        )),
    }
}

fn entry_view(state: &State<'_, AppState>, source: &Path) -> Result<EntryView, String> {
    let session = state.session.lock().unwrap();
    let plan = session.plan.as_ref().ok_or("no plan is open")?;
    let entry = plan
        .entries
        .iter()
        .find(|e| e.source == source)
        .ok_or_else(|| format!("{} is not part of this plan", display(source)))?;
    Ok(view(
        entry,
        plan.header.root(),
        &session.journal,
        &session.overrides,
        &session.rotations,
    ))
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

#[tauri::command]
pub async fn thumbnail_for(path: PathBuf, edge: u32, rotate: Option<String>) -> Thumbnail {
    let transform = rotate.as_deref().and_then(transform_from_label);
    tauri::async_runtime::spawn_blocking(move || thumbnail(&path, edge, transform))
        .await
        .unwrap_or(Thumbnail::None)
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateReport {
    pub groups: Vec<DuplicateView>,
    pub files: usize,
    pub wasted: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateView {
    pub sources: Vec<PathBuf>,
    pub folder: String,
    pub bytes: u64,
    pub wasted: u64,
}

#[tauri::command]
pub fn find_duplicates(state: State<'_, AppState>) -> Result<DuplicateReport, String> {
    let files: Vec<(PathBuf, u64)> = {
        let session = state.session.lock().unwrap();
        let plan = session.plan.as_ref().ok_or("run a scan first")?;
        plan.entries
            .iter()
            .map(|e| (e.source.clone(), e.size))
            .collect()
    };

    let groups =
        eonsort_core::duplicates::exact(&files, &state.cancel).map_err(|e| e.to_string())?;
    let wasted = eonsort_core::duplicates::wasted(&groups);
    let files = groups.iter().map(|group| group.sources.len()).sum();

    Ok(DuplicateReport {
        groups: groups
            .into_iter()
            .map(|group| DuplicateView {
                folder: group
                    .sources
                    .first()
                    .and_then(|s| s.parent())
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                bytes: group.bytes,
                wasted: group.wasted,
                sources: group.sources,
            })
            .collect(),
        files,
        wasted,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct PruneReport {
    pub removed: usize,
    pub freed: u64,
    pub kept: usize,
    pub failures: Vec<SkippedView>,
}

fn keeper_first(sources: &[PathBuf]) -> Vec<PathBuf> {
    let mut ranked = sources.to_vec();
    ranked.sort_by(|a, b| {
        a.components()
            .count()
            .cmp(&b.components().count())
            .then_with(|| a.as_os_str().len().cmp(&b.as_os_str().len()))
            .then_with(|| a.cmp(b))
    });
    ranked
}

#[tauri::command]
pub fn remove_extra_copies(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<PruneReport, String> {
    let files: Vec<(PathBuf, u64)> = {
        let session = state.session.lock().unwrap();
        let plan = session.plan.as_ref().ok_or("run a scan first")?;
        plan.entries
            .iter()
            .map(|e| (e.source.clone(), e.size))
            .collect()
    };

    let groups =
        eonsort_core::duplicates::exact(&files, &state.cancel).map_err(|e| e.to_string())?;

    let mut report = PruneReport {
        removed: 0,
        freed: 0,
        kept: 0,
        failures: Vec::new(),
    };
    let mut gone: Vec<PathBuf> = Vec::new();

    for group in groups {
        if group.sources.len() < 2 {
            continue;
        }
        let ranked = keeper_first(&group.sources);
        report.kept += 1;

        for extra in ranked.iter().skip(1) {
            match trash::delete(extra) {
                Ok(()) => {
                    report.removed += 1;
                    report.freed += group.bytes;
                    gone.push(extra.clone());
                }
                Err(err) => report.failures.push(SkippedView {
                    source: extra.clone(),
                    reason: err.to_string(),
                }),
            }
        }
    }

    if !gone.is_empty() {
        set_excluded(app, state, gone, true)?;
    }
    Ok(report)
}

#[derive(Debug, Clone, Serialize)]
pub struct BurstView {
    pub keeper: PathBuf,
    pub members: Vec<PathBuf>,
    pub folder: String,
    pub taken: String,
    pub extra_bytes: u64,
}

#[tauri::command]
pub fn find_bursts(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<BurstView>, String> {
    let (facts, sizes) = {
        let session = state.session.lock().unwrap();
        let plan = session.plan.as_ref().ok_or("run a scan first")?;
        let sizes: std::collections::HashMap<PathBuf, u64> = plan
            .entries
            .iter()
            .map(|e| (e.source.clone(), e.size))
            .collect();
        let facts: Vec<(PathBuf, NaiveDateTime, u64)> = plan
            .entries
            .iter()
            .filter(|e| similar::hashable(&e.source))
            .map(|e| (e.source.clone(), e.taken, e.size))
            .collect();
        (facts, sizes)
    };

    let ratings = ratings_for(&app, &facts);
    let hashed = similar::fingerprint_all_rated(&facts, &|path| ratings.get(path).copied());

    Ok(similar::group_bursts(&hashed)
        .into_iter()
        .map(|burst| {
            let extra_bytes = burst
                .others()
                .filter_map(|s| sizes.get(s))
                .copied()
                .sum::<u64>();
            let folder = burst
                .keeper
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let taken = facts
                .iter()
                .find(|(s, _, _)| *s == burst.keeper)
                .map(|(_, t, _)| format_time(*t))
                .unwrap_or_default();
            BurstView {
                keeper: burst.keeper,
                members: burst.members,
                folder,
                taken,
                extra_bytes,
            }
        })
        .collect())
}

#[derive(Debug, Clone, Serialize)]
pub struct LookalikeView {
    pub keeper: PathBuf,
    pub members: Vec<PathBuf>,
    pub folders: usize,
    pub extra_bytes: u64,
}

#[tauri::command]
pub fn find_lookalikes(state: State<'_, AppState>) -> Result<Vec<LookalikeView>, String> {
    let (facts, sizes) = {
        let session = state.session.lock().unwrap();
        let plan = session.plan.as_ref().ok_or("run a scan first")?;
        let sizes: std::collections::HashMap<PathBuf, u64> = plan
            .entries
            .iter()
            .map(|e| (e.source.clone(), e.size))
            .collect();
        let facts: Vec<(PathBuf, NaiveDateTime, u64)> = plan
            .entries
            .iter()
            .filter(|e| similar::hashable(&e.source))
            .map(|e| (e.source.clone(), e.taken, e.size))
            .collect();
        (facts, sizes)
    };

    let hashed = similar::fingerprint_all(&facts);

    Ok(
        similar::group_lookalikes(&hashed, &|path| eonsort_core::tags::digest(path))
            .into_iter()
            .map(|found| {
                let extra_bytes = found
                    .others()
                    .filter_map(|s| sizes.get(s))
                    .copied()
                    .sum::<u64>();
                LookalikeView {
                    keeper: found.keeper,
                    members: found.members,
                    folders: found.folders,
                    extra_bytes,
                }
            })
            .collect(),
    )
}

#[tauri::command]
pub fn set_excluded(
    app: AppHandle,
    state: State<'_, AppState>,
    sources: Vec<PathBuf>,
    excluded: bool,
) -> Result<u64, String> {
    let plan_path = {
        let session = state.session.lock().unwrap();
        session.plan_path.clone()
    };
    let plan_path = plan_path.ok_or("no plan is open")?;

    let path = overrides::excluded_path(&plan_path);
    let mut current = overrides::read_excluded(&path).map_err(|e| e.to_string())?;

    let mut changed = 0u64;
    for source in sources {
        let moved = if excluded {
            current.insert(source)
        } else {
            current.remove(&source)
        };
        if moved {
            changed += 1;
        }
    }

    overrides::write_excluded(&path, &current).map_err(|e| e.to_string())?;
    reload(&app)?;
    Ok(changed)
}

fn models_directory(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("models");
    Ok(dir)
}

#[derive(Debug, Clone, Serialize)]
pub struct UprightModelStatus {
    pub present: bool,
    pub bytes: u64,
    pub total: u64,
    pub built_in: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct UprightGuessView {
    pub transform: String,
    pub confidence: f32,
    pub reason: String,
}

#[tauri::command]
pub fn upright_model_status(app: AppHandle) -> Result<UprightModelStatus, String> {
    let dir = models_directory(&app)?;
    Ok(UprightModelStatus {
        present: upright::installed(&dir),
        bytes: upright::present_bytes(&dir),
        total: upright::total_bytes(),
        built_in: cfg!(feature = "upright"),
    })
}

#[tauri::command]
pub fn install_upright_model(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if !cfg!(feature = "upright") {
        return Err("this build was made without the upright model".into());
    }

    {
        let mut fetching = state.fetching_upright.lock().unwrap();
        if *fetching {
            return Err("the upright model is already downloading".into());
        }
        *fetching = true;
    }

    state.upright_cancel.store(false, Ordering::Relaxed);
    let dir = models_directory(&app)?;
    let handle = app.clone();

    std::thread::spawn(move || {
        let state = handle.state::<AppState>();
        let throttle = Mutex::new(Instant::now() - PROGRESS_INTERVAL);

        let result = upright::download(
            &dir,
            &state.upright_cancel,
            &|progress: upright::UprightProgress| {
                emit_throttled(&handle, "upright:progress", &progress, &throttle);
            },
        );
        *state.fetching_upright.lock().unwrap() = false;

        match result {
            Ok(()) => {
                let _ = handle.emit("upright:done", upright::present_bytes(&dir));
            }
            Err(err) => {
                let _ = handle.emit("upright:error", err.to_string());
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub fn cancel_upright_install(state: State<'_, AppState>) {
    state.upright_cancel.store(true, Ordering::Relaxed);
}

#[tauri::command]
pub async fn guess_upright(app: AppHandle, source: PathBuf) -> Result<UprightGuessView, String> {
    let dir = models_directory(&app)?;

    tauri::async_runtime::spawn_blocking(move || {
        upright::Detector::load(&dir).and_then(|detector| detector.guess(&source))
    })
    .await
    .map_err(|e| e.to_string())?
    .map(|guess| UprightGuessView {
        transform: rotate_label(guess.transform).to_string(),
        confidence: guess.confidence,
        reason: guess.reason,
    })
    .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct TagModelStatus {
    pub present: bool,
    pub bytes: u64,
    pub total: u64,
    pub built_in: bool,
    pub credit: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TagProgressView {
    pub done: usize,
    pub total: usize,
    pub current: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TagHit {
    pub source: String,
    pub score: f32,
}

#[tauri::command]
pub fn quality_model_status(app: AppHandle) -> Result<TagModelStatus, String> {
    let dir = models_directory(&app)?;
    Ok(TagModelStatus {
        present: quality::installed(&dir),
        bytes: quality::present_bytes(&dir),
        total: quality::total_bytes(),
        built_in: cfg!(feature = "quality"),
        credit: quality::CREDIT.to_string(),
    })
}

#[tauri::command]
pub fn install_quality_model(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if !cfg!(feature = "quality") {
        return Err("this build was made without the quality model".into());
    }

    {
        let mut fetching = state.fetching_quality.lock().unwrap();
        if *fetching {
            return Err("the quality model is already downloading".into());
        }
        *fetching = true;
    }

    state.quality_cancel.store(false, Ordering::Relaxed);
    let dir = models_directory(&app)?;
    let handle = app.clone();

    std::thread::spawn(move || {
        let state = handle.state::<AppState>();
        let throttle = Mutex::new(Instant::now() - PROGRESS_INTERVAL);

        let result = quality::download(
            &dir,
            &state.quality_cancel,
            &|progress: quality::QualityProgress| {
                emit_throttled(&handle, "quality:fetch", &progress, &throttle);
            },
        );
        *state.fetching_quality.lock().unwrap() = false;

        match result {
            Ok(()) => {
                let _ = handle.emit("quality:fetched", quality::present_bytes(&dir));
            }
            Err(err) => {
                let _ = handle.emit("quality:error", err.to_string());
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub fn cancel_quality_install(state: State<'_, AppState>) {
    state.quality_cancel.store(true, Ordering::Relaxed);
}

#[derive(Debug, Clone, Serialize)]
pub struct GazetteerStatus {
    pub present: bool,
    pub bytes: u64,
    pub total: u64,
    pub credit: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OffsetView {
    pub reference: String,
    pub seconds: i64,
    pub shape: String,
    pub files: usize,
    pub sources: Vec<PathBuf>,
    pub told: String,
}

#[tauri::command]
pub fn plan_offsets(state: State<'_, AppState>) -> Result<Vec<OffsetView>, String> {
    let session = state.session.lock().unwrap();
    let Some(plan) = session.plan.as_ref() else {
        return Ok(Vec::new());
    };

    Ok(eonsort_core::offset::propose(&plan.entries)
        .into_iter()
        .map(|found| OffsetView {
            reference: found.reference.label().to_string(),
            seconds: found.seconds,
            shape: match found.shape {
                eonsort_core::offset::Shape::Timezone { .. } => "timezone".to_string(),
                eonsort_core::offset::Shape::Drift => "drift".to_string(),
            },
            files: found.files(),
            told: found.describe(),
            sources: found.sources,
        })
        .collect())
}

#[tauri::command]
pub fn gazetteer_status(app: AppHandle) -> Result<GazetteerStatus, String> {
    let dir = models_directory(&app)?;
    Ok(GazetteerStatus {
        present: geocode::installed(&dir),
        bytes: geocode::present_bytes(&dir),
        total: geocode::total_bytes(),
        credit: geocode::CREDIT.to_string(),
    })
}

#[tauri::command]
pub fn install_gazetteer(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    {
        let mut fetching = state.fetching_places.lock().unwrap();
        if *fetching {
            return Err("the place names are already downloading".into());
        }
        *fetching = true;
    }

    state.places_cancel.store(false, Ordering::Relaxed);
    let dir = models_directory(&app)?;
    let handle = app.clone();

    std::thread::spawn(move || {
        let state = handle.state::<AppState>();
        let throttle = Mutex::new(Instant::now() - PROGRESS_INTERVAL);

        let result = geocode::download(
            &dir,
            &state.places_cancel,
            &|progress: geocode::PlaceProgress| {
                emit_throttled(&handle, "places:fetch", &progress, &throttle);
            },
        );
        *state.fetching_places.lock().unwrap() = false;

        match result {
            Ok(()) => {
                let _ = handle.emit("places:fetched", geocode::present_bytes(&dir));
            }
            Err(err) => {
                let _ = handle.emit("places:error", err.to_string());
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub fn cancel_gazetteer_install(state: State<'_, AppState>) {
    state.places_cancel.store(true, Ordering::Relaxed);
}

#[tauri::command]
pub fn remove_gazetteer(app: AppHandle) -> Result<(), String> {
    let dir = models_directory(&app)?;
    geocode::remove(&dir).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn tag_model_status(app: AppHandle) -> Result<TagModelStatus, String> {
    let dir = models_directory(&app)?;
    Ok(TagModelStatus {
        present: tagging::installed(&dir),
        bytes: tagging::present_bytes(&dir),
        total: tagging::total_bytes(),
        built_in: cfg!(feature = "tagging"),
        credit: tagging::CREDIT.to_string(),
    })
}

#[tauri::command]
pub fn install_tag_model(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if !cfg!(feature = "tagging") {
        return Err("this build was made without the tagging model".into());
    }

    {
        let mut fetching = state.fetching_tags.lock().unwrap();
        if *fetching {
            return Err("the tagging model is already downloading".into());
        }
        *fetching = true;
    }

    state.tag_cancel.store(false, Ordering::Relaxed);
    let dir = models_directory(&app)?;
    let handle = app.clone();

    std::thread::spawn(move || {
        let state = handle.state::<AppState>();
        let throttle = Mutex::new(Instant::now() - PROGRESS_INTERVAL);

        let result = tagging::download(
            &dir,
            &state.tag_cancel,
            &|progress: tagging::TagProgress| {
                emit_throttled(&handle, "tags:fetch", &progress, &throttle);
            },
        );
        *state.fetching_tags.lock().unwrap() = false;

        match result {
            Ok(()) => {
                let _ = handle.emit("tags:fetched", tagging::present_bytes(&dir));
            }
            Err(err) => {
                let _ = handle.emit("tags:error", err.to_string());
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub fn cancel_tag_install(state: State<'_, AppState>) {
    state.tag_cancel.store(true, Ordering::Relaxed);
}

fn take_the_look(state: &State<'_, AppState>) -> bool {
    let waited = Instant::now();
    loop {
        {
            let mut running = state.tagging.lock().unwrap();
            if !*running {
                *running = true;
                return true;
            }
        }
        if waited.elapsed() > HANDOVER {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[tauri::command]
pub fn cancel_tagging(state: State<'_, AppState>) {
    state.tag_cancel.store(true, Ordering::Relaxed);
}

fn planned(app: &AppHandle) -> Option<PathBuf> {
    let state = app.state::<AppState>();
    let session = state.session.lock().unwrap();
    session.plan_path.as_deref().map(|plan| plan.to_path_buf())
}

fn pictures_of(state: &State<'_, AppState>) -> Result<Vec<PathBuf>, String> {
    let session = state.session.lock().unwrap();
    let Some(plan) = &session.plan else {
        return Err("run a scan first".into());
    };
    Ok(plan
        .entries
        .iter()
        .filter(|entry| is_picture(&entry.source))
        .map(|entry| entry.source.clone())
        .collect())
}

fn announce(app: &AppHandle, total: usize, note: &str) {
    let _ = app.emit(
        "tags:progress",
        &TagProgressView {
            done: 0,
            total,
            current: None,
            note: Some(note.to_string()),
        },
    );
}

fn ratings_for(
    app: &AppHandle,
    facts: &[(PathBuf, NaiveDateTime, u64)],
) -> std::collections::HashMap<PathBuf, f32> {
    let Ok(store) = library(app) else {
        return std::collections::HashMap::new();
    };
    facts
        .iter()
        .filter_map(|(source, _, _)| {
            let seen = store.at(source).ok().flatten()?;
            seen.quality
                .filter(|score| score.is_finite())
                .map(|score| (source.clone(), score))
        })
        .collect()
}

fn library(app: &AppHandle) -> Result<tags::Store, String> {
    let dir = settings::plan_directory(app).ok_or("no data directory available")?;
    let store = tags::Store::library(&dir).map_err(|e| e.to_string())?;
    if let Some(plan) = planned(app) {
        store.inherit(&plan).map_err(|e| e.to_string())?;
    }
    Ok(store)
}

#[derive(Debug, Clone, Serialize)]
pub struct SightingView {
    pub tags: Vec<String>,
    pub quality: Option<f32>,
}

#[tauri::command]
pub fn list_tags(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<HashMap<String, SightingView>, String> {
    if planned(&app).is_none() {
        return Ok(HashMap::new());
    }
    let pictures = pictures_of(&state)?;
    let store = library(&app)?;
    Ok(store
        .listing(&pictures)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(source, tags, quality)| {
            (
                source.to_string_lossy().into_owned(),
                SightingView {
                    tags: tags
                        .into_iter()
                        .filter(|tag| !quality::was_a_rating(tag))
                        .collect(),
                    quality: quality.filter(|score| score.is_finite()),
                },
            )
        })
        .collect())
}

#[tauri::command]
pub fn start_tagging(
    app: AppHandle,
    state: State<'_, AppState>,
    afresh: bool,
) -> Result<usize, String> {
    if !cfg!(feature = "tagging") {
        return Err("this build was made without the tagging model".into());
    }

    let dir = models_directory(&app)?;
    if !tagging::installed(&dir) {
        return Err("the tagging model is not downloaded yet".into());
    }

    let rating = settings::load(&app).rate_quality && quality::installed(&dir);

    if planned(&app).is_none() {
        return Err("run a scan first".into());
    }
    let pictures = pictures_of(&state)?;

    state.tag_cancel.store(true, Ordering::Relaxed);
    let handle = app.clone();
    let wanted = pictures.len();
    announce(&app, wanted, "Getting ready to tag");

    std::thread::spawn(move || {
        let state = handle.state::<AppState>();
        announce(&handle, wanted, "Waiting for the last pass to stop");
        if !take_the_look(&state) {
            let _ = handle.emit("tags:error", "the last tagging pass has not stopped yet");
            return;
        }

        state.tag_cancel.store(false, Ordering::Relaxed);
        if afresh {
            let wiped = library(&handle).and_then(|held| {
                held.forget(&pictures).map_err(|e| e.to_string())?;
                Ok(())
            });
            if let Err(err) = wiped {
                *state.tagging.lock().unwrap() = false;
                let _ = handle.emit("tags:error", err);
                return;
            }
        }

        let result = tag_everything(&handle, &dir, pictures, rating, &state.tag_cancel);
        *state.tagging.lock().unwrap() = false;

        match result {
            Ok(seen) => {
                let _ = handle.emit("tags:done", seen);
            }
            Err(err) => {
                let _ = handle.emit("tags:error", err);
            }
        }
    });

    Ok(wanted)
}

fn tag_everything(
    app: &AppHandle,
    dir: &Path,
    pictures: Vec<PathBuf>,
    rating: bool,
    cancel: &AtomicBool,
) -> Result<usize, String> {
    let total = pictures.len();

    announce(app, total, "Loading the tagging model");
    let tagger = tagging::Tagger::load(dir).map_err(|e| e.to_string())?;

    let rater = if rating {
        announce(app, total, "Loading the rating model");
        Some(quality::Rater::load(dir).map_err(|e| e.to_string())?)
    } else {
        None
    };

    announce(app, total, "Taking in what earlier passes found");
    let stored = library(app)?;

    let stamp = tags::projection_stamp(&tagging::VOCABULARY);
    let model = tagging::stamp();
    if !stored
        .projection_matches(&stamp)
        .map_err(|e| e.to_string())?
    {
        announce(app, total, "Putting the words on pictures already seen");
        stored
            .reproject(&stamp, &|vector| tagger.project(vector))
            .map_err(|e| e.to_string())?;
    }

    announce(app, total, "Working out what is left to do");
    let mut known = stored.known().map_err(|e| e.to_string())?;
    let throttle = Mutex::new(Instant::now() - PROGRESS_INTERVAL);
    let mut since_written = 0usize;
    let mut fresh: HashMap<String, SightingView> = HashMap::new();
    let mut waiting: Vec<(String, tags::Sighting)> = Vec::new();
    let mut rated: Vec<(String, Option<f32>)> = Vec::new();

    for (seen, source) in pictures.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        let Some(digest) = stored.identify(&source).map_err(|e| e.to_string())? else {
            continue;
        };

        let mut looked = known.looked_with(&digest, &model);
        let wants_rating = rating && !known.rated(&digest);
        if looked && !wants_rating {
            continue;
        }

        emit_throttled(
            app,
            "tags:progress",
            &TagProgressView {
                done: seen + 1,
                total,
                current: Some(source.to_string_lossy().into_owned()),
                note: None,
            },
            &throttle,
        );

        let mut held = None;
        if !looked {
            held = stored
                .claim(&source, &digest, &|vector| tagger.project(vector))
                .map_err(|e| e.to_string())?
                .inspect(|_| known.saw(&digest));
            looked = held.is_some();
        }

        let score = if wants_rating {
            held.as_ref()
                .and_then(|taken| taken.quality)
                .filter(|score| score.is_finite())
                .or_else(|| {
                    rater
                        .as_ref()
                        .and_then(|rater| rater.score(&source).ok())
                        .filter(|score| score.is_finite())
                })
        } else {
            None
        };

        if looked {
            let tags = match held {
                Some(taken) => taken.tags,
                None => stored
                    .get(&digest)
                    .map_err(|e| e.to_string())?
                    .map(|kept| kept.tags)
                    .unwrap_or_default(),
            };
            fresh.insert(
                source.to_string_lossy().into_owned(),
                SightingView {
                    tags,
                    quality: score,
                },
            );
            rated.push((digest, score));
        } else {
            match tagger.look(&source) {
                Ok(seen) => {
                    let tags = seen.tags;
                    fresh.insert(
                        source.to_string_lossy().into_owned(),
                        SightingView {
                            tags: tags.clone(),
                            quality: score,
                        },
                    );
                    waiting.push((
                        digest,
                        tags::Sighting {
                            tags,
                            vector: seen.vector,
                            quality: score,
                            model: Some(model.clone()),
                            faces: None,
                        },
                    ));
                }
                Err(_) => continue,
            }
        }
        since_written += 1;

        if since_written >= TAG_BATCH {
            flush(&stored, &mut waiting, &mut rated)?;
            since_written = 0;
            let _ = app.emit("tags:seen", &fresh);
            fresh.clear();
        }
    }

    flush(&stored, &mut waiting, &mut rated)?;
    if !fresh.is_empty() {
        let _ = app.emit("tags:seen", &fresh);
    }
    stored.mark_projection(&stamp).map_err(|e| e.to_string())?;
    stored.len().map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct FaceStatus {
    pub built_in: bool,
    pub present: bool,
    pub credit: String,
    pub knowing_credit: String,
}

fn face_models(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .resolve("models", tauri::path::BaseDirectory::Resource)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn face_status(app: AppHandle) -> FaceStatus {
    let present = face_models(&app)
        .map(|dir| faces::installed(&dir))
        .unwrap_or(false);
    FaceStatus {
        built_in: faces::built_in(),
        present,
        credit: faces::CREDIT.to_string(),
        knowing_credit: faces::KNOWING_CREDIT.to_string(),
    }
}

#[tauri::command]
pub fn list_names(app: AppHandle) -> Result<Vec<NameCount>, String> {
    let store = library(&app)?;
    Ok(store
        .names()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(name, count)| NameCount { name, count })
        .collect())
}

#[derive(Debug, Clone, Serialize)]
pub struct NameCount {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct NamedFaces {
    pub named: usize,
    pub alike: usize,
}

#[tauri::command]
pub fn name_face(
    app: AppHandle,
    source: String,
    ord: usize,
    label: Option<String>,
    spread: bool,
) -> Result<NamedFaces, String> {
    let store = library(&app)?;
    let path = PathBuf::from(&source);
    let digest = store
        .digest_of(&path)
        .map_err(|e| e.to_string())?
        .ok_or("that picture has not been looked at yet")?;

    let wanted = label.as_deref().map(str::trim).filter(|it| !it.is_empty());
    store
        .name_face(&digest, ord, wanted)
        .map_err(|e| e.to_string())?;

    let mut alike = 0usize;
    if let (true, Some(wanted)) = (spread, wanted) {
        let vector = store
            .portraits_of(&digest)
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|held| held.ord == ord)
            .map(|held| held.vector)
            .unwrap_or_default();
        if !vector.is_empty() {
            alike = store
                .name_everything_like(&vector, wanted, SAME_PERSON)
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(NamedFaces {
        named: usize::from(wanted.is_some()),
        alike,
    })
}

#[tauri::command]
pub fn list_faces(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<HashMap<String, Vec<tags::Spot>>, String> {
    if planned(&app).is_none() {
        return Ok(HashMap::new());
    }
    let pictures = pictures_of(&state)?;
    let store = library(&app)?;

    let mut held = HashMap::new();
    for source in pictures {
        let Some(digest) = store.digest_of(&source).map_err(|e| e.to_string())? else {
            continue;
        };
        if let Some(spots) = store.faces_of(&digest).map_err(|e| e.to_string())? {
            held.insert(source.to_string_lossy().into_owned(), spots);
        }
    }
    Ok(held)
}

#[tauri::command]
pub fn cancel_face_hunt(state: State<'_, AppState>) {
    state.face_cancel.store(true, Ordering::Relaxed);
}

#[tauri::command]
pub fn start_face_hunt(
    app: AppHandle,
    state: State<'_, AppState>,
    afresh: bool,
) -> Result<usize, String> {
    if !faces::built_in() {
        return Err("this build was made without the face detector".into());
    }
    if !faces::installed(&face_models(&app)?) {
        return Err("the face models are not installed beside the program".into());
    }
    if planned(&app).is_none() {
        return Err("run a scan first".into());
    }

    let pictures = pictures_of(&state)?;
    let wanted = pictures.len();

    {
        let mut running = state.hunting_faces.lock().unwrap();
        if *running {
            return Err("the faces are already being looked for".into());
        }
        *running = true;
    }
    state.face_cancel.store(false, Ordering::Relaxed);

    let handle = app.clone();
    std::thread::spawn(move || {
        let state = handle.state::<AppState>();
        let result = hunt_faces(&handle, pictures, afresh, &state.face_cancel);
        *state.hunting_faces.lock().unwrap() = false;

        match result {
            Ok(seen) => {
                let _ = handle.emit("faces:done", seen);
            }
            Err(err) => {
                let _ = handle.emit("faces:error", err);
            }
        }
    });

    Ok(wanted)
}

fn hunt_faces(
    app: &AppHandle,
    pictures: Vec<PathBuf>,
    afresh: bool,
    cancel: &AtomicBool,
) -> Result<usize, String> {
    let total = pictures.len();
    let note = |note: &str| {
        let _ = app.emit(
            "faces:progress",
            &TagProgressView {
                done: 0,
                total,
                current: None,
                note: Some(note.to_string()),
            },
        );
    };

    note("Waking the face detector");
    let dir = face_models(app)?;
    let finder = faces::Finder::load(&faces::finding_at(&dir)).map_err(|e| e.to_string())?;
    let knowing = faces::Recogniser::load(&faces::knowing_at(&dir)).map_err(|e| e.to_string())?;
    let stored = library(app)?;

    let crooked = forget_crooked_faces(&stored, &pictures)?;
    if crooked > 0 {
        note(&format!(
            "Looking again at {crooked} pictures whose faces were found the wrong way round"
        ));
    }

    note("Working out what is left to do");
    let known = stored.known().map_err(|e| e.to_string())?;
    let throttle = Mutex::new(Instant::now() - PROGRESS_INTERVAL);
    let mut waiting: Vec<(String, Vec<tags::Found>)> = Vec::new();
    let mut fresh: HashMap<String, Vec<tags::Spot>> = HashMap::new();
    let mut found = 0usize;

    for (seen, source) in pictures.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        let Some(digest) = stored.identify(&source).map_err(|e| e.to_string())? else {
            continue;
        };
        if !afresh && known.faced(&digest) {
            continue;
        }

        emit_throttled(
            app,
            "faces:progress",
            &TagProgressView {
                done: seen + 1,
                total,
                current: Some(source.to_string_lossy().into_owned()),
                note: None,
            },
            &throttle,
        );

        let Ok(seen) = faces::study(&finder, Some(&knowing), &source) else {
            continue;
        };
        found += seen.len();
        fresh.insert(
            source.to_string_lossy().into_owned(),
            seen.iter().map(|held| held.spot.clone()).collect(),
        );
        waiting.push((digest, seen));

        if waiting.len() >= TAG_BATCH {
            stored
                .set_faces_many(waiting.drain(..))
                .map_err(|e| e.to_string())?;
            let _ = app.emit("faces:seen", &fresh);
            fresh.clear();
        }
    }

    stored
        .set_faces_many(waiting.drain(..))
        .map_err(|e| e.to_string())?;
    if !fresh.is_empty() {
        let _ = app.emit("faces:seen", &fresh);
    }
    Ok(found)
}

fn forget_crooked_faces(stored: &tags::Store, pictures: &[PathBuf]) -> Result<usize, String> {
    let stamped = stored.stamp_of(FACE_FRAME).map_err(|e| e.to_string())?;
    if stamped.as_deref() == Some(UPRIGHT_FRAME) {
        return Ok(0);
    }

    let crooked: Vec<String> = pictures
        .iter()
        .filter(|path| {
            Transform::for_orientation(rotate::read_orientation(path)) != Transform::None
        })
        .filter_map(|path| stored.identify(path).ok().flatten())
        .collect();
    let forgotten = stored.forget_faces(&crooked).map_err(|e| e.to_string())?;
    stored
        .set_stamp(FACE_FRAME, UPRIGHT_FRAME)
        .map_err(|e| e.to_string())?;
    Ok(forgotten)
}

fn flush(
    stored: &tags::Store,
    waiting: &mut Vec<(String, tags::Sighting)>,
    rated: &mut Vec<(String, Option<f32>)>,
) -> Result<(), String> {
    stored
        .set_many(waiting.drain(..))
        .map_err(|e| e.to_string())?;
    stored
        .set_quality_many(rated.drain(..))
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn search_pictures(
    app: AppHandle,
    state: State<'_, AppState>,
    words: String,
) -> Result<Vec<TagHit>, String> {
    if planned(&app).is_none() {
        return Ok(Vec::new());
    }
    if words.trim().is_empty() {
        return Ok(Vec::new());
    }

    let pictures = pictures_of(&state)?;
    let dir = models_directory(&app)?;

    tauri::async_runtime::spawn_blocking(move || {
        let stored = library(&app)?;
        let wanted = if cfg!(feature = "tagging") && tagging::installed(&dir) {
            tagging::Tagger::load(&dir)
                .and_then(|tagger| tagger.phrase(&words))
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        Ok(stored
            .search(&wanted, &words, &pictures)
            .map_err(|e| e.to_string())?
            .into_iter()
            .take(SEARCH_LIMIT)
            .map(|(source, score)| TagHit {
                source: source.to_string_lossy().into_owned(),
                score,
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn pictures_like(
    app: AppHandle,
    state: State<'_, AppState>,
    source: PathBuf,
) -> Result<Vec<TagHit>, String> {
    if planned(&app).is_none() {
        return Ok(Vec::new());
    }

    let pictures = pictures_of(&state)?;

    tauri::async_runtime::spawn_blocking(move || {
        let stored = library(&app)?;
        let Some(seen) = stored.at(&source).map_err(|e| e.to_string())? else {
            return Ok(Vec::new());
        };
        if seen.vector.is_empty() {
            return Ok(Vec::new());
        }

        Ok(stored
            .search(&seen.vector, "", &pictures)
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|(found, _)| found != &source)
            .take(SEARCH_LIMIT)
            .map(|(found, score)| TagHit {
                source: found.to_string_lossy().into_owned(),
                score,
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn is_picture(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some(
            "jpg"
                | "jpeg"
                | "jpe"
                | "png"
                | "webp"
                | "bmp"
                | "tif"
                | "tiff"
                | "gif"
                | "heic"
                | "heif"
                | "hif"
        )
    )
}

fn view(
    entry: &PlanEntry,
    root: &Path,
    journal: &std::collections::HashMap<PathBuf, Outcome>,
    overrides: &Overrides,
    rotations: &Rotations,
) -> EntryView {
    EntryView {
        name: entry
            .destination
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        folder: relative_folder(entry, root),
        taken: format_time(entry.taken),
        taken_epoch: epoch(entry.taken),
        provider: entry.provider.label().to_string(),
        provider_info: entry.provider_info.clone(),
        size: entry.size,
        destination_exists: entry.destination.is_absolute() && entry.destination.exists(),
        outcome: journal.get(&entry.source).map(outcome_label),
        candidates: entry
            .candidates
            .iter()
            .map(|c| CandidateView {
                provider: c.provider.label().to_string(),
                provider_info: c.info.clone(),
                taken: format_time(c.taken),
                taken_epoch: epoch(c.taken),
            })
            .collect(),
        flags: entry.flags.iter().map(flag_view).collect(),
        confidence: suspect::confidence(&entry.candidates, &entry.flags)
            .label()
            .to_string(),
        override_origin: overrides.get(&entry.source).map(|o| o.origin.describe()),
        orientation: entry.orientation,
        rotate: rotate_label(entry.rotate).to_string(),
        rotate_by_hand: rotations.get(&entry.source).is_some(),
        rotate_lossless: rotate::lossless_extension(&entry.source),
        reencode: entry.reencode,
        subject: entry.subject.clone(),
        tags: entry.tags.clone(),
        caption: entry.caption.clone(),
        source: entry.source.clone(),
        destination: entry.destination.clone(),
    }
}

fn flag_view(flag: &Flag) -> FlagView {
    FlagView {
        kind: flag_kind(flag).to_string(),
        description: flag.describe(),
        hard: flag.severity() == suspect::Severity::Hard,
    }
}

fn flag_kind(flag: &Flag) -> &'static str {
    match flag {
        Flag::CameraEpoch => "camera_epoch",
        Flag::FutureDate => "future_date",
        Flag::TakenAfterFileWrite => "taken_after_file_write",
        Flag::ProviderSpread { .. } => "provider_spread",
        Flag::ClockResetRun { .. } => "clock_reset_run",
        Flag::IdenticalTimestampCluster { .. } => "identical_timestamp_cluster",
        Flag::SequenceOutlier { .. } => "sequence_outlier",
        Flag::FarFromNeighbours { .. } => "far_from_neighbours",
        Flag::TimezoneShift { .. } => "timezone_shift",
    }
}

fn is_cross_file(flag: &Flag) -> bool {
    matches!(
        flag,
        Flag::ClockResetRun { .. }
            | Flag::IdenticalTimestampCluster { .. }
            | Flag::SequenceOutlier { .. }
            | Flag::FarFromNeighbours { .. }
    )
}

fn format_time(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn epoch(value: NaiveDateTime) -> i64 {
    value.and_utc().timestamp()
}

fn parse_manual(text: &str) -> Result<NaiveDateTime, String> {
    const FORMATS: [&str; 4] = [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
    ];
    for format in FORMATS {
        if let Ok(value) = NaiveDateTime::parse_from_str(text, format) {
            return Ok(value);
        }
    }
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
        .map_err(|_| format!("could not read the date \"{text}\""))
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

fn adopt(app: &AppHandle, plan_path: PathBuf, mut plan: Plan) -> Result<PlanSummary, String> {
    let journal = copy::read_journal(&copy::journal_path(&plan_path)).unwrap_or_default();
    let sidecar = overrides::overrides_path(&plan_path);
    let applied = overrides::read(&sidecar).map_err(|e| {
        format!(
            "could not read your date corrections at {}: {e}",
            sidecar.display()
        )
    })?;
    overrides::apply(&mut plan, &applied).map_err(|e| e.to_string())?;

    let turns_sidecar = overrides::rotations_path(&plan_path);
    let turns = overrides::read_rotations(&turns_sidecar).map_err(|e| {
        format!(
            "could not read your rotation corrections at {}: {e}",
            turns_sidecar.display()
        )
    })?;
    overrides::apply_rotations(&mut plan, &turns);
    annotate(&mut plan, &applied);

    let summary = summarise(app, &plan_path, &plan, &journal);

    let sidecar_plan = plan_path.clone();
    let state = app.state::<AppState>();
    let mut session = state.session.lock().unwrap();
    session.plan_path = Some(plan_path);
    session.plan = Some(plan);
    session.journal = journal;
    session.overrides = applied;
    session.rotations = turns;
    session.excluded =
        overrides::read_excluded(&overrides::excluded_path(&sidecar_plan)).unwrap_or_default();

    Ok(summary)
}

fn annotate(plan: &mut Plan, applied: &Overrides) {
    for entry in &mut plan.entries {
        entry.flags.retain(|f| !is_cross_file(f));
    }

    let facts: Vec<EntryFacts<'_>> = plan
        .entries
        .iter()
        .map(|entry| EntryFacts {
            source: &entry.source,
            taken: entry.taken,
            provider: entry.provider,
            filesystem: entry.filesystem_time(),
        })
        .collect();
    let extra = suspect::cross_file_flags(&facts);
    drop(facts);

    for (entry, flags) in plan.entries.iter_mut().zip(extra) {
        if applied.get(&entry.source).is_some() {
            entry.flags.clear();
            continue;
        }
        entry.flags.extend(flags);
    }
}

fn reload(app: &AppHandle) -> Result<(), String> {
    let plan_path = {
        let state = app.state::<AppState>();
        let session = state.session.lock().unwrap();
        session.plan_path.clone()
    };
    let plan_path = plan_path.ok_or("no plan is open")?;
    let plan = read_plan(&plan_path).map_err(|e| e.to_string())?;
    adopt(app, plan_path, plan)?;
    Ok(())
}

fn store(app: &AppHandle, applied: &Overrides) -> Result<(), String> {
    let plan_path = {
        let state = app.state::<AppState>();
        let session = state.session.lock().unwrap();
        session.plan_path.clone()
    };
    let plan_path = plan_path.ok_or("no plan is open")?;
    overrides::write(&overrides::overrides_path(&plan_path), applied).map_err(|e| e.to_string())
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
        .map(|e| relative_folder(e, plan.header.root()))
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

#[cfg(feature = "screenshot")]
#[tauri::command]
pub async fn save_screenshot(window: tauri::Window) -> Result<String, String> {
    let outer_at = window.outer_position().map_err(|e| e.to_string())?;
    let outer_size = window.outer_size().map_err(|e| e.to_string())?;
    let inner_at = window.inner_position().map_err(|e| e.to_string())?;
    let inner_size = window.inner_size().map_err(|e| e.to_string())?;
    let outer = crate::shot::Rect {
        x: outer_at.x,
        y: outer_at.y,
        width: outer_size.width,
        height: outer_size.height,
    };
    let inner = crate::shot::Rect {
        x: inner_at.x,
        y: inner_at.y,
        width: inner_size.width,
        height: inner_size.height,
    };
    let taken = tauri::async_runtime::spawn_blocking(move || crate::shot::take(outer, inner))
        .await
        .map_err(|e| e.to_string())?;
    match &taken {
        Ok(path) => eprintln!("screenshot saved: {}", path.display()),
        Err(e) => eprintln!("screenshot failed: {e}"),
    }
    taken.map(|path| path.display().to_string())
}

#[cfg(not(feature = "screenshot"))]
#[tauri::command]
pub async fn save_screenshot() -> Result<String, String> {
    Err("screenshots are only in the development build".to_string())
}

#[cfg(test)]
mod crooked_tests {
    use super::*;
    use eonsort_core::tags::{Found, Spot};

    fn room(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("eonsort-crooked-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn picture(dir: &Path, name: &str, orientation: u16) -> PathBuf {
        let mut body = Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            8,
            6,
            image::Rgb([12, 90, 200]),
        ))
        .write_to(
            &mut std::io::Cursor::new(&mut body),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
        let path = dir.join(name);
        std::fs::write(&path, exif_app1(&body, orientation)).unwrap();
        path
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
        app1.extend_from_slice(b"Exif  ");
        app1.extend_from_slice(&tiff);

        let mut out = Vec::new();
        out.extend_from_slice(&jpeg[0..2]);
        out.extend_from_slice(&[0xFF, 0xE1]);
        out.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
        out.extend_from_slice(&app1);
        out.extend_from_slice(&jpeg[2..]);
        out
    }

    fn face() -> Found {
        Found {
            spot: Spot {
                x: 0.1,
                y: 0.2,
                width: 0.3,
                height: 0.4,
                score: 0.9,
                label: None,
            },
            vector: vec![1.0, 0.0],
        }
    }

    fn stored_with_faces(dir: &Path, pictures: &[PathBuf]) -> tags::Store {
        let store = tags::Store::open(&dir.join("sightings.db")).unwrap();
        let found: Vec<(String, Vec<Found>)> = pictures
            .iter()
            .map(|path| (store.identify(path).unwrap().unwrap(), vec![face()]))
            .collect();
        store.set_faces_many(found.into_iter()).unwrap();
        store
    }

    #[test]
    fn faces_found_on_a_sideways_picture_are_looked_for_again() {
        let dir = room("sideways");
        let upright = picture(&dir, "upright.jpg", 1);
        let sideways = picture(&dir, "sideways.jpg", 6);
        let pictures = vec![upright.clone(), sideways.clone()];
        let store = stored_with_faces(&dir, &pictures);

        assert_eq!(forget_crooked_faces(&store, &pictures), Ok(1));

        let known = store.known().unwrap();
        let name = |path: &Path| store.identify(path).unwrap().unwrap();
        assert!(known.faced(&name(&upright)), "the upright one was cleared");
        assert!(!known.faced(&name(&sideways)), "the sideways one was kept");
    }

    #[test]
    fn the_pictures_are_only_looked_for_again_once() {
        let dir = room("once");
        let pictures = vec![picture(&dir, "sideways.jpg", 6)];
        let store = stored_with_faces(&dir, &pictures);

        assert_eq!(forget_crooked_faces(&store, &pictures), Ok(1));

        store
            .set_faces_many(
                vec![(store.identify(&pictures[0]).unwrap().unwrap(), vec![face()])].into_iter(),
            )
            .unwrap();
        assert_eq!(forget_crooked_faces(&store, &pictures), Ok(0));
        assert!(store
            .known()
            .unwrap()
            .faced(&store.identify(&pictures[0]).unwrap().unwrap()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use eonsort_core::providers::Provider;

    #[test]
    #[ignore = "needs a desktop recycle bin, so not on a build runner"]
    fn a_file_really_does_reach_the_recycle_bin() {
        let dir = std::env::temp_dir().join(format!("eonsort-trash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let doomed = dir.join("extra.jpg");
        std::fs::write(&doomed, b"a duplicate").unwrap();

        trash::delete(&doomed).expect("the recycle bin refused the file");
        assert!(!doomed.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_copy_nearest_the_top_of_the_tree_is_the_one_kept() {
        let sources = vec![
            PathBuf::from("/photos/backup/old/deep/a.jpg"),
            PathBuf::from("/photos/a.jpg"),
            PathBuf::from("/photos/backup/a.jpg"),
        ];
        let ranked = keeper_first(&sources);
        assert_eq!(ranked[0], PathBuf::from("/photos/a.jpg"));
        assert_eq!(ranked.len(), sources.len());
    }

    #[test]
    fn copies_at_the_same_depth_are_kept_in_a_settled_order() {
        let sources = vec![
            PathBuf::from("/photos/zebra.jpg"),
            PathBuf::from("/photos/apple.jpg"),
        ];
        assert_eq!(
            keeper_first(&sources)[0],
            PathBuf::from("/photos/apple.jpg")
        );
        assert_eq!(
            keeper_first(&sources),
            keeper_first(&keeper_first(&sources))
        );
    }

    #[test]
    fn no_copy_is_lost_on_the_way_through_the_ranking() {
        let sources = vec![
            PathBuf::from("/a/b/c/one.jpg"),
            PathBuf::from("/a/two.jpg"),
            PathBuf::from("/a/b/three.jpg"),
        ];
        let mut ranked = keeper_first(&sources);
        ranked.sort();
        let mut original = sources.clone();
        original.sort();
        assert_eq!(ranked, original);
    }

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
            ..PlanEntry::default()
        }
    }

    #[test]
    fn reads_the_date_formats_the_manual_field_can_produce() {
        let expected = NaiveDate::from_ymd_opt(2019, 7, 4)
            .unwrap()
            .and_hms_opt(10, 30, 0)
            .unwrap();
        assert_eq!(parse_manual("2019-07-04T10:30").unwrap(), expected);
        assert_eq!(parse_manual("2019-07-04T10:30:00").unwrap(), expected);
        assert_eq!(parse_manual("2019-07-04 10:30").unwrap(), expected);
        assert_eq!(
            parse_manual("2019-07-04").unwrap(),
            NaiveDate::from_ymd_opt(2019, 7, 4)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
        );
        assert!(parse_manual("last tuesday").is_err());
    }

    #[test]
    fn refuses_to_re_date_a_file_that_was_already_copied() {
        let source = PathBuf::from("/src/a.jpg");
        let mut journal = std::collections::HashMap::new();
        assert!(refuse_if_copied(&journal, &source).is_ok());

        journal.insert(
            source.clone(),
            Outcome::Copied {
                destination: PathBuf::from("/out/2003/01/a.jpg"),
            },
        );
        assert!(refuse_if_copied(&journal, &source).is_err());

        journal.insert(
            source.clone(),
            Outcome::Failed {
                error: "disk full".into(),
            },
        );
        assert!(refuse_if_copied(&journal, &source).is_ok());
    }

    #[test]
    fn separates_per_file_flags_from_folder_wide_ones() {
        assert!(!is_cross_file(&Flag::CameraEpoch));
        assert!(!is_cross_file(&Flag::ProviderSpread { days: 900 }));
        assert!(is_cross_file(&Flag::SequenceOutlier {
            days: 9,
            neighbour: "DSC_0201.jpg".to_string(),
            before: false,
        }));
        assert!(is_cross_file(&Flag::IdenticalTimestampCluster {
            files: 12
        }));
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
